use crate::println;
use lazy_static::lazy_static;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

// With the lazy_static macro instead of evaluating a static at compile time, the macro performs
// the initialization when the static is referenced the first time. Thus, we can do almost everything
// in the initialization block and are even able to read runtime values.
lazy_static! {
    // idt needs to be stored at a place where it has a 'static lifetime.
    // To achieve this we could allocate our IDT on the heap using Box and then convert it to a 'static reference,
    // but we are writing an OS kernel and thus don't have a heap (yet).
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt
    };
}

// Initialise interrupt descriptor table
pub fn init_idt() {
    IDT.load();
}

// The breakpoint exception will be used to test exception handling.
// Its only purpose is to temporarily pause a program when the breakpoint instruction int3 is executed.
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

#[test_case]
fn test_breakpoint_exception() {
    // invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}
