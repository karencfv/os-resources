// This line unlinks the standard library. This is necessary in order
// to run the binary on bare metal without an underlying OS
#![no_std]
// This program has no access to the Rust runtime.
// It is necessary to overwrite the entry point (crt0).
#![no_main]
// Implement a custom testing framework for the kernel
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
// The kernel uses _start as the entry point instead of main.
// It's necessary to change the name of the generated function to
// something different than main through the reexport_test_harness_main attribute.
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
use x86_64::instructions::port::Port;

mod serial;
mod vga_buffer;

// Since the standard library has been removed, Some functionality needs to be implemented manually.
// Like a panic_handler and stack unwinding which is used to to run the destructors of all live
// stack variables in case of a panic.
// See Cargo.toml where stack unwinding has been disabled as this exploratory OS does not have
// an API to determine the call-chain of a program yet.

#[cfg(not(test))]
// Defining the required panic_handler attribute.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failure);

    // Note that we still need an endless loop after the exit_qemu call because the compiler
    // does not know that the isa-debug-exit device causes a program exit.
    loop {}
}

static HELLO: &[u8] = b"Hi from the minimal OS :)";
// The no_mangling attribute disables name mangling so the function name is `start()`
// instead of some random function name.
#[no_mangle]
// this function is the entry point, since the linker looks for a function
// named `_start` by default
pub extern "C" fn _start() -> ! {
    // vga_buffer::print_something();

    use core::fmt::Write;
    vga_buffer::WRITER
        .lock()
        .write_str("Allo Wört! \n")
        .unwrap();
    write!(
        vga_buffer::WRITER.lock(),
        "Print numbers {} and {} \n",
        42,
        1.0 / 3.0
    )
    .unwrap();

    println!("Hello from the hand crafted println macro :)");

    // Manually do what the previous function does:

    // The VGA buffer is located as address 0xb8000
    let vga_buffer_addr = 0xb8000 as *mut u8;

    for (i, &byte) in HELLO.iter().enumerate() {
        // Unsafe is used because Rust can't prove that these raw pointers are valid
        unsafe {
            // The offset() method is used to write the string byte and
            // corresponding colour byte (0xb is cyan)
            *vga_buffer_addr.offset(i as isize * 2) = byte;
            *vga_buffer_addr.offset(i as isize * 2 + 1) = 0xb;
        }
    }

    #[cfg(test)]
    test_main();
    // After executing the tests, our test_runner returns to the test_main function,
    // which in turn returns to our _start entry point function.

    // Uncomment to panic
    // panic!("Some panic from the handcrafted panic macro");

    // Uncomment loop if panic removed
    loop {}
}

pub trait Testable {
    fn run(&self) -> ();
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        // \t is the tab character
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    // We use exit code 0x10 for success and 0x11 for failure. The actual exit codes do not matter much,
    // as long as they don't clash with the default exit codes of QEMU. For example, using exit code 0
    // for success is not a good idea because it becomes (0 << 1) | 1 = 1
    Success = 0x10,
    Failure = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    // Using unsafe as writing to an I/O port can generally result in arbitrary behaviour.
    unsafe {
        // Instead of manually invoking the in and out assembly instructions, the x86_64 crate is used.
        // The function creates a new Port at 0xf4, which is the iobase of the isa-debug-exit device.
        // Then it writes the passed exit code to the port. We use u32 because we specified the iosize
        // of the isa-debug-exit device as 4 bytes.
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

#[cfg(test)]
// `&[&dyn Fn()]` is a list of references to types that can be called like a function
fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    exit_qemu(QemuExitCode::Success);
}
