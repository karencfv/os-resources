use x86_64::structures::paging::{Mapper, Page, PageTable, RecursivePageTable};
use x86_64::{PhysAddr, VirtAddr};

// Recursive Paging is an interesting technique that shows how powerful a single mapping in a page table can be.
// The code below shows how to translate a virtual address to its mapped physical address:
// Create a RecursivePageTable instance from the level 4 address.
let level_4_table_addr = [...];
let level_4_table_ptr = level_4_table_addr as *mut PageTable;
let recursive_page_table = unsafe {
    let level_4_table = &mut *level_4_table_ptr;
    RecursivePageTable::new(level_4_table).unwrap();
}
// Retrieve physical address for the given virtual address
let addr: u64 = [...]
let addr = VirtAddr::new(addr);
let page: Page::containing_address(addr);
// Perform the translation
let frame = recursive_page_table.translate_page(page);
frame.map(|frame| frame.start_address() + u64::from(addr.page_offset()))

// Disadvantages of recursive paging:
// - It occupies a large amount of virtual memory (512GiB). This isn't a big problem in the large 48-bit address space, 
//   but it might lead to suboptimal cache behaviour.
// - It only allows accessing the currently active address space easily. Accessing other address spaces is still possible
//   by changing the recursive entry, but a temporary mapping is required for switching back.
// - It heavily relies on the page table format of x86 and might not work on other architectures.
