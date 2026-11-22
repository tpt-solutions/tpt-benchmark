//! Enables ANSI escape processing on the Windows console (stdout).

use std::ffi::c_void;
use std::io;

type Handle = *mut c_void;

/// `STD_OUTPUT_HANDLE` is `(DWORD)-11`.
const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

// SAFETY (declarations): these are the documented kernel32 console functions
// with their documented signatures. kernel32 is linked by the Rust standard
// library on every Windows target, so no extra link attribute is needed.
unsafe extern "system" {
    fn GetStdHandle(std_handle: u32) -> Handle;
    fn GetConsoleMode(console: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(console: Handle, mode: u32) -> i32;
}

/// Turns on ANSI escape processing for stdout.
///
/// Returns `Ok(())` when the console already processes escapes or was switched
/// to do so. Returns an error when stdout is not a console (for example when
/// it is redirected to a file), in which case escapes are not needed anyway.
pub(super) fn enable_virtual_terminal() -> io::Result<()> {
    // SAFETY: GetStdHandle takes no pointers and has no preconditions. The
    // returned handle is validated below before use.
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if handle.is_null() || handle as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let mut mode: u32 = 0;
    // SAFETY: `mode` is a live, writable u32 for the whole call. GetConsoleMode
    // returns 0 for a non-console handle, which is checked below.
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
        return Ok(());
    }
    // SAFETY: `handle` was just confirmed to be a console by GetConsoleMode.
    // SetConsoleMode only changes the console mode flags.
    if unsafe { SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
