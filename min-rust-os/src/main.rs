// This line unlinks the standard library. This is necessary in order
// to run the binary on bare metal without an underlying OS
#![no_std]
// This program has no access to the Rust runtime.
// It is necessary to overwrite the entry point (crt0).
#![no_main]
// Implement a custom testing framework for the kernel
#![feature(custom_test_frameworks)]
#![test_runner(min_rust_os::test_runner)]
// The kernel uses _start as the entry point instead of main.
// It's necessary to change the name of the generated function to
// something different than main through the reexport_test_harness_main attribute.
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;

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
    min_rust_os::test_panic_handler(info)
}

// The no_mangling attribute disables name mangling so the function name is `start()`
// instead of some random function name.
#[no_mangle]
// this function is the entry point, since the linker looks for a function
// named `_start` by default
pub extern "C" fn _start() -> ! {
    // vga_buffer::print_something();

    // The following function is not necessary, it is only used to exemplify what it's like
    // to write directly to VGA
    write_to_vga();

    println!("Hello from the hand crafted println macro :)");

    // Manually do what the previous function does:
    // write_to_vga_unsafe();

    min_rust_os::init();

    // Trigger a stack overflow
    //  stack_overflow();

    // Trigger a page fault
    // unsafe {
    // The 0xdeadbeef virtual address is not mapped to a physical address in the page tables, so a page fault occurs.
    // If a page fault handler hasn't been registered in IDT, so a double fault occurs. The CPU looks at the
    // IDT entry of the double fault handler. If this entry does not specify a handler function either
    // the kernel boot loops.
    //    *(0xdeadbeef as *mut u64) = 42;
    // };

    // Invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();

    #[cfg(test)]
    test_main();
    // After executing the tests, our test_runner returns to the test_main function,
    // which in turn returns to our _start entry point function.

    // Uncomment to panic
    // panic!("Some panic from the handcrafted panic macro");

    println!("It didn't crash!");
    // Uncomment loop if panic removed
    loop {}
}

fn write_to_vga() {
    // Write to BGA buffer directly
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
}

static HELLO: &[u8] = b"Hi from the minimal OS :)";

#[allow(dead_code)]
fn write_to_vga_unsafe() {
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
}

#[allow(dead_code)]
#[allow(unconditional_recursion)]
// For each recursion, the return address is pushed
fn stack_overflow() {
    stack_overflow();
}
