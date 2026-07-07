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
}
