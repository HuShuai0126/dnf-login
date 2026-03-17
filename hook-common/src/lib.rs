//! Shared utilities for x86 hook DLLs.

#![no_std]
#![allow(non_snake_case)]

#[cfg(not(target_arch = "x86"))]
compile_error!("This crate targets 32-bit x86 only");

use core::marker::PhantomData;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

pub type BOOL = i32;
pub type DWORD = u32;
pub type HMODULE = *mut core::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetModuleHandleA(lpModuleName: *const i8) -> HMODULE;
    pub fn GetProcAddress(hModule: HMODULE, lpProcName: *const i8) -> *mut core::ffi::c_void;
    pub fn DisableThreadLibraryCalls(hLibModule: HMODULE) -> BOOL;
    fn VirtualProtect(
        lpAddress: *mut core::ffi::c_void,
        dwSize: usize,
        flNewProtect: DWORD,
        lpflOldProtect: *mut DWORD,
    ) -> BOOL;
    fn VirtualAlloc(
        lpAddress: *mut core::ffi::c_void,
        dwSize: usize,
        flAllocationType: DWORD,
        flProtect: DWORD,
    ) -> *mut core::ffi::c_void;
    fn CreateFileA(
        lpFileName: *const i8,
        dwDesiredAccess: DWORD,
        dwShareMode: DWORD,
        lpSecurityAttributes: *mut core::ffi::c_void,
        dwCreationDisposition: DWORD,
        dwFlagsAndAttributes: DWORD,
        hTemplateFile: *mut core::ffi::c_void,
    ) -> *mut core::ffi::c_void;
    fn WriteFile(
        hFile: *mut core::ffi::c_void,
        lpBuffer: *const core::ffi::c_void,
        nNumberOfBytesToWrite: DWORD,
        lpNumberOfBytesWritten: *mut DWORD,
        lpOverlapped: *mut core::ffi::c_void,
    ) -> BOOL;
}

const MEM_COMMIT_RESERVE: DWORD = 0x3000;
const PAGE_EXECUTE_READWRITE: DWORD = 0x40;
const PAGE_EXECUTE_READ: DWORD = 0x20;

static LOG_HANDLE: AtomicUsize = AtomicUsize::new(0);
const LOG_FAILED: usize = usize::MAX;

const fn nibble_to_hex(n: u8) -> u8 {
    if n < 10 { b'0' + n } else { b'a' + n - 10 }
}

pub fn fmt_hex32(v: u32) -> [u8; 8] {
    let mut out = [b'0'; 8];
    let mut x = v;
    for i in (0..8).rev() {
        out[i] = nibble_to_hex((x & 0xF) as u8);
        x >>= 4;
    }
    out
}

pub fn fmt_hex8(v: u8) -> [u8; 2] {
    [nibble_to_hex(v >> 4), nibble_to_hex(v & 0xF)]
}

/// Opens a log file for writing. Subsequent calls are no-ops.
///
/// # Safety
/// `filename` must be a valid null-terminated C string pointer.
pub unsafe fn log_open(filename: *const i8) {
    if LOG_HANDLE
        .compare_exchange(0, LOG_FAILED, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let h = unsafe {
        CreateFileA(
            filename,
            0x4000_0000, // GENERIC_WRITE
            1,           // FILE_SHARE_READ
            ptr::null_mut(),
            2,    // CREATE_ALWAYS
            0x80, // FILE_ATTRIBUTE_NORMAL
            ptr::null_mut(),
        )
    };
    let val = if h.is_null() || h as usize == usize::MAX {
        LOG_FAILED
    } else {
        h as usize
    };
    LOG_HANDLE.store(val, Ordering::Release);
}

/// Writes all `parts` into a stack buffer and flushes with a single `WriteFile`
/// call to avoid interleaved output from concurrent threads.
///
/// # Safety
/// Must be called after `log_open`. Safe to call from any thread.
pub unsafe fn log_line(parts: &[&[u8]]) {
    let h = LOG_HANDLE.load(Ordering::Acquire) as *mut core::ffi::c_void;
    if h.is_null() || h as usize == LOG_FAILED {
        return;
    }
    // 256 bytes suffices for typical log lines; longer output is silently truncated.
    let mut buf = [0u8; 256];
    let mut pos = 0usize;
    for &part in parts {
        let n = part.len().min(buf.len() - pos);
        buf[pos..pos + n].copy_from_slice(&part[..n]);
        pos += n;
        if pos == buf.len() {
            break;
        }
    }
    let mut written: DWORD = 0;
    unsafe {
        WriteFile(
            h,
            buf.as_ptr().cast(),
            pos as DWORD,
            &mut written,
            ptr::null_mut(),
        );
    }
}

/// Emits a log line at most once over the lifetime of `$flag`.
#[macro_export]
macro_rules! log_once {
    ($flag:expr, $($part:expr),+ $(,)?) => {
        if !$flag.swap(true, ::core::sync::atomic::Ordering::Relaxed) {
            unsafe { $crate::log_line(&[$($part),+]) };
        }
    };
}

/// Stores a trampoline address as `AtomicUsize` with a typed accessor.
///
/// Written once with Release ordering during hook installation and read with
/// Acquire ordering from the hook function. No additional synchronisation is needed.
pub struct TrampolineSlot<F: Copy>(AtomicUsize, PhantomData<*const F>);

// SAFETY: The inner `AtomicUsize` provides atomic access. `PhantomData<*const F>`
// is only a marker; no actual `F` value is stored. The slot is written once
// with Release ordering during hook installation and read with Acquire ordering
// from the hook path.
unsafe impl<F: Copy> Sync for TrampolineSlot<F> {}

impl<F: Copy> Default for TrampolineSlot<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: Copy> TrampolineSlot<F> {
    pub const fn new() -> Self {
        Self(AtomicUsize::new(0), PhantomData)
    }

    pub fn as_raw(&self) -> &AtomicUsize {
        &self.0
    }

    /// Returns the stored function pointer, or `None` if not yet initialised.
    ///
    /// # Safety
    /// The value stored via `as_raw().store()` must be a valid, callable
    /// function pointer of type `F`. `size_of::<F>()` must equal
    /// `size_of::<usize>()`.
    pub unsafe fn get(&self) -> Option<F> {
        const { assert!(core::mem::size_of::<F>() == core::mem::size_of::<usize>()) };
        let v = self.0.load(Ordering::Acquire);
        if v == 0 {
            return None;
        }
        Some(unsafe { core::mem::transmute_copy(&v) })
    }
}

/// Decodes the absolute jump target from common 32-bit trampoline stubs.
///
/// Recognised patterns:
///   `E9 rel32`       -- near JMP
///   `68 imm32 C3`    -- PUSH imm32; RET
///
/// `FF 25 [addr32]` JMP DWORD PTR is not decoded here because the indirect
/// address may be unmapped. The prologue copy path handles it directly.
fn decode_hook_target(at: usize, bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 6 {
        return None;
    }
    if bytes[0] == 0xE9 {
        let rel = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        return Some((at as i32).wrapping_add(5).wrapping_add(rel) as usize);
    }
    if bytes[0] == 0x68 && bytes[5] == 0xC3 {
        return Some(u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize);
    }
    None
}

/// Returns the byte length of the x86 instruction at `code[0]`.
/// Covers opcodes found in typical 32-bit Windows API prologues.
fn x86_instr_len(code: &[u8]) -> usize {
    let Some(&op) = code.first() else {
        return 1;
    };
    match op {
        // Single-byte: PUSH/POP r32, NOP, RET, LEAVE, INT3, segment push/pop
        0x50..=0x5F
        | 0x90
        | 0xC3
        | 0xC9
        | 0xCC
        | 0x06
        | 0x07
        | 0x0E
        | 0x16
        | 0x17
        | 0x1E
        | 0x1F => 1,
        // 2-byte: PUSH imm8, JMP short
        0x6A | 0xEB => 2,
        // 3-byte: RET imm16
        0xC2 => 3,
        // ALU r/m,r and r,r/m: ADD, OR, ADC, SBB, AND, SUB, XOR, CMP
        0x00..=0x03 | 0x08..=0x0B | 0x10..=0x13 | 0x18..=0x1B
        | 0x20..=0x23 | 0x28..=0x2B | 0x30..=0x33 | 0x38..=0x3B
        // TEST r/m,r; MOV variants; LEA r,m
        | 0x84 | 0x85 | 0x88 | 0x89 | 0x8A | 0x8B | 0x8C | 0x8D | 0x8E => {
            1 + modrm_ext(code, 0)
        }
        // Group 1 imm8, 80/83: opcode + ModRM [+SIB] [+disp] + imm8
        0x80 | 0x83 => 1 + modrm_ext(code, 1),
        // Group 1 imm32, 81: opcode + ModRM [+SIB] [+disp] + imm32
        0x81 => 1 + modrm_ext(code, 4),
        // 5-byte: near JMP/CALL rel32, PUSH imm32, MOV r32, imm32
        0xE9 | 0xE8 | 0x68 | 0xB8..=0xBF => 5,
        // JMP DWORD PTR [mem32], FF 25: 6 bytes
        0xFF if code.len() > 1 && code[1] == 0x25 => 6,
        // Two-byte opcode escape, 0F xx
        0x0F if code.len() > 1 => match code[1] {
            0x80..=0x8F => 6, // Jcc near: 0F 8x rel32
            // Reg/mem forms: MOVZX, CMOVcc, IMUL, NOP r/m, etc.
            0x1F | 0x40..=0x4F | 0xAF | 0xB6 | 0xB7 | 0xBE | 0xBF => 1 + modrm_ext(&code[1..], 0),
            _ => 2,
        },
        // Fallback: treat as 1-byte, conservative for unrecognised opcodes in prologues
        _ => 1,
    }
}

/// Computes the extra byte count for a ModRM-encoded operand:
/// ModRM byte + optional SIB + optional displacement + `imm_extra`.
fn modrm_ext(code: &[u8], imm_extra: usize) -> usize {
    if code.len() < 2 {
        return 1 + imm_extra;
    }
    let modrm = code[1];
    let md = (modrm >> 6) & 3;
    let rm = modrm & 7;
    let has_sib = md != 3 && rm == 4;
    let sib_base = if has_sib && code.len() > 2 {
        code[2] & 7
    } else {
        0
    };
    let disp = match md {
        0 if rm == 5 => 4,
        0 if has_sib && sib_base == 5 => 4,
        1 => 1,
        2 => 4,
        _ => 0,
    };
    1 + has_sib as usize + disp + imm_extra
}

/// Returns the number of bytes to copy into the trampoline such that the copy
/// ends on an instruction boundary and spans at least 5 bytes.
fn trampoline_copy_size(code: &[u8]) -> usize {
    let mut n = 0;
    while n < 5 && n < code.len() {
        n += x86_instr_len(&code[n..]);
    }
    n.min(code.len())
}

/// Allocates an executable trampoline. If `target` is already a JMP stub, chains
/// directly. Otherwise copies the instruction-aligned prologue and jumps back.
unsafe fn alloc_trampoline(target: usize, saved: [u8; 8]) -> usize {
    let mem = unsafe {
        VirtualAlloc(
            ptr::null_mut(),
            32,
            MEM_COMMIT_RESERVE,
            PAGE_EXECUTE_READWRITE,
        )
    };
    if mem.is_null() {
        return 0;
    }
    let p = mem as *mut u8;

    let mut hex = [b'0'; 16];
    for (i, &b) in saved.iter().enumerate() {
        let [hi, lo] = fmt_hex8(b);
        hex[i * 2] = hi;
        hex[i * 2 + 1] = lo;
    }
    unsafe { log_line(&[b"[trampoline] bytes=", &hex, b"\n"]) };

    if let Some(dest) = decode_hook_target(target, &saved) {
        // Target is a JMP stub; chain directly to avoid double indirection.
        let new_rel = (dest as i32).wrapping_sub(mem as i32 + 5);
        unsafe {
            *p = 0xE9;
            ptr::write_unaligned(p.add(1) as *mut i32, new_rel);
        }
        unsafe { log_line(&[b"[trampoline] chain->0x", &fmt_hex32(dest as u32), b"\n"]) };
    } else {
        let n = trampoline_copy_size(&saved);
        unsafe { ptr::copy_nonoverlapping(saved.as_ptr(), p, n) };
        let rel = (target as i32 + n as i32).wrapping_sub(mem as i32 + n as i32 + 5);
        unsafe {
            *p.add(n) = 0xE9;
            ptr::write_unaligned(p.add(n + 1) as *mut i32, rel);
        }
        unsafe { log_line(&[b"[trampoline] copy+jmp n=0x", &fmt_hex8(n as u8), b"\n"]) };
    }
    // Downgrade from RWX to RX now that the trampoline is fully written.
    let mut old_prot: DWORD = 0;
    unsafe { VirtualProtect(mem, 32, PAGE_EXECUTE_READ, &mut old_prot) };
    mem as usize
}

/// Patches `target` with a near JMP to `hook` and stores the trampoline in
/// `trampoline_store` so the hook can call through to the original code.
///
/// The trampoline is stored with Release ordering before the JMP is written.
/// Any thread that reaches the hook must have executed the JMP first, so the
/// trampoline is always visible.
///
/// # Safety
/// - `target` must be a valid, executable address whose first 8 bytes are
///   readable. These bytes are copied into the trampoline as the saved prologue.
/// - The caller must ensure no other thread is concurrently executing the
///   first 5 bytes of `target` while the hook is being installed.
/// - Typically called during DllMain / DLL_PROCESS_ATTACH under loader lock,
///   which guarantees single-threaded execution.
pub unsafe fn install_hook(target: usize, hook: usize, trampoline_store: &AtomicUsize) -> bool {
    let target_ptr = target as *mut u8;
    let saved = unsafe { *(target as *const [u8; 8]) };

    let trampoline = unsafe { alloc_trampoline(target, saved) };
    if trampoline == 0 {
        return false;
    }
    trampoline_store.store(trampoline, Ordering::Release);

    let mut old_prot: DWORD = 0;
    if unsafe {
        VirtualProtect(
            target_ptr as *mut _,
            5,
            PAGE_EXECUTE_READWRITE,
            &mut old_prot,
        )
    } == 0
    {
        return false;
    }
    let rel = hook.wrapping_sub(target + 5) as i32;
    unsafe {
        *target_ptr = 0xE9;
        ptr::write_unaligned(target_ptr.add(1) as *mut i32, rel);
        VirtualProtect(target_ptr as *mut _, 5, old_prot, &mut old_prot);
    }
    true
}

/// Returns the export address of `func` from `dll`, or 0 if not found.
///
/// # Safety
/// `dll` and `func` must be null-terminated byte strings.
pub unsafe fn resolve(dll: &[u8], func: &[u8]) -> usize {
    let hmod = unsafe { GetModuleHandleA(dll.as_ptr() as *const i8) };
    if hmod.is_null() {
        return 0;
    }
    let addr = unsafe { GetProcAddress(hmod, func.as_ptr() as *const i8) };
    addr as usize
}

/// Returns true if `s` is a valid dotted-decimal IPv4 address without leading zeros.
pub fn parse_ipv4(s: &[u8]) -> bool {
    let mut count = 0usize;
    for part in s.splitn(5, |&b| b == b'.') {
        if part.is_empty() || part.len() > 3 {
            return false;
        }
        if part.len() > 1 && part[0] == b'0' {
            return false;
        }
        let mut val = 0u16;
        for &b in part {
            if !b.is_ascii_digit() {
                return false;
            }
            val = val * 10 + (b - b'0') as u16;
        }
        if val > 255 {
            return false;
        }
        count += 1;
    }
    count == 4
}

/// Parses "A.B.C.D" into `[A, B, C, D]`.
///
/// Caller must validate with `parse_ipv4` first.
pub fn parse_ipv4_octets(s: &[u8]) -> [u8; 4] {
    debug_assert!(parse_ipv4(s), "parse_ipv4_octets: input is not a valid IPv4 address");
    let mut octets = [0u8; 4];
    for (idx, part) in s.splitn(4, |&b| b == b'.').enumerate() {
        let mut val = 0u8;
        for &b in part {
            val = val.wrapping_mul(10).wrapping_add(b.wrapping_sub(b'0'));
        }
        octets[idx] = val;
    }
    octets
}
