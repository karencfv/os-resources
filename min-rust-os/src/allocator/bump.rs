use super::{align_up, Locked};
use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr;

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
    allocations: usize,
}

impl BumpAllocator {
    // Create a new empty bump allocator
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next: 0,
            // The allocations field is a simple counter for the active allocations with the goal
            // of resetting the allocator after the last allocation was freed. It is initialized with 0.
            allocations: 0,
        }
    }

    // Initializes the bump allocator with the given heap bounds.
    // This method is unsafe because the caller must ensure that the given
    // memory range is unused. Also, this method must be called only once.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start + heap_size;
        // The purpose of the next field is to always point to the first unused byte of the heap,
        // i.e. the start address of the next allocation. It is set to heap_start in the init function because
        // at the beginning the complete heap is unused. On each allocation, this field will be increased by
        // the allocation size ("bumped") to ensure that we don't return the same memory region twice.
        self.next = heap_start;
    }
}

unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // get mutable reference The instance remains locked until the end of the method,
        // so that no data race can occur in multithreaded contexts
        let mut bump = self.lock();

        // the alloc implementation now respects alignment requirements and performs a
        // bounds check to ensure that the allocations stay inside the heap memory region.
        let alloc_start = align_up(bump.next, layout.align());
        // To prevent integer overflow on large allocations, we use the checked_add method.
        // If an overflow occurs or if the resulting end address of the allocation is larger than
        // the end address of the heap, we return a null pointer to signal an out-of-memory situation.
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return ptr::null_mut(),
        };

        if alloc_end > bump.heap_end {
            ptr::null_mut() // out of memory
        } else {
            bump.next = alloc_end;
            bump.allocations += 1;
            alloc_start as *mut u8
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut bump = self.lock(); // get mutable reference

        bump.allocations -= 1;
        if bump.allocations == 0 {
            // If the counter reaches 0 again, it means that all allocations were freed again.
            // In this case, it resets the next address to the heap_start address to make the complete heap memory available again.
            bump.next = bump.heap_start;
        }
    }
}
