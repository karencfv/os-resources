use crate::gdt;
use crate::print;
use crate::println;
use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use pic8259::ChainedPics;
use spin;
use spin::Mutex;
use x86_64::instructions::port::Port;
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
        // The set_stack_index method is unsafe because the the caller must ensure that the used index is valid
        // and not already used for another exception.
        unsafe {
            // Now that we loaded a valid TSS and interrupt stack table, we can set the stack index for
            // our double fault handler in the IDT
            idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
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

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

// Our timer_interrupt_handler has the same signature as our exception handlers,
// because the CPU reacts identically to exceptions and external interrupts
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    print!(".");

    // PIC expects an explicit “end of interrupt” (EOI) signal from our interrupt handler.
    // This signal tells the controller that the interrupt was processed and that the system is ready to
    // receive the next interrupt. So the PIC thinks we're still busy processing the first timer interrupt
    // and waits patiently for the EOI signal before sending the next one.
    unsafe {
        PICS.lock()
            // The notify_end_of_interrupt figures out whether the primary or secondary PIC sent the interrupt
            // and then uses the command and data ports to send an EOI signal to respective controllers.
            // If the secondary PIC sent the interrupt both PICs need to be notified because the secondary PIC
            // is connected to an input line of the primary PIC.
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // read from the data port of the PS/2 controller, which is the I/O port with number 0x60
    //    let mut port = Port::new(0x60);
    //    let scancode: u8 = unsafe { port.read() };
    //
    //    // translate scancodes to keys. In this case only 1-3 will be scanned
    //    let key = match scancode {
    //        0x02 => Some('1'),
    //        0x03 => Some('2'),
    //        0x04 => Some('3'),
    //        _ => None,
    //    };
    //    if let Some(key) = key {
    //        print!("{}", key);
    //    }

    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(
            Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore)
        );
    }
    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(0x60);

    let scancode: u8 = unsafe { port.read() };
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => print!("{}", character),
                DecodedKey::RawKey(key) => print!("{:?}", key),
            }
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

// The default configuration of the PICs is not usable, because it sends interrupt vector numbers
// in the range 0–15 to the CPU. These numbers are already occupied by CPU exceptions.
// To fix this overlapping issue, we need to remap the PIC interrupts to different numbers.
// The actual range doesn't matter as long as it does not overlap with the exceptions,
// but typically the range 32–47 is chosen, because these are the first free numbers after the 32 exception slots.
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// unsafe because wrong offsets could cause undefined behaviour.
pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

// The enum is a C-like enum so that we can directly specify the index for each variant.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    // the keyboard uses line 1 of the primary PIC. This means that it arrives at the CPU as interrupt 33 (1 + offset 32).
    // We add this index as a new Keyboard variant to the InterruptIndex enum. We don't need to specify the value explicitly,
    // since it defaults to the previous value plus one.
    Keyboard,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

#[test_case]
fn test_breakpoint_exception() {
    // invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}
