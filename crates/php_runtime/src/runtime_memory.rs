//! Audited low-level memory primitives (ADR 0020).
//!
//! This is the only module in `php_runtime` allowed to use `unsafe`; every
//! unsafe surface here is recorded with its invariants in
//! `docs/performance/runtime-memory-safety-audit.md`. Public APIs are safe.
// Consumed by the PhpString migration (ADR 0020 stage 2); the primitives
// land first so the audit, tests, and Miri coverage exist before any
// caller depends on them.

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::NonNull;
use std::rc::Rc;

/// Marker for plain native ABI records whose all-zero bit pattern is valid.
///
/// # Safety
///
/// Implementors must be `Copy`, require no drop, and accept an all-zero bit
/// pattern as a valid value. This permits demand-zero virtual storage to be
/// exposed as a stable slice without eagerly initializing its full capacity.
#[allow(unsafe_code)]
pub unsafe trait NativeZeroed: Copy {}

// SAFETY: every bit pattern of these integer byte/word types is valid.
#[allow(unsafe_code)]
unsafe impl NativeZeroed for u8 {}
#[allow(unsafe_code)]
unsafe impl NativeZeroed for u32 {}
#[allow(unsafe_code)]
unsafe impl NativeZeroed for u64 {}

/// Stable, demand-zero native ABI arena.
///
/// On production Unix builds the address range is reserved with anonymous
/// `mmap`; physical pages are supplied only when generated code touches them.
/// Non-runtime/minimal builds use the allocator's zeroed storage while keeping
/// the same API. The allocation never moves.
pub struct StableNativeArena<T: NativeZeroed> {
    ptr: NonNull<T>,
    capacity: usize,
    bytes: usize,
    mapped: bool,
}

// SAFETY: the arena uniquely owns its allocation and moving that owner does
// not move the allocation itself. Request pooling transfers an arena only
// after all published native pointers have expired; `T: Send` prevents
// non-transferable payloads from entering the arena.
#[allow(unsafe_code)]
unsafe impl<T: NativeZeroed + Send> Send for StableNativeArena<T> {}

/// A diagnostics-only snapshot of one stable native arena.
///
/// The request owner supplies high-water state when it asks for this snapshot,
/// and resident pages are queried only while explicit runtime counters are
/// being materialized. Arena allocation itself never scans or commits unused
/// pages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StableNativeArenaUsage {
    pub reserved_bytes: usize,
    pub high_water_bytes: usize,
    pub resident_bytes: usize,
}

impl<T: NativeZeroed> StableNativeArena<T> {
    #[must_use]
    #[allow(unsafe_code)]
    pub fn new(capacity: usize) -> Self {
        if capacity == 0 {
            return Self::default();
        }
        let bytes = std::mem::size_of::<T>()
            .checked_mul(capacity)
            .expect("stable native arena capacity overflow");
        assert!(
            bytes != 0,
            "zero-sized native arena elements are unsupported"
        );

        #[cfg(all(unix, feature = "full-runtime"))]
        {
            let mut flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS;
            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                flags |= libc::MAP_NORESERVE;
            }
            // SAFETY: an anonymous private mapping owns `bytes` demand-zero
            // bytes and is released exactly once by Drop. Page alignment
            // satisfies the native ABI records used by the JIT.
            let address = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    bytes,
                    libc::PROT_READ | libc::PROT_WRITE,
                    flags,
                    -1,
                    0,
                )
            };
            if address == libc::MAP_FAILED {
                std::alloc::handle_alloc_error(
                    Layout::from_size_align(bytes, std::mem::align_of::<T>())
                        .expect("stable native arena layout"),
                );
            }
            let ptr = NonNull::new(address.cast::<T>()).expect("mmap returned null");
            return Self {
                ptr,
                capacity,
                bytes,
                mapped: true,
            };
        }

        #[cfg(not(all(unix, feature = "full-runtime")))]
        {
            let layout = Layout::from_size_align(bytes, std::mem::align_of::<T>())
                .expect("stable native arena layout");
            // SAFETY: `layout` is non-zero and valid. NativeZeroed guarantees
            // the initialized zero bytes form valid values.
            let address = unsafe { std::alloc::alloc_zeroed(layout) };
            let ptr = NonNull::new(address.cast::<T>())
                .unwrap_or_else(|| std::alloc::handle_alloc_error(layout));
            Self {
                ptr,
                capacity,
                bytes,
                mapped: false,
            }
        }
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub const fn reserved_bytes(&self) -> usize {
        self.bytes
    }

    /// Returns a diagnostics-only memory snapshot without touching unused
    /// arena pages. `initialized` is the owner's logical high-water element
    /// count, not the maximum capacity.
    #[must_use]
    pub fn usage(&self, initialized: usize) -> StableNativeArenaUsage {
        let initialized = initialized.min(self.capacity);
        let high_water_bytes = std::mem::size_of::<T>()
            .checked_mul(initialized)
            .expect("stable native arena initialized length overflow");
        StableNativeArenaUsage {
            reserved_bytes: self.bytes,
            high_water_bytes,
            resident_bytes: self.resident_bytes().unwrap_or(high_water_bytes),
        }
    }

    /// Counts resident pages in the arena's private mapping. This is only
    /// called by explicit diagnostic/profile collection; ordinary requests
    /// neither scan page state nor allocate the residency vector.
    #[cfg(all(unix, feature = "full-runtime"))]
    #[allow(unsafe_code)]
    fn resident_bytes(&self) -> Option<usize> {
        if self.bytes == 0 {
            return Some(0);
        }
        if !self.mapped {
            return None;
        }
        // SAFETY: sysconf has no memory-safety preconditions.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let page_size = usize::try_from(page_size).ok().filter(|size| *size != 0)?;
        let page_count = self.bytes.div_ceil(page_size);
        let mut residency = vec![0_u8; page_count];
        // SAFETY: mmap returned a page-aligned base covering `self.bytes`,
        // and `residency` contains one output byte per covered page.
        let status = unsafe {
            libc::mincore(
                self.ptr.as_ptr().cast(),
                self.bytes,
                residency.as_mut_ptr().cast(),
            )
        };
        if status != 0 {
            return None;
        }
        Some(
            residency
                .into_iter()
                .enumerate()
                .filter(|(_, state)| state & 1 != 0)
                .map(|(index, _)| {
                    self.bytes
                        .saturating_sub(index.saturating_mul(page_size))
                        .min(page_size)
                })
                .sum(),
        )
    }

    #[cfg(not(all(unix, feature = "full-runtime")))]
    fn resident_bytes(&self) -> Option<usize> {
        None
    }

    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Resets the initialized prefix to its demand-zero state without moving
    /// the arena or writing across every used element.
    ///
    /// The request owner must first release any native ownership represented
    /// by records in the prefix. These arenas contain only plain ABI records,
    /// so returning their pages to the kernel is then equivalent to filling
    /// the prefix with zeroes while also dropping its resident-memory cost.
    #[allow(unsafe_code)]
    pub fn discard_prefix(&mut self, initialized: usize) {
        let initialized = initialized.min(self.capacity);
        if initialized == 0 {
            return;
        }
        let bytes = std::mem::size_of::<T>()
            .checked_mul(initialized)
            .expect("stable native arena initialized length overflow");

        #[cfg(all(unix, feature = "full-runtime"))]
        if self.mapped {
            // SAFETY: the address is page-aligned because it came from mmap,
            // and `bytes` is bounded by the live mapping. MADV_DONTNEED keeps
            // the mapping and makes subsequent reads observe anonymous zero
            // pages. The kernel may round the length up to the final page;
            // bytes beyond `initialized` have never been published as live.
            let result =
                unsafe { libc::madvise(self.ptr.as_ptr().cast(), bytes, libc::MADV_DONTNEED) };
            if result == 0 {
                return;
            }
        }

        // Allocator-backed platforms do not offer a portable page-discard
        // primitive. Clear only the initialized prefix, never the arena's
        // maximum capacity.
        self[..initialized].fill(unsafe { std::mem::zeroed() });
    }
}

impl<T: NativeZeroed> Default for StableNativeArena<T> {
    fn default() -> Self {
        Self {
            ptr: NonNull::dangling(),
            capacity: 0,
            bytes: 0,
            mapped: false,
        }
    }
}

impl<T: NativeZeroed> Deref for StableNativeArena<T> {
    type Target = [T];

    #[allow(unsafe_code)]
    fn deref(&self) -> &Self::Target {
        // SAFETY: the stable allocation contains `capacity` demand-zero
        // NativeZeroed records and remains alive for this borrow.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.capacity) }
    }
}

impl<T: NativeZeroed> std::ops::DerefMut for StableNativeArena<T> {
    #[allow(unsafe_code)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the arena is exclusively borrowed and never reallocates.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.capacity) }
    }
}

impl<T: NativeZeroed> Drop for StableNativeArena<T> {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if self.capacity == 0 {
            return;
        }
        #[cfg(all(unix, feature = "full-runtime"))]
        if self.mapped {
            // SAFETY: this exact range was returned by mmap in `new` and has
            // not been unmapped or moved.
            unsafe {
                libc::munmap(self.ptr.as_ptr().cast(), self.bytes);
            }
            return;
        }
        let layout = Layout::from_size_align(self.bytes, std::mem::align_of::<T>())
            .expect("stable native arena layout");
        // SAFETY: the fallback allocation used this identical layout.
        unsafe { dealloc(self.ptr.as_ptr().cast::<u8>(), layout) };
    }
}

impl<T: NativeZeroed> std::fmt::Debug for StableNativeArena<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StableNativeArena")
            .field("capacity", &self.capacity)
            .field("reserved_bytes", &self.bytes)
            .field("demand_backed", &self.mapped)
            .finish()
    }
}

/// ABI-stable single-threaded shared storage used by native-visible PHP data.
/// The strong counter is the first field, so published native descriptors may
/// retain a COW snapshot with one direct integer update instead of calling
/// through Rust's opaque `Rc` implementation.
#[repr(C)]
struct SharedHeader<T> {
    strong: Cell<usize>,
    weak: Cell<usize>,
    value: ManuallyDrop<T>,
}

pub(crate) struct Shared<T> {
    ptr: NonNull<SharedHeader<T>>,
    _not_send: PhantomData<Rc<()>>,
}

pub(crate) struct WeakShared<T> {
    ptr: NonNull<SharedHeader<T>>,
    _not_send: PhantomData<Rc<()>>,
}

impl<T> Shared<T> {
    pub(crate) fn new(value: T) -> Self {
        let header = Box::new(SharedHeader {
            strong: Cell::new(1),
            // One implicit weak owner keeps the allocation alive while any
            // strong handle exists.
            weak: Cell::new(1),
            value: ManuallyDrop::new(value),
        });
        Self {
            ptr: NonNull::from(Box::leak(header)),
            _not_send: PhantomData,
        }
    }

    #[allow(unsafe_code)]
    fn header(&self) -> &SharedHeader<T> {
        // SAFETY: every strong handle owns one count in this live allocation.
        unsafe { self.ptr.as_ref() }
    }

    pub(crate) fn strong_count(&self) -> usize {
        self.header().strong.get()
    }

    pub(crate) fn downgrade(&self) -> WeakShared<T> {
        let weak = self.header().weak.get();
        if weak == usize::MAX {
            std::process::abort();
        }
        self.header().weak.set(weak + 1);
        WeakShared {
            ptr: self.ptr,
            _not_send: PhantomData,
        }
    }

    pub(crate) fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        if this.strong_count() != 1 {
            let separated = T::clone(this);
            *this = Self::new(separated);
        }
        // SAFETY: `this` owns the only strong reference after separation and
        // is exclusively borrowed for the returned lifetime.
        #[allow(unsafe_code)]
        unsafe {
            &mut *(&mut (*this.ptr.as_ptr()).value as *mut ManuallyDrop<T> as *mut T)
        }
    }

    pub(crate) fn try_unwrap(this: Self) -> Result<T, Self> {
        if this.strong_count() != 1 {
            return Err(this);
        }
        let ptr = this.ptr;
        std::mem::forget(this);
        // SAFETY: the forgotten handle was the sole strong owner. Move the
        // value once, retire the implicit weak owner, and free only when no
        // explicit weak handle remains.
        #[allow(unsafe_code)]
        unsafe {
            (*ptr.as_ptr()).strong.set(0);
            let value = ManuallyDrop::take(&mut (*ptr.as_ptr()).value);
            let weak = (*ptr.as_ptr()).weak.get() - 1;
            (*ptr.as_ptr()).weak.set(weak);
            if weak == 0 {
                drop(Box::from_raw(ptr.as_ptr()));
            }
            Ok(value)
        }
    }

    /// Address of the stable native-visible strong count.
    pub(crate) fn strong_count_address(&self) -> usize {
        std::ptr::from_ref(&self.header().strong) as usize
    }

    /// Address of the immutable storage payload.
    #[allow(unsafe_code)]
    pub(crate) fn clone_from_strong_count_address(address: usize) -> Option<Self> {
        let ptr = NonNull::new(address as *mut SharedHeader<T>)?;
        // SAFETY: callers obtain this address from `strong_count_address` on
        // the same concrete `Shared<T>` ABI. A non-zero strong count keeps the
        // initialized value alive while we acquire another owner.
        unsafe {
            let strong = (*ptr.as_ptr()).strong.get();
            if strong == 0 {
                return None;
            }
            if strong == usize::MAX {
                std::process::abort();
            }
            (*ptr.as_ptr()).strong.set(strong + 1);
        }
        Some(Self {
            ptr,
            _not_send: PhantomData,
        })
    }

    #[allow(unsafe_code)]
    pub(crate) fn from_retained_strong_count_address(address: usize) -> Option<Self> {
        let ptr = NonNull::new(address as *mut SharedHeader<T>)?;
        // SAFETY: the native slot transferred one already-retained strong
        // owner to this handle. No counter update is required here.
        if unsafe { (*ptr.as_ptr()).strong.get() } == 0 {
            return None;
        }
        Some(Self {
            ptr,
            _not_send: PhantomData,
        })
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        let strong = self.header().strong.get();
        if strong == usize::MAX {
            std::process::abort();
        }
        self.header().strong.set(strong + 1);
        Self {
            ptr: self.ptr,
            _not_send: PhantomData,
        }
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    #[allow(unsafe_code)]
    fn deref(&self) -> &Self::Target {
        // SAFETY: a live strong handle guarantees an initialized value.
        unsafe { &*(&(*self.ptr.as_ptr()).value as *const ManuallyDrop<T> as *const T) }
    }
}

impl<T> AsRef<T> for Shared<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Shared")
            .field("strong", &self.strong_count())
            .field("value", &self.deref())
            .finish()
    }
}

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}

impl<T: Eq> Eq for Shared<T> {}

impl<T> Drop for Shared<T> {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        let header = self.header();
        let strong = header.strong.get();
        if strong > 1 {
            header.strong.set(strong - 1);
            return;
        }
        // SAFETY: this is the last strong owner. Drop the value exactly once,
        // then retire the implicit weak owner and possibly the allocation.
        unsafe {
            header.strong.set(0);
            ManuallyDrop::drop(&mut (*self.ptr.as_ptr()).value);
            let weak = header.weak.get() - 1;
            header.weak.set(weak);
            if weak == 0 {
                drop(Box::from_raw(self.ptr.as_ptr()));
            }
        }
    }
}

impl<T> WeakShared<T> {
    #[allow(unsafe_code)]
    fn header(&self) -> &SharedHeader<T> {
        // SAFETY: this weak handle owns one weak count in the allocation.
        unsafe { self.ptr.as_ref() }
    }

    pub(crate) fn strong_count(&self) -> usize {
        self.header().strong.get()
    }

    pub(crate) fn upgrade(&self) -> Option<Shared<T>> {
        let strong = self.strong_count();
        if strong == 0 {
            return None;
        }
        if strong == usize::MAX {
            std::process::abort();
        }
        self.header().strong.set(strong + 1);
        Some(Shared {
            ptr: self.ptr,
            _not_send: PhantomData,
        })
    }
}

impl<T> Clone for WeakShared<T> {
    fn clone(&self) -> Self {
        let weak = self.header().weak.get();
        if weak == usize::MAX {
            std::process::abort();
        }
        self.header().weak.set(weak + 1);
        Self {
            ptr: self.ptr,
            _not_send: PhantomData,
        }
    }
}

impl<T> Drop for WeakShared<T> {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        let weak = self.header().weak.get() - 1;
        self.header().weak.set(weak);
        if weak == 0 {
            debug_assert_eq!(self.header().strong.get(), 0);
            // SAFETY: the final weak and all strong owners are gone.
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) };
        }
    }
}

impl<T> std::fmt::Debug for WeakShared<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WeakShared")
            .field("strong", &self.strong_count())
            .finish()
    }
}

/// Header of a [`CompactBytes`] allocation; the bytes follow directly
/// after it in the same heap block.
#[repr(C)]
struct Header {
    /// Single-threaded reference count; starts at one.
    refcount: Cell<usize>,
    /// Cached FNV hash of the bytes; `0` means "not computed".
    hash: Cell<u64>,
    /// Interned symbol identity; `u64::MAX` means "none".
    symbol: Cell<u64>,
    /// Number of bytes trailing the header.
    len: usize,
}

/// Marker symbol value meaning "no interned identity".
const NO_SYMBOL: u64 = u64::MAX;

/// Single-allocation shared byte storage: refcount, cached hash, interned
/// symbol, and length live in one header directly in front of the bytes,
/// so a string is one allocation and one pointer dereference.
///
/// Semantically `Rc<[u8]>` plus two lazily-cached header cells. `!Send`
/// and `!Sync` like `Rc`.
#[allow(dead_code)]
pub struct CompactBytes {
    ptr: NonNull<Header>,
    /// Pins `!Send`/`!Sync`: the refcount is a plain `Cell`.
    _not_send: PhantomData<Rc<()>>,
}

#[allow(dead_code)]
impl CompactBytes {
    fn layout(len: usize) -> Layout {
        // SAFETY-ADJACENT: the identical layout must be recomputed in
        // `drop`; both sides derive it from the header type and `len`.
        let header = Layout::new::<Header>();
        let bytes = match Layout::array::<u8>(len) {
            Ok(layout) => layout,
            Err(_) => handle_alloc_error(header),
        };
        match header.extend(bytes) {
            Ok((layout, _)) => layout.pad_to_align(),
            Err(_) => handle_alloc_error(header),
        }
    }

    /// Allocates one block holding the header and a copy of `bytes`.
    #[must_use]
    #[allow(unsafe_code)]
    pub fn from_slice(bytes: &[u8]) -> Self {
        let len = bytes.len();
        let layout = Self::layout(len);
        // SAFETY: the layout has non-zero size (the header is non-empty),
        // and an allocation failure diverts to `handle_alloc_error`.
        let raw = unsafe { alloc(layout) };
        let Some(ptr) = NonNull::new(raw.cast::<Header>()) else {
            handle_alloc_error(layout);
        };
        // SAFETY: `ptr` is freshly allocated with space for `Header`
        // followed by `len` bytes; writing the header initializes the
        // block, and the byte copy targets the tail region of the same
        // allocation, which cannot overlap the source slice.
        unsafe {
            ptr.as_ptr().write(Header {
                refcount: Cell::new(1),
                hash: Cell::new(0),
                symbol: Cell::new(NO_SYMBOL),
                len,
            });
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.as_ptr().add(1).cast::<u8>(), len);
        }
        Self {
            ptr,
            _not_send: PhantomData,
        }
    }

    /// Allocates one block holding the header and the concatenation of
    /// `parts`, without an intermediate growable buffer. This is the
    /// string-concatenation primitive: the result length is known up
    /// front, so the bytes are copied straight into their final home.
    #[must_use]
    #[allow(unsafe_code)]
    pub fn from_parts(parts: &[&[u8]]) -> Self {
        let len = parts.iter().map(|part| part.len()).sum();
        let layout = Self::layout(len);
        // SAFETY: the layout has non-zero size (the header is non-empty),
        // and an allocation failure diverts to `handle_alloc_error`.
        let raw = unsafe { alloc(layout) };
        let Some(ptr) = NonNull::new(raw.cast::<Header>()) else {
            handle_alloc_error(layout);
        };
        // SAFETY: `ptr` is freshly allocated with space for `Header`
        // followed by `len` bytes; writing the header initializes the
        // block, and each part copy targets a disjoint window of the tail
        // region (offsets advance by exactly the previous parts' lengths,
        // and their sum is `len`), which cannot overlap the source slices.
        unsafe {
            ptr.as_ptr().write(Header {
                refcount: Cell::new(1),
                hash: Cell::new(0),
                symbol: Cell::new(NO_SYMBOL),
                len,
            });
            let mut tail = ptr.as_ptr().add(1).cast::<u8>();
            for part in parts {
                std::ptr::copy_nonoverlapping(part.as_ptr(), tail, part.len());
                tail = tail.add(part.len());
            }
        }
        Self {
            ptr,
            _not_send: PhantomData,
        }
    }

    #[allow(unsafe_code)]
    fn header(&self) -> &Header {
        // SAFETY: `ptr` always points at the live header of an allocation
        // this handle keeps alive through its refcount.
        unsafe { self.ptr.as_ref() }
    }

    /// The stored bytes.
    #[must_use]
    #[allow(unsafe_code)]
    pub fn as_bytes(&self) -> &[u8] {
        let len = self.header().len;
        // SAFETY: the tail of the allocation holds exactly `len`
        // initialized bytes written in `from_slice`, immutable for the
        // allocation's lifetime.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr().add(1).cast::<u8>(), len) }
    }

    /// Number of stored bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.header().len
    }

    /// True when no bytes are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.header().len == 0
    }

    /// True when this handle is the only owner.
    #[must_use]
    pub fn is_unique(&self) -> bool {
        self.header().refcount.get() == 1
    }

    /// Cached hash; `0` means "not computed".
    #[must_use]
    pub fn hash(&self) -> u64 {
        self.header().hash.get()
    }

    /// Stores the cached hash.
    pub fn set_hash(&self, hash: u64) {
        self.header().hash.set(hash);
    }

    /// Cached interned symbol.
    #[must_use]
    pub fn symbol(&self) -> Option<u64> {
        let raw = self.header().symbol.get();
        (raw != NO_SYMBOL).then_some(raw)
    }

    /// Stores the interned symbol.
    pub fn set_symbol(&self, symbol: Option<u64>) {
        self.header().symbol.set(symbol.unwrap_or(NO_SYMBOL));
    }

    /// True when both handles share one allocation.
    #[must_use]
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.ptr == b.ptr
    }

    /// Process-local allocation identity for request-scoped caches.
    #[must_use]
    pub fn addr(&self) -> usize {
        self.ptr.as_ptr() as usize
    }

    /// Mutable access to the bytes of a uniquely-owned allocation.
    ///
    /// Callers must hold the only handle (`is_unique`); shared storage must
    /// be separated first. The uniqueness requirement keeps the mutation
    /// invisible to any other owner, mirroring `Rc::get_mut` semantics.
    #[must_use]
    #[allow(unsafe_code)]
    pub fn unique_bytes_mut(&mut self) -> &mut [u8] {
        assert!(self.is_unique(), "mutating shared compact bytes");
        let len = self.header().len;
        // SAFETY: the tail holds exactly `len` initialized bytes; `self` is
        // the only owner (asserted above) and holds an exclusive borrow, so
        // no aliasing reference can exist for the returned lifetime.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr().add(1).cast::<u8>(), len) }
    }
}

impl Clone for CompactBytes {
    fn clone(&self) -> Self {
        let header = self.header();
        let count = header.refcount.get();
        // Mirrors `Rc`: an overflowing count aborts before it can wrap
        // into a premature free.
        if count == usize::MAX {
            std::process::abort();
        }
        header.refcount.set(count + 1);
        Self {
            ptr: self.ptr,
            _not_send: PhantomData,
        }
    }
}

#[allow(unsafe_code)]
impl Drop for CompactBytes {
    fn drop(&mut self) {
        let header = self.header();
        let count = header.refcount.get();
        if count > 1 {
            header.refcount.set(count - 1);
            return;
        }
        let layout = Self::layout(header.len);
        // SAFETY: this was the last owner, `ptr` came from `alloc` with
        // the identical layout (recomputed from the stored `len`), and no
        // reference into the allocation outlives the handle that is being
        // dropped.
        unsafe {
            dealloc(self.ptr.as_ptr().cast::<u8>(), layout);
        }
    }
}

impl std::fmt::Debug for CompactBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompactBytes")
            .field("len", &self.len())
            .field("unique", &self.is_unique())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_native_arena_usage_reports_reserved_high_water_and_residency() {
        let mut arena = StableNativeArena::<u64>::new(4096);
        arena[0] = 7;
        arena[1023] = 9;
        let usage = arena.usage(1024);
        assert_eq!(usage.reserved_bytes, 4096 * std::mem::size_of::<u64>());
        assert_eq!(usage.high_water_bytes, 1024 * std::mem::size_of::<u64>());
        assert!(usage.resident_bytes <= usage.reserved_bytes);
        assert!(usage.resident_bytes >= std::mem::size_of::<u64>());
    }

    #[test]
    fn shared_storage_clones_separates_and_keeps_weak_lifetime_exact() {
        let mut first = Shared::new(vec![1_u32]);
        let weak = first.downgrade();
        let second = first.clone();
        assert_eq!(first.strong_count(), 2);
        Shared::make_mut(&mut first).push(2);
        assert_eq!(&*first, &[1, 2]);
        assert_eq!(&*second, &[1]);
        assert_eq!(weak.strong_count(), 1);
        drop(second);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn round_trips_bytes_of_every_size_class() {
        for source in [&b""[..], b"a", b"hello world", &[0xffu8; 4096][..]] {
            let bytes = CompactBytes::from_slice(source);
            assert_eq!(bytes.as_bytes(), source);
            assert_eq!(bytes.len(), source.len());
            assert_eq!(bytes.is_empty(), source.is_empty());
        }
    }

    #[test]
    fn clones_share_the_allocation_and_drop_frees_once() {
        let first = CompactBytes::from_slice(b"shared");
        assert!(first.is_unique());
        let second = first.clone();
        assert!(!first.is_unique());
        assert_eq!(first.as_bytes().as_ptr(), second.as_bytes().as_ptr());
        drop(second);
        assert!(first.is_unique());
        assert_eq!(first.as_bytes(), b"shared");
    }

    #[test]
    fn header_cells_cache_hash_and_symbol_across_clones() {
        let bytes = CompactBytes::from_slice(b"key");
        assert_eq!(bytes.hash(), 0);
        assert_eq!(bytes.symbol(), None);
        bytes.set_hash(42);
        bytes.set_symbol(Some(7));
        let clone = bytes.clone();
        assert_eq!(clone.hash(), 42);
        assert_eq!(clone.symbol(), Some(7));
        clone.set_symbol(None);
        assert_eq!(bytes.symbol(), None);
    }

    #[test]
    fn identity_and_unique_mutation_hold_their_contracts() {
        let mut a = CompactBytes::from_slice(b"abc");
        let b = a.clone();
        assert!(CompactBytes::ptr_eq(&a, &b));
        assert_eq!(a.addr(), b.addr());
        drop(b);
        a.unique_bytes_mut()[0] = b'x';
        assert_eq!(a.as_bytes(), b"xbc");
        let c = CompactBytes::from_slice(b"abc");
        assert!(!CompactBytes::ptr_eq(&a, &c));
    }

    #[test]
    fn from_parts_concatenates_without_inheriting_cached_cells() {
        let cases: &[&[&[u8]]] = &[
            &[],
            &[b""],
            &[b"", b"", b""],
            &[b"solo"],
            &[b"left", b"right"],
            &[b"a", b"", b"bc", b"defg", b""],
        ];
        for parts in cases {
            let expected: Vec<u8> = parts.iter().flat_map(|part| part.iter().copied()).collect();
            let joined = CompactBytes::from_parts(parts);
            assert_eq!(joined.as_bytes(), expected.as_slice());
            assert_eq!(joined.len(), expected.len());
            assert!(joined.is_unique());
            assert_eq!(joined.hash(), 0);
            assert_eq!(joined.symbol(), None);
        }
        let big_left = vec![0xAB_u8; 4096];
        let big_right = vec![0xCD_u8; 2048];
        let joined = CompactBytes::from_parts(&[&big_left, &big_right]);
        assert_eq!(&joined.as_bytes()[..4096], big_left.as_slice());
        assert_eq!(&joined.as_bytes()[4096..], big_right.as_slice());
    }

    #[test]
    fn from_parts_result_is_independent_of_its_sources() {
        let mut source = CompactBytes::from_slice(b"shared");
        source.set_hash(99);
        source.set_symbol(Some(7));
        let joined = CompactBytes::from_parts(&[source.as_bytes(), b"-tail"]);
        assert!(!CompactBytes::ptr_eq(&source, &joined));
        assert_eq!(joined.hash(), 0);
        assert_eq!(joined.symbol(), None);
        source.unique_bytes_mut()[0] = b'X';
        assert_eq!(joined.as_bytes(), b"shared-tail");
    }

    #[test]
    fn interleaved_clone_and_drop_keeps_contents_stable() {
        let a = CompactBytes::from_slice(b"interleave");
        let b = a.clone();
        let c = b.clone();
        drop(a);
        let d = c.clone();
        drop(b);
        drop(c);
        assert_eq!(d.as_bytes(), b"interleave");
        assert!(d.is_unique());
    }
}
