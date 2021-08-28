use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use linked_list_allocator::LockedHeap;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

pub struct Dummy;

// dummy allocator does the absolute minimum to implement the GlobalAlloc trait
// and always return an error when alloc is called.
unsafe impl GlobalAlloc for Dummy {
    // always return the null pointer from alloc, which corresponds to an allocation error.
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // the allocator never returns any memory so a call to dealloc should never occur
        panic!("dealloc should never be called")
    }
}

// Temporarily tell the Rust compiler to register an instance of the Dummy allocator as the global heap allocator.
#[global_allocator]
// The struct is named LockedHeap because it uses the spinning_top::Spinlock type for synchronization.
// This is required because multiple threads could access the ALLOCATOR static at the same time.
// Create an allocator without any backing memory.
static ALLOCATOR: LockedHeap = LockedHeap::empty(); // Dummy = Dummy;

// Define a virtual memory region for the heap. Any virtual address range is fine as long as it's
// not already used for a different memory region. By using `0x_4444_4444_0000` it will be easy
// to identify a heap pointer.
pub const HEAP_START: usize = 0x_4444_4444_0000;
// 100kb for now, can be increased later
pub const HEAP_SIZE: usize = 100 * 1024;

// Map the virtual memory region to the physical memory
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        // To create a range of the pages that we want to map, we convert the HEAP_START pointer to a VirtAddr type.
        // Then we calculate the heap end address from it by adding the HEAP_SIZE.
        // We want an inclusive bound (the address of the last byte of the heap), so we subtract 1.
        // Next, we convert the addresses into Page types using the containing_address function.
        // Finally, we create a page range from the start and end pages using the Page::range_inclusive function.
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    // map all pages of the page range we just created
    for page in page_range {
        // allocate a physical frame that the page should be mapped to using the FrameAllocator::allocate_frame method.
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        // set the required PRESENT flag and the WRITABLE flag for the page.
        // With these flags both read and write accesses are allowed, which makes sense for heap memory.
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        // use the Mapper::map_to method for creating the mapping in the active page table.
        // The method can fail, therefore we use the question mark operator again to forward the error to the caller.
        // On success, the method returns a MapperFlush instance that we can use to update the
        // translation lookaside buffer using the flush method
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    // Initialize the allocator after creating the heap.
    // use the lock method on the inner spinlock of the LockedHeap type to get an exclusive reference
    // to the wrapped Heap instance, on which we then call the init method with the heap bounds as arguments.
    // It is important that we initialize the heap after mapping the heap pages,
    // since the init function already tries to write to the heap memory.
    unsafe { ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE) };

    Ok(())
}
