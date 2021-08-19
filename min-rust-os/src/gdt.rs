use lazy_static::lazy_static;
#[allow(deprecated)]
use x86_64::instructions::segmentation::set_cs;
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

// lazy_static is used because Rust's const evaluator does not yet do this initialization at compile time
lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            // We haven't implemented memory management yet, so we don't have a proper way to allocate a new stack.
            // Instead, we use a static mut array as stack storage for now. The unsafe is required because the compiler
            // can't guarantee race freedom when mutable statics are accessed.
            let stack_start = VirtAddr::from_ptr(unsafe { &STACK });
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (
            gdt,
            Selectors {
                code_selector,
                tss_selector,
            },
        )
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    GDT.0.load();
    // The reason for the unsafe block is that it might be possible to break memory safety by loading invalid selectors.
    unsafe {
        // use the selectors to reload the cs segment register and load the TSS
        #[allow(deprecated)]
        set_cs(GDT.1.code_selector);
        load_tss(GDT.1.tss_selector);
    }
}
