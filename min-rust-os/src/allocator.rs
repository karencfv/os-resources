use alloc::alloc::{GlobalAlloc, Layout};
// use bump::BumpAllocator;
use core::ptr::null_mut;
use linked_list::LinkedListAllocator;
// use linked_list_allocator::LockedHeap;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

// pub mod bump;
pub mod linked_list;

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
// Using bump allocator
// static ALLOCATOR: Locked<BumpAllocator> = Locked::new(BumpAllocator::new()); // using linked_list_allocator external crate LockedHeap = LockedHeap::empty(); // Dummy = Dummy;
static ALLOCATOR: Locked<LinkedListAllocator> = Locked::new(LinkedListAllocator::new());

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

// A wrapper around a spin::Mutex to permit trait implementations
pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

// The type is a generic wrapper around a spin::Mutex<A>. It imposes no restrictions on the wrapped type A,
// so it can be used to wrap all kinds of types, not just allocators. It provides a simple new constructor
// function that wraps a given value. For convenience, it also provides a lock function that calls lock on
// the wrapped Mutex. Since the Locked type is general enough to be useful for other allocator implementations too,
// we put it in the parent allocator module.
impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

// Align the given address `addr` upwards to alignment `align`.
// fn align_up(addr: usize, align: usize) -> usize {
//     let remainder = addr % align;
//     if remainder == 0 {
//         addr // addr already aligned
//     } else {
//         addr - remainder + align
//     }
// }

// Requires that `align` is a power of two.
// This method utilizes that the GlobalAlloc trait guarantees that align is always a power of two.
// This makes it possible to create a bitmask to align the address in a very efficient way.
fn align_up(addr: usize, align: usize) -> usize {
    // Since align is a power of two, its binary representation has only a single bit set (e.g. 0b000100000).
    // This means that align - 1 has all the lower bits set (e.g. 0b00011111).
    // By creating the bitwise NOT through the ! operator, we get a number that has all the bits set
    // except for the bits lower than align (e.g. 0b…111111111100000).
    // By performing a bitwise AND on an address and `!(align - 1)`, we align the address downwards.
    // This works by clearing all the bits that are lower than align.
    // Since we want to align upwards instead of downwards, we increase the addr by align - 1 before performing the bitwise AND.
    // This way, already aligned addresses remain the same while non-aligned addresses are rounded to the next alignment boundary.
    (addr + align - 1) & !(align - 1)
}
