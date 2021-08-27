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

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use min_rust_os::memory;
// use min_rust_os::memory::{active_level_4_table, translate_addr};
use x86_64::{structures::paging::Translate, VirtAddr};
// use x86_64::structures::paging::PageTable;

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
    min_rust_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    min_rust_os::test_panic_handler(info)
}

// This macro defines the real lower level `_start` entry point.
// It provides a type-checked way to define a Rust function as the entry point,
// so there is no need to define an `extern "C" fn _start()` function.
entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
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

    // Test a page fault
    // Instruction pointer that point to a code page. These are mapped read only by the bootloader.
    // let ptr = 0x2063a4 as *mut u32;
    // read from code page
    //    unsafe {
    //        let _x = *ptr;
    //    }
    // println!("read worked");
    // write to a code page (cannot be done as per the above comment)
    //    unsafe {
    //        *ptr = 42;
    //    }
    //    println!("write worked");

    // Accessing a page table currently active level 4 page table is stored at
    // address 0x1000 in physical memory, as indicated by the PhysAddr wrapper type.
    // Accessing physical memory directly is not possible when paging is active,
    // since programs could easily circumvent memory protection and access memory of other programs otherwise.
    // So the only way to access the table is through some virtual page that is mapped to the physical frame at address 0x1000.
    // This problem of creating mappings for page table frames is a general problem, since the kernel
    // needs to access the page tables regularly, for example when allocating a stack for a new thread.
    //    use x86_64::registers::control::Cr3;
    //    let (level_4_page_table, _) = Cr3::read();
    //    println!(
    //        "Level 4 page table at: {:?}",
    //        level_4_page_table.start_address()
    //    );
    //
    //    // Use memory module to traverse the entries of the level 4 table manually:
    //    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    //    let l4_table = unsafe { active_level_4_table(phys_mem_offset) };
    //
    //    for (i, entry) in l4_table.iter().enumerate() {
    //        if !entry.is_unused() {
    //            println!("L4 entry {}: {:?}", i, entry);
    //
    //            // To traverse the page tables further and take a look at a level 3 table,
    //            // we can take the mapped frame of an entry convert it to a virtual address again:
    //
    //            // Get physical address and convert it
    //            let phys = entry.frame().unwrap().start_address();
    //            let virt = phys.as_u64() + boot_info.physical_memory_offset;
    //            let ptr = VirtAddr::new(virt).as_mut_ptr();
    //            let l3_table: &PageTable = unsafe { &*ptr };
    //
    //            // print non-empty entries of level 3 table
    //            for (i, entry) in l3_table.iter().enumerate() {
    //                if !entry.is_unused() {
    //                    println!("L3 entry {}: {:?}", i, entry);
    //                }
    //            }
    //        }
    //    }

    // Use translation function
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mapper = unsafe { memory::init(phys_mem_offset) };
    let addresses = [
        // identity-mapped vga buffer page
        0xb8000,
        // some code page
        0x201008,
        // some stack page
        0x0100_0020_1a10,
        // virtual address mapped to a physical address 0
        boot_info.physical_memory_offset,
    ];
    for &address in &addresses {
        let virt = VirtAddr::new(address);
        //let phys = unsafe { translate_addr(virt, phys_mem_offset) };
        let phys = mapper.translate_addr(virt);
        println!("{:?} -> {:?}", virt, phys);
    }

    // Invoke a breakpoint exception
    // x86_64::instructions::interrupts::int3();

    #[cfg(test)]
    test_main();
    // After executing the tests, our test_runner returns to the test_main function,
    // which in turn returns to our _start entry point function.

    // Uncomment to panic
    // panic!("Some panic from the handcrafted panic macro");

    println!("It didn't crash!");
    // Uncomment loop if panic removed
    min_rust_os::hlt_loop();
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
