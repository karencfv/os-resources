// This line unlinks the standard library. This is necessary in order
// to run the binary on bare metal without an underlying OS
#![no_std]
// This program has no access to the Rust runtime.
// It is necessary to overwrite the entry point (crt0).
#![no_main]

use core::panic::PanicInfo;

// Since the standard library has been removed, Some functionality needs to be implemented manually.
// Like a panic_handler and stack unwinding which is used to to run the destructors of all live
// stack variables in case of a panic.
// See Cargo.toml where stack unwinding has been disabled as this exploratory OS does not have
// an API to determine the call-chain of a program yet.

// Defining the required panic_handler attribute.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// The no_mangling attribute disables name mangling so the function name is `start()`
// instead of some random function name.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // this function is the entry point, since the linker looks for a function
    // named `_start` by default
    loop {}
}
