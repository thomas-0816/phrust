//! VM-owned executable code memory — ADR 0019 prerequisite #1.
//!
//! This is the repository-owned abstraction for emitted machine-code memory in
//! the (future, default-off) native tier. It upholds W^X: on this path a page is
//! never simultaneously writable and executable in a way the CPU can both store
//! to and fetch from. On Apple Silicon it maps `MAP_JIT` pages and uses the
//! per-thread `pthread_jit_write_protect_np` toggle (write, flip to execute,
//! invalidate the i-cache); on other Unix hosts it maps read/write, copies, then
//! `mprotect`s the range to read/execute. Hosts without a supported path fail
//! closed with [`CodeMemoryError::UnsupportedHost`].
//!
//! Constructing a [`CodeMemory`] is the repository-owned executable-memory path
//! for emitted machine-code experiments; Cranelift-generated entries remain
//! governed separately by ADR 0018 and Cranelift's JIT memory provider. There
//! must be no ad hoc `mmap`/`mprotect` for code elsewhere. Owning and testing
//! this abstraction is a prerequisite for the native tier (see
//! `docs/adr/0019-fast-baseline-native-tier-prerequisites.md`). It is
//! deliberately NOT wired into VM execution: no interpreter path calls it, so
//! building it does not enable a `native_execution` mode.

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
            __clear_cache(ptr.cast(), ptr.add(code.len()).cast());
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
        fn __clear_cache(start: *mut core::ffi::c_void, end: *mut core::ffi::c_void);
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

    // The copy-and-patch `guarded_int_arithmetic` stencil realized as native
    // code: a fast path with an overflow guard that takes a side exit (deopt)
    // instead of writing a wrong result, matching PHP's checked-add semantics.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_emitted_guarded_add_with_overflow_side_exit() {
        use crate::aarch64::{Aarch64Assembler, Cond, X0, X1, X2, X3};

        // extern "C" fn(a, b, out) -> i32:
        //   adds x3, x0, x1        ; sum + flags
        //   b.vs deopt             ; overflow -> side exit
        //   str  x3, [x2]          ; *out = sum
        //   movz x0, #0 ; ret      ; return 0 (ok)
        // deopt:
        //   movz x0, #1 ; ret      ; return 1 (side exit / deopt)
        let mut asm = Aarch64Assembler::new();
        let deopt = asm.new_label();
        asm.adds(X3, X0, X1);
        asm.b_cond(Cond::Overflow, deopt);
        asm.str_reg(X3, X2);
        asm.movz(X0, 0);
        asm.ret();
        asm.bind(deopt);
        asm.movz(X0, 1);
        asm.ret();
        let mem = CodeMemory::new(&asm.finish()).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(i64, i64, *mut i64)
        // -> i32` over a read-execute region that outlives the calls.
        let guarded_add: extern "C" fn(i64, i64, *mut i64) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        let mut out: i64 = 0;
        assert_eq!(guarded_add(3, 4, &mut out), 0, "fast path returns ok");
        assert_eq!(out, 7);

        out = -1;
        assert_eq!(
            guarded_add(i64::MAX, 1, &mut out),
            1,
            "overflow takes the side exit"
        );
        assert_eq!(out, -1, "the side exit must not write a result");
    }

    // The `binary_add` copy-and-patch stencil operating on the real VM value
    // ABI (`JitCValue`): guard both operand tags are Int, add with an overflow
    // guard, write an Int result — otherwise take the interpreter side exit.
    // This is native codegen over the actual value representation, the shape
    // that running `$a + $b` natively requires.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_binary_add_stencil_over_jit_c_value() {
        use crate::JitCValue;
        use crate::aarch64::{Aarch64Assembler, Cond, X0, X1, X2, X3, X4, X5, X6};
        use crate::abi::JitCValueTag;

        const INT_TAG: u16 = JitCValueTag::Int as u16;

        // extern "C" fn(a: *const JitCValue, b: *const JitCValue,
        //               out: *mut JitCValue) -> i32
        //   tag @ +0 (u32), payload @ +8 (u64); Int tag == 3.
        let mut asm = Aarch64Assembler::new();
        let deopt = asm.new_label();
        asm.ldr_w(X3, X0, 0); // a.tag
        asm.cmp_imm_w(X3, INT_TAG);
        asm.b_cond(Cond::NotEqual, deopt);
        asm.ldr_w(X3, X1, 0); // b.tag
        asm.cmp_imm_w(X3, INT_TAG);
        asm.b_cond(Cond::NotEqual, deopt);
        asm.ldr_x(X4, X0, 8); // a.payload
        asm.ldr_x(X5, X1, 8); // b.payload
        asm.adds(X6, X4, X5); // sum + flags
        asm.b_cond(Cond::Overflow, deopt);
        asm.movz(X3, INT_TAG);
        asm.str_w(X3, X2, 0); // out.tag = Int
        asm.str_x(X6, X2, 8); // out.payload = sum
        asm.movz(X0, 0);
        asm.ret();
        asm.bind(deopt);
        asm.movz(X0, 1);
        asm.ret();
        let mem = CodeMemory::new(&asm.finish()).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(*const JitCValue,
        // *const JitCValue, *mut JitCValue) -> i32` over a read-execute region;
        // the pointers below are all valid, aligned `JitCValue`s that outlive the call.
        let add: extern "C" fn(*const JitCValue, *const JitCValue, *mut JitCValue) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        // Fast path: 3 + 4 = 7.
        let a = JitCValue::int(3);
        let b = JitCValue::int(4);
        let mut out = JitCValue::uninitialized();
        assert_eq!(add(&a, &b, &mut out), 0);
        assert_eq!(out.tag, JitCValueTag::Int);
        assert_eq!(out.payload as i64, 7);

        // Overflow -> side exit; result left uninitialized.
        let big = JitCValue::int(i64::MAX);
        let one = JitCValue::int(1);
        let mut over = JitCValue::uninitialized();
        assert_eq!(add(&big, &one, &mut over), 1);
        assert_eq!(over.tag, JitCValueTag::Uninitialized);

        // Non-int operand -> type-guard side exit.
        let not_int = JitCValue::null();
        let mut typed = JitCValue::uninitialized();
        assert_eq!(add(&a, &not_int, &mut typed), 1);
        assert_eq!(typed.tag, JitCValueTag::Uninitialized);
    }

    // The Frame-Local Slot ABI made executable: a dense function's working set
    // is a single flat `[JitCValue]` slot buffer that the VM marshals in/out
    // around the call. Native code addresses slot `i` at `slot_base + i*24`
    // directly through the emitter's scaled-offset loads/stores — no per-access
    // helper. This computes `slot[2] = slot[0] + slot[1]` (the "add two locals
    // into a third local" kernel) with Int type guards and an overflow side
    // exit, reading and writing three slots of one contiguous buffer.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_add_over_a_flat_slot_buffer() {
        use crate::JitCValue;
        use crate::aarch64::{Aarch64Assembler, Cond, X0, X3, X4, X5, X6};
        use crate::abi::JitCValueTag;

        const INT_TAG: u16 = JitCValueTag::Int as u16;
        // `JitCValue` is repr(C) and 24 bytes (tag@0 u32, payload@8 u64,
        // aux@16 u64), so slot `i` lives at `i * 24` and every field lands on a
        // legal scaled-immediate boundary.
        const STRIDE: u32 = 24;
        const TAG: u32 = 0;
        const PAYLOAD: u32 = 8;

        // extern "C" fn(slot_base: *mut JitCValue) -> i32, computing
        //   slot[2] = slot[0] + slot[1]  (Int-guarded, overflow -> side exit)
        // x0 = slot_base; x3 = scratch/tag; x4/x5 = payloads; x6 = sum.
        let mut asm = Aarch64Assembler::new();
        let deopt = asm.new_label();
        asm.ldr_w(X3, X0, TAG); // slot[0].tag
        asm.cmp_imm_w(X3, INT_TAG);
        asm.b_cond(Cond::NotEqual, deopt);
        asm.ldr_w(X3, X0, STRIDE + TAG); // slot[1].tag
        asm.cmp_imm_w(X3, INT_TAG);
        asm.b_cond(Cond::NotEqual, deopt);
        asm.ldr_x(X4, X0, PAYLOAD); // slot[0].payload
        asm.ldr_x(X5, X0, STRIDE + PAYLOAD); // slot[1].payload
        asm.adds(X6, X4, X5); // sum + flags
        asm.b_cond(Cond::Overflow, deopt);
        asm.movz(X3, INT_TAG);
        asm.str_w(X3, X0, 2 * STRIDE + TAG); // slot[2].tag = Int
        asm.str_x(X6, X0, 2 * STRIDE + PAYLOAD); // slot[2].payload = sum
        asm.movz(X0, 0);
        asm.ret();
        asm.bind(deopt);
        asm.movz(X0, 1);
        asm.ret();
        let mem = CodeMemory::new(&asm.finish()).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(*mut JitCValue) -> i32`
        // over a read-execute region; the buffer below is a live, aligned,
        // contiguous `[JitCValue; 3]` (24-byte stride) that outlives the call.
        let run: extern "C" fn(*mut JitCValue) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        // Fast path: slot[0]=20, slot[1]=22 -> slot[2]=42.
        let mut slots = [
            JitCValue::int(20),
            JitCValue::int(22),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[2].tag, JitCValueTag::Int);
        assert_eq!(slots[2].payload as i64, 42);

        // Type-guard side exit: slot[1] is not Int -> result slot untouched.
        let mut typed = [
            JitCValue::int(1),
            JitCValue::null(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(typed.as_mut_ptr()), 1);
        assert_eq!(typed[2].tag, JitCValueTag::Uninitialized);

        // Overflow side exit: result slot untouched.
        let mut over = [
            JitCValue::int(i64::MAX),
            JitCValue::int(1),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(over.as_mut_ptr()), 1);
        assert_eq!(over[2].tag, JitCValueTag::Uninitialized);
    }

    // The native<->VM-helper call boundary: copy-and-patch-emitted code
    // materializes a runtime-resolved helper address and `blr`s into a real VM
    // helper (with a saved link register), returning the helper's status. This
    // is the mechanism every VM-state operation in the native tier uses (frame
    // access, arrays, objects, strings), since those touch VM-owned state only
    // through helpers rather than direct memory.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn emitted_code_calls_a_vm_helper() {
        use crate::JIT_HELPER_STATUS_OK;
        use crate::aarch64::{Aarch64Assembler, X9};
        use crate::helpers::phrust_jit_i64_add_checked;

        // extern "C" fn(a, b, out) -> i32 forwarding to the checked-add helper:
        //   push fp/lr ; mov_imm64 x9, &helper ; blr x9 ; pop fp/lr ; ret
        // a/b/out already sit in x0/x1/x2, matching the helper (lhs, rhs, out) ABI.
        let helper_addr = phrust_jit_i64_add_checked as *const () as usize as u64;
        let mut asm = Aarch64Assembler::new();
        asm.push_fp_lr();
        asm.mov_imm64(X9, helper_addr);
        asm.blr(X9);
        asm.pop_fp_lr();
        asm.ret();
        let mem = CodeMemory::new(&asm.finish()).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(i64, i64, *mut i64)
        // -> i32` matching the helper ABI, over a read-execute region.
        let call: extern "C" fn(i64, i64, *mut i64) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        let mut out: i64 = 0;
        assert_eq!(call(3, 4, &mut out), JIT_HELPER_STATUS_OK);
        assert_eq!(out, 7, "the VM helper ran via emitted native code");

        // Overflow: the helper reports a non-OK status through the same boundary.
        let mut over: i64 = 0;
        assert_ne!(call(i64::MAX, 1, &mut over), JIT_HELPER_STATUS_OK);
    }

    // The copy-and-patch sequencer (`copy_patch::emit_guarded_int_add_sequence`)
    // lowering a multi-step region over the flat slot buffer, executed
    // end-to-end. Steps chain through the buffer: step 2 reads slot 2, which
    // step 1 wrote. Verifies the success path, a mid-sequence side exit with its
    // resume-consistent partial store, and an overflow side exit.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_guarded_int_add_sequence() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{GuardedIntAddStep, emit_guarded_int_add_sequence};

        // slot[2] = slot[0] + slot[1]; slot[4] = slot[2] + slot[3].
        let steps = [
            GuardedIntAddStep {
                dst: 2,
                lhs: 0,
                rhs: 1,
            },
            GuardedIntAddStep {
                dst: 4,
                lhs: 2,
                rhs: 3,
            },
        ];
        let code = emit_guarded_int_add_sequence(&steps).expect("sequence emits");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(*mut JitCValue) -> i32`
        // over a read-execute region; each buffer below is a live, aligned,
        // contiguous `[JitCValue; 5]` (24-byte stride) that outlives the call.
        let run: extern "C" fn(*mut JitCValue) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        // Success: (10 + 20) + 12 = 42, chained through slot 2.
        let mut slots = [
            JitCValue::int(10),
            JitCValue::int(20),
            JitCValue::uninitialized(),
            JitCValue::int(12),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[2].payload as i64, 30);
        assert_eq!(slots[4].tag, JitCValueTag::Int);
        assert_eq!(slots[4].payload as i64, 42);

        // Mid-sequence side exit: slot 3 is non-Int, so step 2's guard fails.
        // Step 1 already ran, so slot 2 keeps its result (resume-consistent).
        let mut typed = [
            JitCValue::int(10),
            JitCValue::int(20),
            JitCValue::uninitialized(),
            JitCValue::null(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(typed.as_mut_ptr()), 1);
        assert_eq!(
            typed[2].payload as i64, 30,
            "a completed step's store survives the later side exit"
        );
        assert_eq!(typed[4].tag, JitCValueTag::Uninitialized);

        // Overflow in step 1 takes the side exit before its store, so nothing
        // downstream is written.
        let mut over = [
            JitCValue::int(i64::MAX),
            JitCValue::int(1),
            JitCValue::uninitialized(),
            JitCValue::int(7),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(over.as_mut_ptr()), 1);
        assert_eq!(over[2].tag, JitCValueTag::Uninitialized);
        assert_eq!(over[4].tag, JitCValueTag::Uninitialized);
    }

    // The full region-IR -> native path: build a real RegionGraph computing
    // (p0 + p1) + p2 over three marshaled locals, lower it with
    // copy_patch::compile_scalar_int_region, and execute the result over the
    // slot buffer it specifies. Proves the compiler's node->slot layout matches
    // what the emitted code reads/writes, end-to-end.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_compiled_param_add_region() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::compile_scalar_int_region;
        use crate::region_ir::{
            RegionEffects, RegionGraph, RegionId, RegionNode, RegionNodeKind, RegionPlacement,
            RegionValueType, VmSlotId,
        };

        fn param(graph: &mut RegionGraph, slot: u32) -> crate::region_ir::NodeId {
            graph.add_node(RegionNode::new(
                RegionNodeKind::Param {
                    slot: VmSlotId::new(slot),
                },
                Vec::new(),
                None,
                RegionValueType::I64,
                RegionPlacement::Floating,
                RegionEffects::PURE,
            ))
        }

        let mut graph = RegionGraph::new(RegionId::new(7), "region-add-sum");
        let p0 = param(&mut graph, 0);
        let p1 = param(&mut graph, 1);
        let p2 = param(&mut graph, 2);
        let sum01 = graph.add_node(RegionNode::new(
            RegionNodeKind::Add,
            vec![p0, p1],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ));
        let total = graph.add_node(RegionNode::new(
            RegionNodeKind::Add,
            vec![sum01, p2],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ));

        let compiled = compile_scalar_int_region(&graph, total).expect("region compiles");
        assert_eq!(compiled.buffer_slots, 5);
        assert_eq!(compiled.result_slot, 4);
        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(*mut JitCValue) -> i32`
        // over a read-execute region; each buffer below has `buffer_slots` live,
        // aligned, contiguous `JitCValue`s that outlive the call.
        let run: extern "C" fn(*mut JitCValue) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        // Success: (11 + 20) + 11 = 42, result in slot 4.
        let mut slots = [
            JitCValue::int(11),
            JitCValue::int(20),
            JitCValue::int(11),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[4].tag, JitCValueTag::Int);
        assert_eq!(slots[4].payload as i64, 42);

        // A non-Int marshaled local takes the side exit; the result is untouched.
        let mut typed = [
            JitCValue::int(1),
            JitCValue::null(),
            JitCValue::int(3),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(typed.as_mut_ptr()), 1);
        assert_eq!(typed[4].tag, JitCValueTag::Uninitialized);
    }

    // The widened scalar-int compiler executing sub, const, and overflow-guarded
    // mul over a real RegionGraph: result = (p0 - p1) * 10.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_compiled_sub_mul_const_region() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::compile_scalar_int_region;
        use crate::region_ir::{
            NodeId, RegionConst, RegionEffects, RegionGraph, RegionId, RegionNode, RegionNodeKind,
            RegionPlacement, RegionValueType, VmSlotId,
        };

        fn i64_node(graph: &mut RegionGraph, kind: RegionNodeKind, inputs: Vec<NodeId>) -> NodeId {
            graph.add_node(RegionNode::new(
                kind,
                inputs,
                None,
                RegionValueType::I64,
                RegionPlacement::Floating,
                RegionEffects::PURE,
            ))
        }

        let mut graph = RegionGraph::new(RegionId::new(8), "region-sub-mul");
        let p0 = i64_node(
            &mut graph,
            RegionNodeKind::Param {
                slot: VmSlotId::new(0),
            },
            Vec::new(),
        );
        let p1 = i64_node(
            &mut graph,
            RegionNodeKind::Param {
                slot: VmSlotId::new(1),
            },
            Vec::new(),
        );
        let diff = i64_node(&mut graph, RegionNodeKind::Sub, vec![p0, p1]);
        let ten_const = graph.add_constant(RegionConst::I64(10));
        let ten = i64_node(&mut graph, RegionNodeKind::Const(ten_const), Vec::new());
        let scaled = i64_node(&mut graph, RegionNodeKind::Mul, vec![diff, ten]);

        let compiled = compile_scalar_int_region(&graph, scaled).expect("region compiles");
        assert_eq!(compiled.result_slot, 4);
        assert_eq!(compiled.buffer_slots, 5);
        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute
        // region; each buffer is 5 live, aligned, contiguous `JitCValue`s.
        let run: extern "C" fn(*mut JitCValue) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        // Success: (17 - 13) * 10 = 40, result in slot 4.
        let mut slots = [
            JitCValue::int(17),
            JitCValue::int(13),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[4].tag, JitCValueTag::Int);
        assert_eq!(slots[4].payload as i64, 40);

        // Multiplication overflow takes the side exit: (i64::MAX - 0) * 10.
        let mut overflow = [
            JitCValue::int(i64::MAX),
            JitCValue::int(0),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(overflow.as_mut_ptr()), 1);
        assert_eq!(overflow[4].tag, JitCValueTag::Uninitialized);
    }

    // Steady-state native throughput of the guarded int-add stencil: compile
    // once, finalize one CodeMemory, then call the emitted fn in a tight loop
    // over a pre-filled buffer (no per-call mmap or marshaling). Reports ns per
    // native op to compare against the interpreter's measured ~50 ns/op. Ignored
    // by default; run with:
    //   cargo test --release -p php_jit --ignored --nocapture bench_native_scalar_int
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    #[ignore = "timing benchmark; run with --release --ignored --nocapture"]
    fn bench_native_scalar_int_throughput() {
        use crate::JitCValue;
        use crate::copy_patch::compile_scalar_int_region;
        use crate::region_ir::{
            NodeId, RegionEffects, RegionGraph, RegionId, RegionNode, RegionNodeKind,
            RegionPlacement, RegionValueType, VmSlotId,
        };
        use std::time::Instant;

        fn i64_node(graph: &mut RegionGraph, kind: RegionNodeKind, inputs: Vec<NodeId>) -> NodeId {
            graph.add_node(RegionNode::new(
                kind,
                inputs,
                None,
                RegionValueType::I64,
                RegionPlacement::Floating,
                RegionEffects::PURE,
            ))
        }

        // A chain of ADD_OPS guarded int adds: acc = p0 + p1, then acc += p1.
        const ADD_OPS: usize = 500; // buffer stays under the addressable slot bound
        let mut graph = RegionGraph::new(RegionId::new(99), "bench-add-chain");
        let p0 = i64_node(
            &mut graph,
            RegionNodeKind::Param {
                slot: VmSlotId::new(0),
            },
            Vec::new(),
        );
        let p1 = i64_node(
            &mut graph,
            RegionNodeKind::Param {
                slot: VmSlotId::new(1),
            },
            Vec::new(),
        );
        let mut acc = i64_node(&mut graph, RegionNodeKind::Add, vec![p0, p1]);
        for _ in 1..ADD_OPS {
            acc = i64_node(&mut graph, RegionNodeKind::Add, vec![acc, p1]);
        }
        let compiled = compile_scalar_int_region(&graph, acc).expect("region compiles");

        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        let mut buffer = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        buffer[0] = JitCValue::int(1);
        buffer[1] = JitCValue::int(1);

        for _ in 0..1000 {
            assert_eq!(run(buffer.as_mut_ptr()), 0); // warm i-cache / predictor
        }

        let iters: u64 = 200_000;
        let start = Instant::now();
        for _ in 0..iters {
            std::hint::black_box(run(buffer.as_mut_ptr()));
        }
        let elapsed = start.elapsed();

        let total_ops = iters * ADD_OPS as u64;
        let ns_per_op = elapsed.as_nanos() as f64 / total_ops as f64;
        println!(
            "native scalar-int add: {ns_per_op:.3} ns/op  ({iters} iters x {ADD_OPS} ops = {total_ops} ops in {elapsed:?})"
        );
        assert_eq!(
            buffer[compiled.result_slot as usize].payload as i64,
            ADD_OPS as i64 + 1
        );
    }

    #[test]
    #[ignore = "timing benchmark; run with --release --ignored --nocapture"]
    fn bench_native_float_throughput() {
        use crate::JitCValue;
        use crate::copy_patch::{FloatBinOp, ScalarFloatOp, emit_scalar_float_ops};
        use std::time::Instant;

        // A chain of guarded float adds: acc = a + a, then acc += a repeatedly.
        const ADD_OPS: usize = 500;
        let mut ops = vec![ScalarFloatOp::Binary {
            op: FloatBinOp::Add,
            dst: 2,
            lhs: 0,
            rhs: 0,
        }];
        for _ in 1..ADD_OPS {
            ops.push(ScalarFloatOp::Binary {
                op: FloatBinOp::Add,
                dst: 2,
                lhs: 2,
                rhs: 0,
            });
        }
        let code = emit_scalar_float_ops(&ops).expect("float ops emit");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        let mut buffer = [
            JitCValue::float(1.0),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        for _ in 0..1000 {
            assert_eq!(run(buffer.as_mut_ptr()), 0);
        }

        let iters: u64 = 200_000;
        let start = Instant::now();
        for _ in 0..iters {
            std::hint::black_box(run(buffer.as_mut_ptr()));
        }
        let elapsed = start.elapsed();

        let total_ops = iters * ADD_OPS as u64;
        let ns_per_op = elapsed.as_nanos() as f64 / total_ops as f64;
        println!(
            "native scalar-float add: {ns_per_op:.3} ns/op  ({iters} iters x {ADD_OPS} ops = {total_ops} ops in {elapsed:?})"
        );
        // acc = 1.0 (a+a) then +1.0 each of the remaining ops => ADD_OPS + 1.
        assert_eq!(f64::from_bits(buffer[2].payload), (ADD_OPS as f64) + 1.0);
    }

    // A native counted loop executed end-to-end: for (i=0; i<n; i++) s += i.
    // The whole loop runs natively with no per-iteration interpreter dispatch —
    // the shape where the tier's real speedup lives.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_counted_loop() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{CountedLoop, IntBinOp, ScalarIntOp, emit_counted_loop};

        // slots: i=0 (counter), n=1 (limit), s=2 (accumulator).
        let counted = CountedLoop {
            prologue: vec![
                ScalarIntOp::Const { dst: 0, value: 0 }, // i = 0
                ScalarIntOp::Const { dst: 2, value: 0 }, // s = 0
            ],
            counter: 0,
            limit: 1,
            body: vec![ScalarIntOp::Binary {
                op: IntBinOp::Add,
                dst: 2,
                lhs: 2,
                rhs: 0,
            }], // s = s + i
        };
        let code = emit_counted_loop(&counted).expect("loop emits");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        // n = 100 -> sum(0..99) = 4950.
        let mut slots = [
            JitCValue::uninitialized(),
            JitCValue::int(100),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[2].tag, JitCValueTag::Int);
        assert_eq!(slots[2].payload as i64, 4950);

        // n = 0 -> the body never runs, s stays 0.
        let mut zero = [
            JitCValue::uninitialized(),
            JitCValue::int(0),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(zero.as_mut_ptr()), 0);
        assert_eq!(zero[2].payload as i64, 0);
    }

    #[test]
    fn executes_native_loop_with_const_operand_body() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{CountedLoop, IntBinOp, ScalarIntOp, emit_counted_loop};

        // slots: i=0 (counter), n=1 (limit), s=2 (accumulator).
        // Body `s = s + 5` runs once per iteration -> s = 5 * n.
        let counted = CountedLoop {
            prologue: vec![
                ScalarIntOp::Const { dst: 0, value: 0 },
                ScalarIntOp::Const { dst: 2, value: 0 },
            ],
            counter: 0,
            limit: 1,
            body: vec![ScalarIntOp::BinaryConst {
                op: IntBinOp::Add,
                dst: 2,
                lhs: 2,
                rhs: 5,
            }],
        };
        let code = emit_counted_loop(&counted).expect("const-body loop emits");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        // n = 10 -> s = 5 * 10 = 50.
        let mut slots = [
            JitCValue::uninitialized(),
            JitCValue::int(10),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[2].tag, JitCValueTag::Int);
        assert_eq!(slots[2].payload as i64, 50);
    }

    #[test]
    fn executes_native_loop_with_bitwise_const_body() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{CountedLoop, IntBinOp, ScalarIntOp, emit_counted_loop};

        // slots: i=0 (counter), n=1 (limit), s=2 (accumulator).
        // Body `s = s | 5` OR-folds a constant; idempotent once the bits are set.
        let counted = CountedLoop {
            prologue: vec![
                ScalarIntOp::Const { dst: 0, value: 0 },
                ScalarIntOp::Const { dst: 2, value: 2 }, // s starts at 0b010
            ],
            counter: 0,
            limit: 1,
            body: vec![ScalarIntOp::BinaryConst {
                op: IntBinOp::BitOr,
                dst: 2,
                lhs: 2,
                rhs: 5, // 0b101
            }],
        };
        let code = emit_counted_loop(&counted).expect("bitwise const-body loop emits");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        // n = 3 -> after any iteration, s = 0b010 | 0b101 = 0b111 = 7.
        let mut slots = [
            JitCValue::uninitialized(),
            JitCValue::int(3),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[2].tag, JitCValueTag::Int);
        assert_eq!(slots[2].payload as i64, 7);
    }

    /// Build, finalize, and invoke a flat scalar-int op sequence over `slots`,
    /// returning the side-exit code (0 = completed, 1 = deopt).
    fn run_scalar_ops(
        ops: &[crate::copy_patch::ScalarIntOp],
        slots: &mut [crate::JitCValue],
    ) -> i32 {
        use crate::JitCValue;
        use crate::copy_patch::emit_scalar_int_ops;
        let code = emit_scalar_int_ops(ops).expect("scalar-int ops emit");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };
        run(slots.as_mut_ptr())
    }

    /// Flat scalar-**float** op-sequence variant of [`run_scalar_ops`].
    fn run_scalar_float_ops(
        ops: &[crate::copy_patch::ScalarFloatOp],
        slots: &mut [crate::JitCValue],
    ) -> i32 {
        use crate::JitCValue;
        use crate::copy_patch::emit_scalar_float_ops;
        let code = emit_scalar_float_ops(ops).expect("scalar-float ops emit");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };
        run(slots.as_mut_ptr())
    }

    // The call stencil executed end-to-end: `slot[dst] = abs(slot[arg])`
    // emitted as a real `blr` into the `phrust_jit_abs_i64` VM helper (fp/lr
    // saved, a 16-byte scratch frame for the out value and the slot base). This
    // is the safe subset of native-tier gap (b): a pure C-ABI call to a pure
    // function, with the `abs(INT_MIN)` overflow taking the side exit so the
    // interpreter produces PHP's float result.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_abs_call_stencil() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{IntBinOp, ScalarIntOp};

        // abs(5) = 5 and abs(-7) = 7, written to slot[1].
        for (input, expected) in [(5i64, 5i64), (-7, 7), (0, 0), (i64::MAX, i64::MAX)] {
            let mut slots = [JitCValue::int(input), JitCValue::uninitialized()];
            assert_eq!(
                run_scalar_ops(&[ScalarIntOp::CallAbsI64 { dst: 1, arg: 0 }], &mut slots),
                0,
                "abs({input}) runs natively"
            );
            assert_eq!(slots[1].tag, JitCValueTag::Int);
            assert_eq!(slots[1].payload as i64, expected);
        }

        // abs(PHP_INT_MIN) overflows i64 (PHP returns a float): the helper
        // reports fallback, so the stencil side-exits and leaves slot[1] alone.
        let mut min = [JitCValue::int(i64::MIN), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[ScalarIntOp::CallAbsI64 { dst: 1, arg: 0 }], &mut min),
            1,
            "abs(INT_MIN) takes the side exit"
        );
        assert_eq!(min[1].tag, JitCValueTag::Uninitialized);

        // A non-Int argument trips the type guard before the call.
        let mut bad = [JitCValue::null(), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[ScalarIntOp::CallAbsI64 { dst: 1, arg: 0 }], &mut bad),
            1,
            "a non-Int argument side-exits at the guard"
        );

        // The helper result feeds a following op — the `abs($x) + 1` shape:
        // slot[2] = abs(slot[0]); slot[3] = slot[2] + slot[1].
        let mut chain = [
            JitCValue::int(-5),
            JitCValue::int(1),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[
                    ScalarIntOp::CallAbsI64 { dst: 2, arg: 0 },
                    ScalarIntOp::Binary {
                        op: IntBinOp::Add,
                        dst: 3,
                        lhs: 2,
                        rhs: 1,
                    },
                ],
                &mut chain,
            ),
            0
        );
        assert_eq!(chain[2].payload as i64, 5, "abs(-5) = 5 written to slot 2");
        assert_eq!(chain[3].tag, JitCValueTag::Int);
        assert_eq!(chain[3].payload as i64, 6, "abs(-5) + 1 = 6");
    }

    // The count() call stencil executed end-to-end: `slot[dst] = count(slot[arg])`
    // over a read-only borrowed array handle. The array crosses as an
    // `OpaqueArray` slot whose payload is a pointer the emitted code passes to a
    // runtime-resolved `php_jit_array_len` ABI wrapper (fp/lr saved, a 16-byte
    // scratch frame for the out length and the slot base). This is the first
    // heap-handle shape of the native tier: the array-tag guard fires before the
    // call (a non-array side-exits), and a helper fallback (a non-packed array in
    // the VM) also side-exits — only a plain packed array returns natively. The
    // helper here stands in for `php_runtime::php_jit_array_len` (php_jit cannot
    // depend on php_runtime), exercising the exact call boundary and ABI.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_count_call_stencil() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::ScalarIntOp;

        // Stand-in for the runtime `php_jit_array_len` ABI wrapper: read the array
        // length the test parked at `value_ptr` and write it out. A negative
        // sentinel reports a non-OK status, modeling the helper's fallback for a
        // non-packed (hashed) array so the stencil side-exits.
        extern "C" fn test_array_len(value_ptr: usize, out: *mut i64) -> i32 {
            if value_ptr == 0 || out.is_null() {
                return 1;
            }
            // SAFETY: the test parks a live `i64` at `value_ptr` for the call.
            let len = unsafe { *(value_ptr as *const i64) };
            if len < 0 {
                return 1; // helper fallback -> side exit
            }
            // SAFETY: `out` is the stencil's stack out-slot, valid for the call.
            unsafe {
                *out = len;
            }
            crate::JIT_HELPER_STATUS_OK
        }

        let helper = test_array_len as *const () as usize as u64;
        let array_slot = |len: &i64| JitCValue {
            tag: JitCValueTag::OpaqueArray,
            reserved: 0,
            payload: (len as *const i64) as u64,
            aux: 0,
        };
        let count_op = ScalarIntOp::CallCountI64 {
            dst: 1,
            arg: 0,
            array_len_helper: helper,
        };

        // A borrowed packed-array handle of length 3: the fast path runs the
        // helper and writes the Int length to slot[1].
        let length: i64 = 3;
        let mut slots = [array_slot(&length), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[count_op], &mut slots),
            0,
            "count over a packed array handle runs natively"
        );
        assert_eq!(slots[1].tag, JitCValueTag::Int);
        assert_eq!(slots[1].payload as i64, 3);

        // A non-array argument (an Int) trips the array-tag guard before the call
        // — the helper is never reached and the result slot is untouched.
        let mut not_array = [JitCValue::int(5), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[count_op], &mut not_array),
            1,
            "a non-array argument side-exits at the tag guard"
        );
        assert_eq!(not_array[1].tag, JitCValueTag::Uninitialized);

        // Uninitialized (how the bridge marshals a Countable object or any
        // non-array heap value) also side-exits at the guard.
        let mut uninit = [JitCValue::uninitialized(), JitCValue::uninitialized()];
        assert_eq!(run_scalar_ops(&[count_op], &mut uninit), 1);
        assert_eq!(uninit[1].tag, JitCValueTag::Uninitialized);

        // A genuine array handle whose helper reports fallback (a non-packed array
        // in the VM) side-exits after the call, leaving the result untouched.
        let sentinel: i64 = -1;
        let mut hashed = [array_slot(&sentinel), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[count_op], &mut hashed),
            1,
            "a helper fallback (non-packed array) side-exits"
        );
        assert_eq!(hashed[1].tag, JitCValueTag::Uninitialized);
    }

    // The strlen() call stencil executed end-to-end: `slot[dst] = strlen(slot[arg])`
    // over a read-only borrowed string handle. Mirrors the count stencil — the
    // string crosses as an `OpaqueString` slot whose payload is a pointer the
    // emitted code passes to a runtime-resolved string-length ABI wrapper. The
    // string-tag guard fires before the call (a non-string side-exits), and a
    // helper fallback (an unexpected shape in the VM) also side-exits. The helper
    // here stands in for the VM's `copy_patch_strlen_abi` (php_jit cannot depend
    // on the VM), exercising the exact call boundary and ABI.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_strlen_call_stencil() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::ScalarIntOp;

        // Stand-in for the runtime string-length ABI wrapper: read the byte length
        // the test parked at `value_ptr` and write it out. A negative sentinel
        // reports a non-OK status, modeling the helper's fallback for a value it
        // cannot length so the stencil side-exits.
        extern "C" fn test_strlen(value_ptr: usize, out: *mut i64) -> i32 {
            if value_ptr == 0 || out.is_null() {
                return 1;
            }
            // SAFETY: the test parks a live `i64` at `value_ptr` for the call.
            let len = unsafe { *(value_ptr as *const i64) };
            if len < 0 {
                return 1; // helper fallback -> side exit
            }
            // SAFETY: `out` is the stencil's stack out-slot, valid for the call.
            unsafe {
                *out = len;
            }
            crate::JIT_HELPER_STATUS_OK
        }

        let helper = test_strlen as *const () as usize as u64;
        let string_slot = |len: &i64| JitCValue {
            tag: JitCValueTag::OpaqueString,
            reserved: 0,
            payload: (len as *const i64) as u64,
            aux: 0,
        };
        let strlen_op = ScalarIntOp::CallStrlenI64 {
            dst: 1,
            arg: 0,
            strlen_helper: helper,
        };

        // A borrowed string handle of byte length 5: the fast path runs the helper
        // and writes the Int length to slot[1].
        let length: i64 = 5;
        let mut slots = [string_slot(&length), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[strlen_op], &mut slots),
            0,
            "strlen over a string handle runs natively"
        );
        assert_eq!(slots[1].tag, JitCValueTag::Int);
        assert_eq!(slots[1].payload as i64, 5);

        // Empty string -> length 0 still runs natively.
        let zero: i64 = 0;
        let mut empty = [string_slot(&zero), JitCValue::uninitialized()];
        assert_eq!(run_scalar_ops(&[strlen_op], &mut empty), 0);
        assert_eq!(empty[1].payload as i64, 0);

        // A non-string argument (an Int) trips the string-tag guard before the
        // call — the helper is never reached and the result slot is untouched.
        let mut not_string = [JitCValue::int(5), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[strlen_op], &mut not_string),
            1,
            "a non-string argument side-exits at the tag guard"
        );
        assert_eq!(not_string[1].tag, JitCValueTag::Uninitialized);

        // Uninitialized (how the bridge marshals null / an object / any non-string
        // heap value) also side-exits at the guard.
        let mut uninit = [JitCValue::uninitialized(), JitCValue::uninitialized()];
        assert_eq!(run_scalar_ops(&[strlen_op], &mut uninit), 1);
        assert_eq!(uninit[1].tag, JitCValueTag::Uninitialized);

        // A genuine string handle whose helper reports fallback side-exits after
        // the call, leaving the result untouched.
        let sentinel: i64 = -1;
        let mut bad = [string_slot(&sentinel), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[strlen_op], &mut bad),
            1,
            "a helper fallback side-exits"
        );
        assert_eq!(bad[1].tag, JitCValueTag::Uninitialized);
    }

    // The monomorphic property-load call stencil executed end-to-end:
    // `slot[dst] = <object>-><declared scalar property>` over a read-only
    // borrowed object handle. The object crosses as an `OpaqueObject` slot whose
    // payload is a pointer the emitted code passes to a property-load ABI wrapper,
    // along with a borrowed layout-metadata pointer (arg x1) and a full-`JitCValue`
    // out slot (arg x2, a 32-byte scratch frame that also saves the slot base).
    // The object-tag guard fires before the call (a non-object side-exits), and
    // the helper's layout guard side-exits for a non-matching class — so a
    // polymorphic call site never reads a wrong slot. The helper here stands in
    // for the VM's `copy_patch_property_load_abi` (php_jit cannot depend on the
    // VM), exercising the exact call boundary, the OpaqueObject tag, the layout
    // guard, and the marshaled scalar result.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_property_load_call_stencil() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::ScalarIntOp;

        // Stand-in for the VM's copy_patch_property_load_abi. The object handle's
        // payload points at a `TestObject { layout_id, scalar }` (modeling an
        // object at a known layout holding a scalar property); the metadata
        // pointer points at the expected layout id. A matching layout marshals the
        // Int scalar into the out JitCValue and returns OK; a mismatch reports a
        // non-OK status (the layout side exit) so the stencil defers to the
        // interpreter, never reading a wrong slot.
        #[repr(C)]
        struct TestObject {
            layout_id: u64,
            scalar: i64,
        }
        extern "C" fn test_property_load(
            value_ptr: usize,
            metadata_ptr: usize,
            out: *mut JitCValue,
        ) -> i32 {
            if value_ptr == 0 || metadata_ptr == 0 || out.is_null() {
                return crate::JIT_HELPER_STATUS_FALLBACK;
            }
            // SAFETY: the test parks a live `TestObject` at `value_ptr` and a live
            // expected-layout `u64` at `metadata_ptr` for the call's duration.
            let object = unsafe { &*(value_ptr as *const TestObject) };
            let expected = unsafe { *(metadata_ptr as *const u64) };
            if object.layout_id != expected {
                return crate::JIT_HELPER_STATUS_FALLBACK; // layout mismatch -> side exit
            }
            // SAFETY: `out` is the stencil's stack scratch, a valid 24-byte JitCValue.
            unsafe {
                *out = JitCValue::int(object.scalar);
            }
            crate::JIT_HELPER_STATUS_OK
        }

        let helper = test_property_load as *const () as usize as u64;
        let expected_layout: u64 = 0xABCD;
        let metadata_ptr = (&expected_layout as *const u64) as u64;
        let object_slot = |obj: &TestObject| JitCValue {
            tag: JitCValueTag::OpaqueObject,
            reserved: 0,
            payload: (obj as *const TestObject) as u64,
            aux: 0,
        };
        let op = ScalarIntOp::CallPropertyLoadScalar {
            dst: 1,
            arg: 0,
            metadata_ptr,
            helper,
        };

        // Matching layout: the property read runs natively and writes the Int
        // scalar to slot[1].
        let matching = TestObject {
            layout_id: 0xABCD,
            scalar: 42,
        };
        let mut slots = [object_slot(&matching), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[op], &mut slots),
            0,
            "a matching-layout property load runs natively"
        );
        assert_eq!(slots[1].tag, JitCValueTag::Int);
        assert_eq!(slots[1].payload as i64, 42);

        // Layout mismatch (a different class reaching the same monomorphic site):
        // the helper reports non-OK, so the stencil side-exits and the result slot
        // is untouched — the guard fires before any commit, never a wrong slot.
        let mismatch = TestObject {
            layout_id: 0x1234,
            scalar: 99,
        };
        let mut wrong = [object_slot(&mismatch), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[op], &mut wrong),
            1,
            "a layout-mismatch object handle side-exits"
        );
        assert_eq!(wrong[1].tag, JitCValueTag::Uninitialized);

        // A non-object argument (an Int) trips the OpaqueObject tag guard before
        // the call — the helper is never reached and the result slot is untouched.
        let mut not_object = [JitCValue::int(7), JitCValue::uninitialized()];
        assert_eq!(
            run_scalar_ops(&[op], &mut not_object),
            1,
            "a non-object argument side-exits at the tag guard"
        );
        assert_eq!(not_object[1].tag, JitCValueTag::Uninitialized);

        // Uninitialized (how the bridge marshals a non-object/non-handle value)
        // also side-exits at the tag guard.
        let mut uninit = [JitCValue::uninitialized(), JitCValue::uninitialized()];
        assert_eq!(run_scalar_ops(&[op], &mut uninit), 1);
        assert_eq!(uninit[1].tag, JitCValueTag::Uninitialized);
    }

    // The is_TYPE() predicate stencils executed end-to-end: `slot[dst] =
    // (slot[arg].tag == expected_tag)`. No call and no payload deref — the answer
    // is exactly the marshaled tag. For each predicate, a matching marshaled value
    // yields Bool(true), every non-matching definite tag yields Bool(false), and
    // an Uninitialized-marshaled argument (null/object/etc.) side-exits so the
    // interpreter answers the ambiguous case.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_native_is_type_stencils() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::ScalarIntOp;

        const INT_TAG: u16 = JitCValueTag::Int as u16;
        const BOOL_TAG: u16 = JitCValueTag::Bool as u16;
        const FLOAT_TAG: u16 = JitCValueTag::FloatBits as u16;
        const STRING_TAG: u16 = JitCValueTag::OpaqueString as u16;
        const ARRAY_TAG: u16 = JitCValueTag::OpaqueArray as u16;

        // Borrowed-handle slots for the opaque tags; the is_* stencil only reads
        // the tag word, so the payload pointer is never dereferenced.
        let dummy: i64 = 0;
        let string_slot = JitCValue {
            tag: JitCValueTag::OpaqueString,
            reserved: 0,
            payload: (&dummy as *const i64) as u64,
            aux: 0,
        };
        let array_slot = JitCValue {
            tag: JitCValueTag::OpaqueArray,
            reserved: 0,
            payload: (&dummy as *const i64) as u64,
            aux: 0,
        };

        // (predicate tag, a value that marshals with that tag).
        let cases: [(u16, JitCValue); 5] = [
            (INT_TAG, JitCValue::int(7)),
            (BOOL_TAG, JitCValue::bool(true)),
            (FLOAT_TAG, JitCValue::float(1.5)),
            (STRING_TAG, string_slot),
            (ARRAY_TAG, array_slot),
        ];
        // One value per definite tag, to exercise every non-matching answer.
        let definite = [
            JitCValue::int(7),
            JitCValue::bool(false),
            JitCValue::float(0.0),
            string_slot,
            array_slot,
        ];

        for (expected_tag, matching) in cases {
            let op = ScalarIntOp::IsType {
                dst: 1,
                arg: 0,
                expected_tag,
            };

            // Matching tag -> Bool(true).
            let mut slots = [matching, JitCValue::uninitialized()];
            assert_eq!(run_scalar_ops(&[op], &mut slots), 0);
            assert_eq!(slots[1].tag, JitCValueTag::Bool);
            assert_eq!(slots[1].payload, 1, "is_type(matching tag) is true");

            // Every non-matching definite tag -> Bool(false).
            for other in definite {
                if other.tag as u16 == expected_tag {
                    continue;
                }
                let mut slots = [other, JitCValue::uninitialized()];
                assert_eq!(run_scalar_ops(&[op], &mut slots), 0);
                assert_eq!(slots[1].tag, JitCValueTag::Bool);
                assert_eq!(slots[1].payload, 0, "is_type(non-matching tag) is false");
            }

            // Uninitialized argument -> side exit (ambiguous; interpreter answers).
            let mut uninit = [JitCValue::uninitialized(), JitCValue::uninitialized()];
            assert_eq!(
                run_scalar_ops(&[op], &mut uninit),
                1,
                "an Uninitialized argument side-exits"
            );
            assert_eq!(uninit[1].tag, JitCValueTag::Uninitialized);
        }
    }

    #[test]
    fn executes_native_mod_shift_ops() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{IntBinOp, ScalarIntOp};

        // slot[2] = slot[0] % slot[1]; slot[3] = slot[0] << slot[1]; etc.
        // 17 % 5 = 2.
        let mut slots = [
            JitCValue::int(17),
            JitCValue::int(5),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Mod,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut slots,
            ),
            0
        );
        assert_eq!(slots[2].tag, JitCValueTag::Int);
        assert_eq!(slots[2].payload as i64, 2);

        // INT_MIN % -1 == 0 (aarch64 wraps the overflowing division to match PHP).
        let mut wrap = [
            JitCValue::int(i64::MIN),
            JitCValue::int(-1),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Mod,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut wrap,
            ),
            0
        );
        assert_eq!(wrap[2].payload as i64, 0);

        // 3 << 4 = 48 (arithmetic left shift).
        let mut shl = [
            JitCValue::int(3),
            JitCValue::int(4),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Shl,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut shl,
            ),
            0
        );
        assert_eq!(shl[2].payload as i64, 48);

        // -256 >> 2 = -64 (arithmetic right shift preserves the sign).
        let mut shr = [
            JitCValue::int(-256),
            JitCValue::int(2),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Shr,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut shr,
            ),
            0
        );
        assert_eq!(shr[2].payload as i64, -64);
    }

    #[test]
    fn executes_general_cfg_if_else_diamond() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::compile_scalar_int_function;
        use php_ir::instruction::TerminatorKind;
        use php_ir::{
            BasicBlock, BlockId, CompareOp, FunctionFlags, InstrId, Instruction, InstructionKind,
            IrParam, IrReturnType, IrSpan, LocalId, Operand, RegId, Terminator,
        };

        // function max2(int $a, int $b): int { if ($a > $b) return $a; return $b; }
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let load = |dst, local| {
            ins(InstructionKind::LoadLocal {
                dst: RegId::new(dst),
                local: LocalId::new(local),
            })
        };
        let ret = |reg| {
            Some(Terminator {
                span,
                kind: TerminatorKind::Return {
                    value: Some(Operand::Register(RegId::new(reg))),
                    by_ref_local: None,
                },
            })
        };
        let int_param = |name: &str, local| IrParam {
            name: name.to_string(),
            local: LocalId::new(local),
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        };
        let function = php_ir::IrFunction {
            name: "max2".to_string(),
            params: vec![int_param("a", 0), int_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 6,
            blocks: vec![
                BasicBlock {
                    id: BlockId::new(0),
                    instructions: vec![
                        load(0, 0),
                        load(1, 1),
                        ins(InstructionKind::Compare {
                            dst: RegId::new(2),
                            op: CompareOp::Greater,
                            lhs: Operand::Register(RegId::new(0)),
                            rhs: Operand::Register(RegId::new(1)),
                        }),
                    ],
                    terminator: Some(Terminator {
                        span,
                        kind: TerminatorKind::JumpIf {
                            condition: Operand::Register(RegId::new(2)),
                            if_true: BlockId::new(1),
                            if_false: BlockId::new(2),
                        },
                    }),
                },
                BasicBlock {
                    id: BlockId::new(1),
                    instructions: vec![load(3, 0)],
                    terminator: ret(3),
                },
                BasicBlock {
                    id: BlockId::new(2),
                    instructions: vec![load(4, 1)],
                    terminator: ret(4),
                },
            ],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        };

        let compiled =
            compile_scalar_int_function(&function, &[], 1).expect("max2 compiles via CFG path");
        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };
        let result = compiled.result_slot as usize;

        // Buffer holds all slots; params in slots 0,1. a > b -> returns a.
        let mut slots = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        slots[0] = JitCValue::int(7);
        slots[1] = JitCValue::int(3);
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[result].tag, JitCValueTag::Int);
        assert_eq!(slots[result].payload as i64, 7);

        // a <= b -> returns b (the else arm).
        let mut other = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        other[0] = JitCValue::int(2);
        other[1] = JitCValue::int(9);
        assert_eq!(run(other.as_mut_ptr()), 0);
        assert_eq!(other[result].payload as i64, 9);

        // Non-int argument -> the operand guard side-exits.
        let mut bad = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        bad[0] = JitCValue::int(7);
        // slot 1 left Uninitialized (not Int) -> the compare's guard deopts.
        assert_eq!(run(bad.as_mut_ptr()), 1);
    }

    #[test]
    fn executes_general_cfg_while_loop_with_back_edge() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::compile_scalar_int_function;
        use php_ir::instruction::TerminatorKind;
        use php_ir::{
            BasicBlock, BinaryOp, BlockId, CompareOp, ConstId, FunctionFlags, InstrId, Instruction,
            InstructionKind, IrConstant, IrParam, IrReturnType, IrSpan, LocalId, Operand, RegId,
            Terminator,
        };

        // function sumn(int $n): int {
        //   $s = 0; $i = 0; while ($i < $n) { $s = $s + $i; $i = $i + 1; } return $s;
        // }
        // A 4-block while loop (body folds in the increment), so the 5-block
        // counted-loop recognizer declines it and the CFG compiler emits the
        // back-edge itself. Locals: $n=0, $s=1, $i=2.
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let load = |dst, local| {
            ins(InstructionKind::LoadLocal {
                dst: RegId::new(dst),
                local: LocalId::new(local),
            })
        };
        let load_const = |dst, c| {
            ins(InstructionKind::LoadConst {
                dst: RegId::new(dst),
                constant: ConstId::new(c),
            })
        };
        let store = |local, reg| {
            ins(InstructionKind::StoreLocal {
                local: LocalId::new(local),
                src: Operand::Register(RegId::new(reg)),
            })
        };
        let add = |dst, lhs, rhs| {
            ins(InstructionKind::Binary {
                dst: RegId::new(dst),
                op: BinaryOp::Add,
                lhs: Operand::Register(RegId::new(lhs)),
                rhs: Operand::Register(RegId::new(rhs)),
            })
        };
        let jump = |target| {
            Some(Terminator {
                span,
                kind: TerminatorKind::Jump {
                    target: BlockId::new(target),
                },
            })
        };
        let function = php_ir::IrFunction {
            name: "sumn".to_string(),
            params: vec![IrParam {
                name: "n".to_string(),
                local: LocalId::new(0),
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            }],
            locals: vec!["n".to_string(), "s".to_string(), "i".to_string()],
            local_count: 3,
            register_count: 12,
            blocks: vec![
                // entry: $s = 0; $i = 0; jump header.
                BasicBlock {
                    id: BlockId::new(0),
                    instructions: vec![
                        load_const(0, 0),
                        store(1, 0),
                        load_const(1, 0),
                        store(2, 1),
                    ],
                    terminator: jump(1),
                },
                // header: $i < $n ? body : exit.
                BasicBlock {
                    id: BlockId::new(1),
                    instructions: vec![
                        load(2, 2),
                        load(3, 0),
                        ins(InstructionKind::Compare {
                            dst: RegId::new(4),
                            op: CompareOp::Less,
                            lhs: Operand::Register(RegId::new(2)),
                            rhs: Operand::Register(RegId::new(3)),
                        }),
                    ],
                    terminator: Some(Terminator {
                        span,
                        kind: TerminatorKind::JumpIf {
                            condition: Operand::Register(RegId::new(4)),
                            if_true: BlockId::new(2),
                            if_false: BlockId::new(3),
                        },
                    }),
                },
                // body: $s = $s + $i; $i = $i + 1; jump header (back-edge).
                BasicBlock {
                    id: BlockId::new(2),
                    instructions: vec![
                        load(5, 1),
                        load(6, 2),
                        add(7, 5, 6),
                        store(1, 7),
                        load(8, 2),
                        load_const(9, 1),
                        add(10, 8, 9),
                        store(2, 10),
                    ],
                    terminator: jump(1),
                },
                // exit: return $s.
                BasicBlock {
                    id: BlockId::new(3),
                    instructions: vec![load(11, 1)],
                    terminator: Some(Terminator {
                        span,
                        kind: TerminatorKind::Return {
                            value: Some(Operand::Register(RegId::new(11))),
                            by_ref_local: None,
                        },
                    }),
                },
            ],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        };
        let constants = [IrConstant::Int(0), IrConstant::Int(1)];

        let compiled = compile_scalar_int_function(&function, &constants, 1)
            .expect("while loop compiles via CFG path");
        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };
        let result = compiled.result_slot as usize;

        // n = 5 -> sum(0..4) = 10.
        let mut slots = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        slots[0] = JitCValue::int(5);
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[result].tag, JitCValueTag::Int);
        assert_eq!(slots[result].payload as i64, 10);

        // n = 0 -> the body never runs, $s stays 0.
        let mut zero = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        zero[0] = JitCValue::int(0);
        assert_eq!(run(zero.as_mut_ptr()), 0);
        assert_eq!(zero[result].payload as i64, 0);
    }

    #[test]
    fn executes_native_float_arithmetic() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{FloatBinOp, ScalarFloatOp};

        // slot[2] = (a + b) via a Const rhs and a Binary; a=1.5, then + 2.25.
        let mut slots = [
            JitCValue::float(1.5),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_float_ops(
                &[
                    ScalarFloatOp::Const {
                        dst: 1,
                        bits: 2.25f64.to_bits(),
                    },
                    ScalarFloatOp::Binary {
                        op: FloatBinOp::Add,
                        dst: 2,
                        lhs: 0,
                        rhs: 1,
                    },
                ],
                &mut slots,
            ),
            0
        );
        assert_eq!(slots[2].tag, JitCValueTag::FloatBits);
        assert_eq!(f64::from_bits(slots[2].payload), 3.75);

        // 10.0 / 4.0 = 2.5 (float division is float-typed).
        let mut div = [
            JitCValue::float(10.0),
            JitCValue::float(4.0),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_float_ops(
                &[ScalarFloatOp::Binary {
                    op: FloatBinOp::Div,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut div,
            ),
            0
        );
        assert_eq!(f64::from_bits(div[2].payload), 2.5);

        // Sub and Mul: (3.0 - 0.5) * 2.0 chained through slot 2.
        let mut chain = [
            JitCValue::float(3.0),
            JitCValue::float(0.5),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_float_ops(
                &[
                    ScalarFloatOp::Binary {
                        op: FloatBinOp::Sub,
                        dst: 2,
                        lhs: 0,
                        rhs: 1,
                    },
                    ScalarFloatOp::Const {
                        dst: 1,
                        bits: 2.0f64.to_bits(),
                    },
                    ScalarFloatOp::Binary {
                        op: FloatBinOp::Mul,
                        dst: 2,
                        lhs: 2,
                        rhs: 1,
                    },
                ],
                &mut chain,
            ),
            0
        );
        assert_eq!(f64::from_bits(chain[2].payload), 5.0);
    }

    #[test]
    fn executes_scalar_float_leaf_end_to_end() {
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::compile_scalar_int_function;
        use php_ir::instruction::TerminatorKind;
        use php_ir::{
            BasicBlock, BinaryOp, BlockId, FunctionFlags, InstrId, Instruction, InstructionKind,
            IrParam, IrReturnType, IrSpan, LocalId, Operand, RegId, Terminator,
        };

        // function div(float $a, float $b): float { return $a / $b; }
        let span = IrSpan::default();
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        let float_param = |name: &str, local| IrParam {
            name: name.to_string(),
            local: LocalId::new(local),
            required: true,
            default: None,
            type_: Some(IrReturnType::Float),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        };
        let function = php_ir::IrFunction {
            name: "div".to_string(),
            params: vec![float_param("a", 0), float_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 3,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(0),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(1),
                    }),
                    ins(InstructionKind::Binary {
                        dst: RegId::new(2),
                        op: BinaryOp::Div,
                        lhs: Operand::Register(RegId::new(0)),
                        rhs: Operand::Register(RegId::new(1)),
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(2))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Float),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        };

        let compiled = compile_scalar_int_function(&function, &[], 1).expect("float leaf compiles");
        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };
        let result = compiled.result_slot as usize;

        // 7.0 / 2.0 = 3.5.
        let mut slots = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        slots[0] = JitCValue::float(7.0);
        slots[1] = JitCValue::float(2.0);
        assert_eq!(run(slots.as_mut_ptr()), 0);
        assert_eq!(slots[result].tag, JitCValueTag::FloatBits);
        assert_eq!(f64::from_bits(slots[result].payload), 3.5);

        // Zero divisor -> side exit (interpreter raises DivisionByZeroError).
        let mut zero = vec![JitCValue::uninitialized(); compiled.buffer_slots as usize];
        zero[0] = JitCValue::float(1.0);
        zero[1] = JitCValue::float(0.0);
        assert_eq!(run(zero.as_mut_ptr()), 1);
    }

    #[test]
    fn float_div_by_zero_and_non_float_operand_side_exit() {
        use crate::JitCValue;
        use crate::copy_patch::{FloatBinOp, ScalarFloatOp};

        // x / 0.0 -> side exit (the interpreter raises DivisionByZeroError).
        let mut by_zero = [
            JitCValue::float(1.0),
            JitCValue::float(0.0),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_float_ops(
                &[ScalarFloatOp::Binary {
                    op: FloatBinOp::Div,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut by_zero,
            ),
            1
        );

        // -0.0 divisor also side-exits (fcmp treats +0.0 and -0.0 as equal).
        let mut neg_zero = [
            JitCValue::float(1.0),
            JitCValue::float(-0.0),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_float_ops(
                &[ScalarFloatOp::Binary {
                    op: FloatBinOp::Div,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut neg_zero,
            ),
            1
        );

        // A non-float (int) operand fails the FloatBits guard.
        let mut wrong_tag = [
            JitCValue::float(1.0),
            JitCValue::int(2),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_float_ops(
                &[ScalarFloatOp::Binary {
                    op: FloatBinOp::Add,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut wrong_tag,
            ),
            1
        );
    }

    #[test]
    fn mod_by_zero_and_out_of_range_shift_take_the_side_exit() {
        use crate::JitCValue;
        use crate::copy_patch::{IntBinOp, ScalarIntOp};

        // x % 0 -> side exit (the interpreter raises DivisionByZeroError).
        let mut by_zero = [
            JitCValue::int(10),
            JitCValue::int(0),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Mod,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut by_zero,
            ),
            1
        );

        // Shift amount 64 is outside PHP's 0..=63 domain -> side exit (aarch64
        // would otherwise mask it to 0 and return the operand unchanged).
        let mut wide = [
            JitCValue::int(1),
            JitCValue::int(64),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Shl,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut wide,
            ),
            1
        );

        // A negative shift amount reads as a huge unsigned value -> side exit.
        let mut neg = [
            JitCValue::int(1),
            JitCValue::int(-1),
            JitCValue::uninitialized(),
        ];
        assert_eq!(
            run_scalar_ops(
                &[ScalarIntOp::Binary {
                    op: IntBinOp::Shr,
                    dst: 2,
                    lhs: 0,
                    rhs: 1,
                }],
                &mut neg,
            ),
            1
        );
    }

    // Native loop throughput: sum 0..n natively vs the interpreter's ~50 ns/op.
    // Run with: cargo test --release -p php_jit --ignored --nocapture bench_native_counted_loop
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    #[ignore = "timing benchmark; run with --release --ignored --nocapture"]
    fn bench_native_counted_loop() {
        use crate::JitCValue;
        use crate::copy_patch::{CountedLoop, IntBinOp, ScalarIntOp, emit_counted_loop};
        use std::time::Instant;

        let counted = CountedLoop {
            prologue: vec![
                ScalarIntOp::Const { dst: 0, value: 0 },
                ScalarIntOp::Const { dst: 2, value: 0 },
            ],
            counter: 0,
            limit: 1,
            body: vec![ScalarIntOp::Binary {
                op: IntBinOp::Add,
                dst: 2,
                lhs: 2,
                rhs: 0,
            }],
        };
        let code = emit_counted_loop(&counted).expect("loop emits");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        const N: i64 = 30_000_000;
        let mut slots = [
            JitCValue::uninitialized(),
            JitCValue::int(N),
            JitCValue::uninitialized(),
        ];
        let start = Instant::now();
        assert_eq!(run(slots.as_mut_ptr()), 0);
        let elapsed = start.elapsed();
        let per_iter = elapsed.as_nanos() as f64 / N as f64;
        println!(
            "native counted loop: {per_iter:.3} ns/iter ({N} iters in {elapsed:?}), sum={}",
            slots[2].payload as i64
        );
        assert_eq!(slots[2].payload as i64, N * (N - 1) / 2);
    }

    // Guarded int comparison producing a Bool: slot[2] = slot[0] < slot[1].
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_int_compare_to_bool() {
        use crate::JitCValue;
        use crate::aarch64::Cond;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{ScalarIntOp, emit_scalar_int_ops};

        let code = emit_scalar_int_ops(&[ScalarIntOp::Compare {
            cond: Cond::LessThan,
            dst: 2,
            lhs: 0,
            rhs: 1,
        }])
        .expect("compare op emits");
        let mem = CodeMemory::new(&code).expect("code memory should finalize");
        // SAFETY: valid `extern "C" fn(*mut JitCValue) -> i32` over a read-execute region.
        let run: extern "C" fn(*mut JitCValue) -> i32 = unsafe {
            core::mem::transmute::<*const u8, extern "C" fn(*mut JitCValue) -> i32>(mem.as_ptr())
        };

        // 3 < 7 -> true.
        let mut t = [
            JitCValue::int(3),
            JitCValue::int(7),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(t.as_mut_ptr()), 0);
        assert_eq!(t[2].tag, JitCValueTag::Bool);
        assert_eq!(t[2].payload, 1);

        // 7 < 3 -> false.
        let mut f = [
            JitCValue::int(7),
            JitCValue::int(3),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(f.as_mut_ptr()), 0);
        assert_eq!(f[2].tag, JitCValueTag::Bool);
        assert_eq!(f[2].payload, 0);

        // Non-Int operand -> type-guard side exit.
        let mut ne = [
            JitCValue::int(1),
            JitCValue::null(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(ne.as_mut_ptr()), 1);
    }

    // The native->userland tail-call region executed end-to-end: the recognized
    // leaf `f($a,$b): int { return g($a + $b); }` computes the argument `$a + $b`
    // natively, leaves it in the plan's argument slot, and returns the tail-call
    // status. A non-int marshaled argument takes the shared side exit instead.
    #[cfg(all(unix, target_arch = "aarch64"))]
    #[test]
    fn executes_userland_tailcall_region() {
        use crate::JIT_HELPER_STATUS_TAILCALL;
        use crate::JitCValue;
        use crate::abi::JitCValueTag;
        use crate::copy_patch::{NativeCallPermits, compile_scalar_int_function_with_permits};
        use php_ir::instruction::{IrCallArg, IrCallArgValueKind, TerminatorKind};
        use php_ir::{
            BasicBlock, BinaryOp, BlockId, FunctionFlags, InstrId, Instruction, InstructionKind,
            IrParam, IrReturnType, IrSpan, LocalId, Operand, RegId, Terminator,
        };

        let span = IrSpan::default();
        let int_param = |name: &str, local: u32| IrParam {
            name: name.to_string(),
            local: LocalId::new(local),
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        };
        let ins = |kind| Instruction {
            id: InstrId::new(0),
            span,
            kind,
        };
        // `function f($a, $b): int { return g($a + $b); }`.
        let function = php_ir::IrFunction {
            name: "f".to_string(),
            params: vec![int_param("a", 0), int_param("b", 1)],
            locals: vec!["a".to_string(), "b".to_string()],
            local_count: 2,
            register_count: 4,
            blocks: vec![BasicBlock {
                id: BlockId::new(0),
                instructions: vec![
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(1),
                        local: LocalId::new(0),
                    }),
                    ins(InstructionKind::LoadLocal {
                        dst: RegId::new(2),
                        local: LocalId::new(1),
                    }),
                    ins(InstructionKind::Binary {
                        dst: RegId::new(3),
                        op: BinaryOp::Add,
                        lhs: Operand::Register(RegId::new(1)),
                        rhs: Operand::Register(RegId::new(2)),
                    }),
                    ins(InstructionKind::CallFunction {
                        dst: RegId::new(0),
                        name: "g".to_string(),
                        args: vec![IrCallArg {
                            name: None,
                            value: Operand::Register(RegId::new(3)),
                            unpack: false,
                            value_kind: IrCallArgValueKind::Direct,
                            by_ref_local: None,
                            by_ref_dim: None,
                            by_ref_property: None,
                            by_ref_property_dim: None,
                        }],
                    }),
                ],
                terminator: Some(Terminator {
                    span,
                    kind: TerminatorKind::Return {
                        value: Some(Operand::Register(RegId::new(0))),
                        by_ref_local: None,
                    },
                }),
            }],
            span,
            flags: FunctionFlags::default(),
            return_type: Some(IrReturnType::Int),
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        };

        let permits = NativeCallPermits {
            allow_userland_tailcall: true,
            ..NativeCallPermits::default()
        };
        let compiled = compile_scalar_int_function_with_permits(&function, &[], 1, permits)
            .expect("tail-call leaf compiles");
        let plan = compiled
            .tail_call
            .as_ref()
            .expect("records a tail-call plan");
        assert_eq!(plan.callee_name, "g");
        assert_eq!(plan.arg_slots, vec![6]);
        assert_eq!(compiled.buffer_slots, 7);

        let mem = CodeMemory::new(&compiled.code).expect("code memory should finalize");
        // SAFETY: the emitted bytes are a valid `extern "C" fn(*mut JitCValue) -> i32`
        // over a read-execute region; each buffer below has `buffer_slots` (7)
        // live, aligned, contiguous `JitCValue`s that outlive the call.
        let run: extern "C" fn(*mut JitCValue) -> i32 =
            unsafe { core::mem::transmute(mem.as_ptr()) };

        // Tail call requested: slot[0]=20, slot[1]=22 -> arg slot 6 holds 42.
        let mut slots = [
            JitCValue::int(20),
            JitCValue::int(22),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(slots.as_mut_ptr()), JIT_HELPER_STATUS_TAILCALL);
        assert_eq!(slots[6].tag, JitCValueTag::Int);
        assert_eq!(slots[6].payload as i64, 42);

        // A non-Int marshaled argument trips the prefix `Int` guard -> side exit;
        // the argument slot is left untouched (still Uninitialized).
        let mut typed = [
            JitCValue::null(),
            JitCValue::int(22),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
            JitCValue::uninitialized(),
        ];
        assert_eq!(run(typed.as_mut_ptr()), 1);
        assert_eq!(typed[6].tag, JitCValueTag::Uninitialized);
    }
}
