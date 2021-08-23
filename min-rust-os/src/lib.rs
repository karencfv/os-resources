// Like the main.rs, the lib.rs is a special file that is automatically recognized by cargo.
// The library is a separate compilation unit, so we need to specify the #![no_std] attribute again.
#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;
use x86_64::instructions::port::Port;

pub mod gdt;
pub mod interrupts;
pub mod serial;
pub mod vga_buffer;

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

// `&[&dyn Fn()/Testable]` is a list of references to types that can be called like a function
pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failure);

    // Note that we still need an endless loop after the exit_qemu call because the compiler
    // does not know that the isa-debug-exit device causes a program exit.
    hlt_loop();
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

pub fn init() {
    gdt::init();
    interrupts::init_idt();
    unsafe { interrupts::PICS.lock().initialize() };
    // The interrupts::enable function of the x86_64 crate executes the special sti instruction
    // (“set interrupts”) to enable external interrupts.
    x86_64::instructions::interrupts::enable();
}

pub fn hlt_loop() -> ! {
    // The hlt assembly instruction puts the CPU in sleep mode until the next interrupt arrives.
    // this `hlt()` function is a thin wrapper around the assembly instruction.
    loop {
        x86_64::instructions::hlt();
    }
}

// Entry point for `cargo test`
#[cfg(test)]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();
    test_main();
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}
