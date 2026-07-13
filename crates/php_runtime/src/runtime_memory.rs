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
use std::ptr::NonNull;
use std::rc::Rc;

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
