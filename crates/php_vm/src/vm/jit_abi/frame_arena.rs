use super::*;

#[cfg(unix)]
use std::ptr::NonNull;

const FRAME_ARENA_CHUNK_BYTES: usize = 64 * 1024;
const FRAME_ARENA_MAX_BYTES: usize = 16 * 1024 * 1024;
const FRAME_ARENA_MAX_ALLOCATION_BYTES: usize = 1024 * 1024;
const FRAME_ARENA_MAX_ACTIVE_ALLOCATIONS: usize = 768;

struct FrameChunk {
    storage: FrameChunkStorage,
    used: usize,
}

impl std::fmt::Debug for FrameChunk {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrameChunk")
            .field("capacity", &self.storage.len())
            .field("used", &self.used)
            .finish()
    }
}

#[cfg(unix)]
struct FrameChunkStorage {
    base: NonNull<u8>,
    usable_len: usize,
    mapping_len: usize,
}

// SAFETY: `FrameChunkStorage` uniquely owns its anonymous mapping. Moving the
// owner between worker requests does not expose or alias the mapping; checkout
// occurs only after the previous request has ended and reset its allocations.
#[cfg(unix)]
#[allow(unsafe_code)]
unsafe impl Send for FrameChunkStorage {}

#[cfg(unix)]
impl std::fmt::Debug for FrameChunkStorage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GuardedFrameChunk")
            .field("usable_len", &self.usable_len)
            .field("mapping_len", &self.mapping_len)
            .finish_non_exhaustive()
    }
}

#[cfg(unix)]
impl FrameChunkStorage {
    // SAFETY: this item owns and bounds every raw mapping operation it performs.
    #[allow(unsafe_code)]
    fn new(usable_len: usize) -> Result<Self, String> {
        // SAFETY: `sysconf` has no memory-safety preconditions.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let page = usize::try_from(page)
            .map_err(|_| "E_PHP_VM_NATIVE_FRAME_LIMIT: host page size is unavailable".to_owned())?;
        if page == 0 || !page.is_power_of_two() {
            return Err("E_PHP_VM_NATIVE_FRAME_LIMIT: host page size is invalid".to_owned());
        }
        let usable_len = usable_len
            .checked_add(page - 1)
            .map(|value| value & !(page - 1))
            .ok_or_else(|| "E_PHP_VM_NATIVE_FRAME_LIMIT: guarded chunk size overflow".to_owned())?;
        let mapping_len = usable_len.checked_add(page).ok_or_else(|| {
            "E_PHP_VM_NATIVE_FRAME_LIMIT: guarded mapping size overflow".to_owned()
        })?;
        // SAFETY: The anonymous private mapping has no borrowed backing and
        // `mapping_len` was checked above. It is owned until `Drop` unmaps it.
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                mapping_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if address == libc::MAP_FAILED {
            return Err(format!(
                "E_PHP_VM_NATIVE_FRAME_LIMIT: guarded frame mapping failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let base = NonNull::new(address.cast::<u8>()).ok_or_else(|| {
            "E_PHP_VM_NATIVE_FRAME_LIMIT: guarded frame mapping returned null".to_owned()
        })?;
        // SAFETY: `base + usable_len` is the final mapped page and both the
        // address and length are page-aligned.
        let protected =
            unsafe { libc::mprotect(base.as_ptr().add(usable_len).cast(), page, libc::PROT_NONE) };
        if protected != 0 {
            let error = std::io::Error::last_os_error();
            // SAFETY: The mapping is still exclusively owned here.
            unsafe {
                libc::munmap(base.as_ptr().cast(), mapping_len);
            }
            return Err(format!(
                "E_PHP_VM_NATIVE_FRAME_LIMIT: frame guard page failed: {error}"
            ));
        }
        Ok(Self {
            base,
            usable_len,
            mapping_len,
        })
    }

    fn len(&self) -> usize {
        self.usable_len
    }

    // SAFETY: offset bounds are checked by the arena before pointer arithmetic.
    #[allow(unsafe_code)]
    fn pointer_at(&mut self, offset: usize) -> *mut u8 {
        debug_assert!(offset < self.usable_len);
        // SAFETY: Callers validate `offset` against `usable_len`.
        unsafe { self.base.as_ptr().add(offset) }
    }

    #[cfg(test)]
    fn guard_address(&self) -> *mut u8 {
        // This address belongs to the mapping but is intentionally PROT_NONE.
        self.base.as_ptr().wrapping_add(self.usable_len)
    }
}

#[cfg(unix)]
impl Drop for FrameChunkStorage {
    // SAFETY: this item releases the mapping uniquely owned by the storage value.
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: This instance uniquely owns the complete mapping.
        unsafe {
            libc::munmap(self.base.as_ptr().cast(), self.mapping_len);
        }
    }
}

#[cfg(not(unix))]
#[derive(Debug)]
struct FrameChunkStorage(Box<[u8]>);

#[cfg(not(unix))]
impl FrameChunkStorage {
    fn new(usable_len: usize) -> Result<Self, String> {
        Ok(Self(vec![0; usable_len].into_boxed_slice()))
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn pointer_at(&mut self, offset: usize) -> *mut u8 {
        debug_assert!(offset < self.0.len());
        self.0[offset..].as_mut_ptr()
    }
}

#[derive(Clone, Copy, Debug)]
struct FrameAllocation {
    chunk: usize,
    previous_used: usize,
    charged_bytes: usize,
    address: usize,
}

/// Bounded request-local storage for generated PHP call frames and slot tables.
#[derive(Debug)]
pub(super) struct NativeFrameArena {
    chunks: Vec<FrameChunk>,
    allocations: Vec<FrameAllocation>,
    capacity_bytes: usize,
    active_bytes: usize,
    high_water_bytes: usize,
}

impl Default for NativeFrameArena {
    fn default() -> Self {
        Self {
            chunks: Vec::with_capacity(4),
            allocations: Vec::with_capacity(256),
            capacity_bytes: 0,
            active_bytes: 0,
            high_water_bytes: 0,
        }
    }
}

impl NativeFrameArena {
    fn allocate(&mut self, bytes: usize, alignment: usize) -> Result<usize, String> {
        if self.allocations.len() >= FRAME_ARENA_MAX_ACTIVE_ALLOCATIONS {
            return Err(format!(
                "E_PHP_VM_NATIVE_FRAME_DEPTH: active PHP frame storage exceeds {} allocations",
                FRAME_ARENA_MAX_ACTIVE_ALLOCATIONS
            ));
        }
        if bytes == 0 || bytes > FRAME_ARENA_MAX_ALLOCATION_BYTES {
            return Err(format!(
                "E_PHP_VM_NATIVE_FRAME_LIMIT: frame allocation {bytes} exceeds {} bytes",
                FRAME_ARENA_MAX_ALLOCATION_BYTES
            ));
        }
        if alignment == 0 || !alignment.is_power_of_two() || alignment > 64 {
            return Err(format!(
                "E_PHP_VM_NATIVE_FRAME_ALIGNMENT: unsupported frame alignment {alignment}"
            ));
        }
        let fits = |chunk: &FrameChunk| {
            let aligned = chunk
                .used
                .checked_add(alignment - 1)
                .map(|value| value & !(alignment - 1))?;
            aligned
                .checked_add(bytes)
                .filter(|end| *end <= chunk.storage.len())?;
            Some((aligned, chunk.used))
        };
        let (chunk, offset, previous_used) = self
            .chunks
            .last()
            .and_then(fits)
            .map(|(offset, previous)| (self.chunks.len() - 1, offset, previous))
            .map_or_else(
                || {
                    let required = bytes.checked_add(alignment - 1).ok_or_else(|| {
                        "E_PHP_VM_NATIVE_FRAME_LIMIT: frame size overflow".to_owned()
                    })?;
                    let capacity = FRAME_ARENA_CHUNK_BYTES.max(required.next_power_of_two());
                    let next_capacity =
                        self.capacity_bytes.checked_add(capacity).ok_or_else(|| {
                            "E_PHP_VM_NATIVE_FRAME_LIMIT: arena size overflow".to_owned()
                        })?;
                    if next_capacity > FRAME_ARENA_MAX_BYTES {
                        return Err(format!(
                            "E_PHP_VM_NATIVE_FRAME_LIMIT: request frame arena exceeds {} bytes",
                            FRAME_ARENA_MAX_BYTES
                        ));
                    }
                    self.chunks.push(FrameChunk {
                        storage: FrameChunkStorage::new(capacity)?,
                        used: 0,
                    });
                    self.capacity_bytes = next_capacity;
                    Ok((self.chunks.len() - 1, 0, 0))
                },
                Ok,
            )?;
        let address = {
            let chunk_ref = &mut self.chunks[chunk];
            chunk_ref.used = offset + bytes;
            chunk_ref.storage.pointer_at(offset) as usize
        };
        let charged_bytes = offset + bytes - previous_used;
        self.active_bytes = self.active_bytes.saturating_add(charged_bytes);
        self.high_water_bytes = self.high_water_bytes.max(self.active_bytes);
        self.allocations.push(FrameAllocation {
            chunk,
            previous_used,
            charged_bytes,
            address,
        });
        Ok(address)
    }

    fn release(&mut self, address: usize) -> Result<(), String> {
        let Some(allocation) = self.allocations.pop() else {
            return Err("E_PHP_VM_NATIVE_FRAME_ORDER: release from empty arena".to_owned());
        };
        if allocation.address != address {
            self.allocations.push(allocation);
            return Err("E_PHP_VM_NATIVE_FRAME_ORDER: non-LIFO frame release".to_owned());
        }
        self.chunks[allocation.chunk].used = allocation.previous_used;
        self.active_bytes = self.active_bytes.saturating_sub(allocation.charged_bytes);
        Ok(())
    }

    pub(super) fn high_water_bytes(&self) -> usize {
        self.high_water_bytes
    }

    pub(super) fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }

    /// Ends one request's use of the worker-owned arena without discarding its
    /// guarded mappings. No generated frame pointer survives request teardown.
    pub(super) fn reset_for_pool(&mut self) {
        debug_assert!(
            self.allocations.is_empty(),
            "native frame arena returned with live allocations"
        );
        self.allocations.clear();
        for chunk in &mut self.chunks {
            chunk.used = 0;
        }
        self.active_bytes = 0;
        self.high_water_bytes = 0;
    }
}

// SAFETY: this ABI boundary returns an opaque integer address without dereferencing it.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_frame_alloc_abi(
    runtime: *mut NativeRequestFastState,
    _vm_context: u64,
    bytes: u64,
    alignment: u64,
) -> u64 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_native_context_for(runtime, "frame_arena", |context| {
            let bytes = usize::try_from(bytes).map_err(|_| {
                "E_PHP_VM_NATIVE_FRAME_LIMIT: frame size does not fit usize".to_owned()
            })?;
            let alignment = usize::try_from(alignment).map_err(|_| {
                "E_PHP_VM_NATIVE_FRAME_ALIGNMENT: alignment does not fit usize".to_owned()
            })?;
            match context.native_frame_arena.allocate(bytes, alignment) {
                Ok(address) => Ok(address),
                Err(message) => {
                    context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                        "E_PHP_VM_NATIVE_FRAME_LIMIT",
                        php_runtime::api::RuntimeSeverity::FatalError,
                        message.clone(),
                        php_runtime::api::RuntimeSourceSpan::default(),
                        Vec::new(),
                        None,
                    ));
                    Err(message)
                }
            }
        })
        .and_then(Result::ok)
        .unwrap_or(0) as u64
    }))
    .unwrap_or(0)
}

pub(in crate::vm) extern "C" fn jit_native_frame_release_abi(
    runtime: *mut NativeRequestFastState,
    _vm_context: u64,
    address: u64,
) -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if with_native_context_for(runtime, "frame_arena", |context| {
            context.native_frame_arena.release(address as usize)
        })
        .is_some_and(|result| result.is_ok())
        {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    }))
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_addresses_are_stable_bounded_and_lifo() {
        let mut arena = NativeFrameArena::default();
        let outer = arena.allocate(48, 16).unwrap();
        let inner = arena.allocate(96, 16).unwrap();
        assert_ne!(outer, 0);
        assert_ne!(inner, 0);
        assert!(arena.release(outer).is_err());
        arena.release(inner).unwrap();
        arena.release(outer).unwrap();
        assert!(arena.high_water_bytes() >= 144);
        assert!(
            arena
                .allocate(FRAME_ARENA_MAX_ALLOCATION_BYTES + 1, 16)
                .is_err()
        );
    }

    #[test]
    #[cfg(all(unix, not(miri)))]
    // SAFETY: this test confines its deliberate guard-page access to a child process.
    #[allow(unsafe_code)]
    fn guarded_chunk_faults_on_first_byte_past_the_usable_range() {
        let chunk = FrameChunkStorage::new(FRAME_ARENA_CHUNK_BYTES).unwrap();
        // SAFETY: The child performs one intentional guard-page write and
        // exits immediately. The parent never dereferences the protected page.
        let child = unsafe { libc::fork() };
        assert!(
            child >= 0,
            "fork failed: {}",
            std::io::Error::last_os_error()
        );
        if child == 0 {
            // SAFETY: This is deliberately outside the usable range but still
            // within the owned mapping; mprotect must terminate the child.
            unsafe {
                chunk.guard_address().write_volatile(1);
                libc::_exit(0);
            }
        }
        let mut status = 0;
        // SAFETY: `child` is the live PID returned by `fork` above.
        assert_eq!(unsafe { libc::waitpid(child, &mut status, 0) }, child);
        assert!(libc::WIFSIGNALED(status));
        assert!(matches!(
            libc::WTERMSIG(status),
            libc::SIGSEGV | libc::SIGBUS
        ));
    }
}
