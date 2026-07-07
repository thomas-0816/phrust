//! VM-owned executable code memory — ADR 0787 prerequisite #1.
//!
//! This is the single abstraction that owns executable machine-code memory for
//! the (future, default-off) native tier. It upholds W^X: on this path a page
//! is never simultaneously writable and executable in a way the CPU can both
//! store to and fetch from. On Apple Silicon it maps `MAP_JIT` pages and uses
//! the per-thread `pthread_jit_write_protect_np` toggle (write, flip to execute,
//! invalidate the i-cache); on other Unix hosts it maps read/write, copies, then
//! `mprotect`s the range to read/execute. Hosts without a supported path fail
//! closed with [`CodeMemoryError::UnsupportedHost`].
//!
//! Constructing a [`CodeMemory`] is the ONLY place the engine is permitted to
//! allocate executable memory; there must be no ad hoc `mmap`/`mprotect` for
//! code elsewhere. Owning and testing this abstraction is a prerequisite for the
//! native tier (see `docs/adr/0787-fast-baseline-native-tier-prerequisites.md`).
//! It is deliberately NOT wired into VM execution: no interpreter path calls it,
//! so building it does not enable a `native_execution` mode.

/// Error allocating or finalizing executable code memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeMemoryError {
    /// The host platform has no supported executable-memory path.
    UnsupportedHost,
    /// An empty code buffer was requested.
    EmptyCode,
    /// The platform allocator failed with this errno.
    AllocationFailed(i32),
    /// The read-execute finalize transition failed with this errno.
    FinalizeFailed(i32),
}

/// A finalized, read-execute region of machine code owned by the VM. Dropping it
/// releases the mapping. After construction the region is executable and not
/// writable.
pub struct CodeMemory {
    ptr: *mut u8,
    mapped_len: usize,
    code_len: usize,
}

// The finalized region is immutable after construction and owns its own mapping,
// so handing the read-execute pointer to other threads is sound.
unsafe impl Send for CodeMemory {}
unsafe impl Sync for CodeMemory {}

impl CodeMemory {
    /// Allocates executable memory, copies `code` into it, and finalizes it as
    /// read-execute. The returned region's entry point is [`CodeMemory::as_ptr`].
    ///
    /// # Errors
    /// Returns [`CodeMemoryError`] if `code` is empty, the host is unsupported,
    /// or the platform allocation / read-execute transition fails.
    pub fn new(code: &[u8]) -> Result<Self, CodeMemoryError> {
        if code.is_empty() {
            return Err(CodeMemoryError::EmptyCode);
        }
        imp::allocate_and_finalize(code)
    }

    /// Entry pointer into the finalized (read-execute) code.
    #[must_use]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.cast_const()
    }

    /// Number of machine-code bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.code_len
    }

    /// Whether the region holds any code.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.code_len == 0
    }
}

impl Drop for CodeMemory {
    fn drop(&mut self) {
        // SAFETY: `ptr`/`mapped_len` came from a successful `imp` mapping and are
        // released exactly once here.
        unsafe { imp::deallocate(self.ptr, self.mapped_len) };
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{CodeMemory, CodeMemoryError};

    unsafe extern "C" {
        // Darwin instruction-cache flush (libkern); not exposed by the libc crate.
        fn sys_icache_invalidate(start: *mut core::ffi::c_void, len: usize);
    }

    pub(super) fn allocate_and_finalize(code: &[u8]) -> Result<CodeMemory, CodeMemoryError> {
        let mapped_len = round_up_to_page(code.len());
        // SAFETY: anonymous private JIT mapping; the arguments are valid and the
        // returned pointer is checked against MAP_FAILED before use.
        let raw = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                mapped_len,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            )
        };
        if raw == libc::MAP_FAILED {
            return Err(CodeMemoryError::AllocationFailed(errno()));
        }
        let ptr = raw.cast::<u8>();
        // SAFETY: W^X on Apple Silicon — disable write protection for this
        // thread's MAP_JIT pages, copy the code, re-enable write protection
        // (making the pages executable and not writable), then flush the
        // instruction cache for the written range. The destination is a fresh
        // `mapped_len >= code.len()` mapping, so the copy is in bounds.
        unsafe {
            libc::pthread_jit_write_protect_np(0);
            core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len());
            libc::pthread_jit_write_protect_np(1);
            sys_icache_invalidate(ptr.cast(), code.len());
        }
        Ok(CodeMemory {
            ptr,
            mapped_len,
            code_len: code.len(),
        })
    }

    pub(super) unsafe fn deallocate(ptr: *mut u8, mapped_len: usize) {
        // SAFETY: unmapping a live mapping owned by this type exactly once.
        unsafe { libc::munmap(ptr.cast(), mapped_len) };
    }

    fn round_up_to_page(n: usize) -> usize {
        let page = page_size();
        (n + page - 1) & !(page - 1)
    }

    fn page_size() -> usize {
        // SAFETY: `sysconf(_SC_PAGESIZE)` takes no pointers and is always safe.
        let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if value > 0 { value as usize } else { 4096 }
    }

    fn errno() -> i32 {
        // SAFETY: `__error()` returns a valid pointer to this thread's errno.
        unsafe { *libc::__error() }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{CodeMemory, CodeMemoryError};

    pub(super) fn allocate_and_finalize(code: &[u8]) -> Result<CodeMemory, CodeMemoryError> {
        let mapped_len = round_up_to_page(code.len());
        // SAFETY: anonymous private read/write mapping; the returned pointer is
        // checked against MAP_FAILED before use.
        let raw = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                mapped_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if raw == libc::MAP_FAILED {
            return Err(CodeMemoryError::AllocationFailed(errno()));
        }
        let ptr = raw.cast::<u8>();
        // SAFETY: copy into the fresh writable mapping (`mapped_len >=
        // code.len()`), then transition the range to read-execute so it is never
        // simultaneously writable and executable.
        unsafe { core::ptr::copy_nonoverlapping(code.as_ptr(), ptr, code.len()) };
        let rc = unsafe { libc::mprotect(raw, mapped_len, libc::PROT_READ | libc::PROT_EXEC) };
        if rc != 0 {
            let saved = errno();
            // SAFETY: releasing the mapping we just created.
            unsafe { libc::munmap(raw, mapped_len) };
            return Err(CodeMemoryError::FinalizeFailed(saved));
        }
        #[cfg(target_arch = "aarch64")]
        // SAFETY: flush the instruction cache for the newly written range so the
        // CPU fetches the stored bytes rather than stale i-cache contents.
        unsafe {
            clear_cache(ptr.cast(), ptr.add(code.len()).cast());
        }
        Ok(CodeMemory {
            ptr,
            mapped_len,
            code_len: code.len(),
        })
    }

    pub(super) unsafe fn deallocate(ptr: *mut u8, mapped_len: usize) {
        // SAFETY: unmapping a live mapping owned by this type exactly once.
        unsafe { libc::munmap(ptr.cast(), mapped_len) };
    }

    #[cfg(target_arch = "aarch64")]
    unsafe extern "C" {
        fn clear_cache(start: *mut core::ffi::c_void, end: *mut core::ffi::c_void);
    }

    fn round_up_to_page(n: usize) -> usize {
        let page = page_size();
        (n + page - 1) & !(page - 1)
    }

    fn page_size() -> usize {
        // SAFETY: `sysconf(_SC_PAGESIZE)` takes no pointers and is always safe.
        let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if value > 0 { value as usize } else { 4096 }
    }

    fn errno() -> i32 {
        // SAFETY: `__errno_location()` returns a valid pointer to this thread's errno.
        unsafe { *libc::__errno_location() }
    }
}

#[cfg(not(unix))]
mod imp {
    use super::{CodeMemory, CodeMemoryError};

    pub(super) fn allocate_and_finalize(_code: &[u8]) -> Result<CodeMemory, CodeMemoryError> {
        Err(CodeMemoryError::UnsupportedHost)
    }

    pub(super) unsafe fn deallocate(_ptr: *mut u8, _mapped_len: usize) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_code() {
        assert!(matches!(
            CodeMemory::new(&[]),
            Err(CodeMemoryError::EmptyCode)
        ));
    }

    // A finalized region must actually execute native code and return its value.
    // The stub is a leaf function returning the 32-bit constant 42.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_return_constant_aarch64() {
        // movz w0, #42   (0x52800540) ; ret (0xd65f03c0), little-endian bytes.
        let code: [u8; 8] = [0x40, 0x05, 0x80, 0x52, 0xc0, 0x03, 0x5f, 0xd6];
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        assert_eq!(mem.len(), 8);
        // SAFETY: the bytes are a valid `extern "C" fn() -> i32` for aarch64, the
        // region is read-execute, and `mem` outlives the call.
        let func: extern "C" fn() -> i32 = unsafe { core::mem::transmute(mem.as_ptr()) };
        assert_eq!(func(), 42);
    }

    #[cfg(all(unix, target_arch = "x86_64"))]
    #[test]
    fn executes_native_return_constant_x86_64() {
        // mov eax, 42 (b8 2a 00 00 00) ; ret (c3).
        let code: [u8; 6] = [0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3];
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        assert_eq!(mem.len(), 6);
        // SAFETY: the bytes are a valid `extern "C" fn() -> i32` for x86-64, the
        // region is read-execute, and `mem` outlives the call.
        let func: extern "C" fn() -> i32 = unsafe { core::mem::transmute(mem.as_ptr()) };
        assert_eq!(func(), 42);
    }

    // End-to-end: the aarch64 encoder emits real integer arithmetic, the code
    // memory finalizes it, and it runs natively over live arguments. This is the
    // copy-and-patch codegen path in miniature (emit -> finalize -> execute).
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_emitted_native_arithmetic() {
        use crate::aarch64::{Aarch64Assembler, X0, X1};

        // extern "C" fn(a, b) -> a + b : add x0, x0, x1 ; ret
        let mut asm = Aarch64Assembler::new();
        asm.add(X0, X0, X1);
        asm.ret();
        let mem = CodeMemory::new(&asm.finish()).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(i64, i64) -> i64`,
        // the region is read-execute, and `mem` outlives the calls.
        let add: extern "C" fn(i64, i64) -> i64 = unsafe { core::mem::transmute(mem.as_ptr()) };
        assert_eq!(add(3, 4), 7);
        assert_eq!(add(100, -1), 99);

        // extern "C" fn(a, b) -> a * b : mul x0, x0, x1 ; ret
        let mut asm = Aarch64Assembler::new();
        asm.mul(X0, X0, X1);
        asm.ret();
        let mem = CodeMemory::new(&asm.finish()).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(i64, i64) -> i64` over a read-execute region.
        let mul: extern "C" fn(i64, i64) -> i64 = unsafe { core::mem::transmute(mem.as_ptr()) };
        assert_eq!(mul(6, 7), 42);
    }
}
