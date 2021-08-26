// use x86_64::structures::paging::{Mapper, Page, PageTable, RecursivePageTable};
// use x86_64::{PhysAddr, VirtAddr};

// Recursive Paging is an interesting technique that shows how powerful a single mapping in a page table can be.

// Disadvantages of recursive paging:
// - It occupies a large amount of virtual memory (512GiB). This isn't a big problem in the large 48-bit address space,
//   but it might lead to suboptimal cache behaviour.
// - It only allows accessing the currently active address space easily. Accessing other address spaces is still possible
//   by changing the recursive entry, but a temporary mapping is required for switching back.
// - It heavily relies on the page table format of x86 and might not work on other architectures.

// The code below shows how to translate a virtual address to its mapped physical address:
// Create a RecursivePageTable instance from the level 4 address.
// let level_4_table_addr = [...];
// let level_4_table_ptr = level_4_table_addr as *mut PageTable;
// let recursive_page_table = unsafe {
//     let level_4_table = &mut *level_4_table_ptr;
//     RecursivePageTable::new(level_4_table).unwrap();
// }
// // Retrieve physical address for the given virtual address
// let addr: u64 = [...]
// let addr = VirtAddr::new(addr);
// let page: Page::containing_address(addr);
// // Perform the translation
// let frame = recursive_page_table.translate_page(page);
// frame.map(|frame| frame.start_address() + u64::from(addr.page_offset()))

use x86_64::{registers::control::Cr3, structures::paging::PageTable, VirtAddr};

// Returns a mutable reference to the active level 4 table.

// This function is unsafe because the caller must guarantee that the
// complete physical memory is mapped to virtual memory at the passed
// `physical_memory_offset`. Also, this function must be only called once
// to avoid aliasing `&mut` references (which is undefined behaviour).

pub unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    // First, we read the physical frame of the active level 4 table from the CR3 register
    let (level_4_page_table_frame, _) = Cr3::read();

    // then take its physical start address, convert it to an u64, and add it to
    // physical_memory_offset to get the virtual address where the page table frame is mapped.
    let phys = level_4_page_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();

    // Finally, we convert the virtual address to a *mut PageTable raw pointer
    // through the as_mut_ptr method and then unsafely create a &mut PageTable reference from it.
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}
