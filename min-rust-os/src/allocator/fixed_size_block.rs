use super::Locked;
use alloc::alloc::{GlobalAlloc, Layout};
use core::{mem, ptr, ptr::NonNull};

struct ListNode {
    next: Option<&'static mut ListNode>,
}

// The block sizes to use.
// The sizes must each be power of 2 because they are also used as
// the block alignment (alignments must be always powers of 2).
// As block sizes, we use powers of 2 starting from 8 up to 2048.
// We don't define any block sizes smaller than 8 because each block
// must be capable of storing a 64-bit pointer to the next block when freed.
// For allocations greater than 2048 bytes we will fall back to a linked list allocator.
const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub struct FixedSizeBlockAllocator {
    // The list_heads field is an array of head pointers, one for each block size.
    // This is implemented by using the len() of the BLOCK_SIZES slice as the array length.
    // As a fallback allocator for allocations larger than the largest block size
    // we use the allocator provided by the linked_list_allocator
    list_heads: [Option<&'static mut ListNode>; BLOCK_SIZES.len()],
    fallback_allocator: linked_list_allocator::Heap,
}

impl FixedSizeBlockAllocator {
    // Creates an empty FixedSizeBlockAllocator
    pub const fn new() -> Self {
        // The EMPTY constant is needed because to tell the Rust compiler that we
        // want to initialize the array with a constant value. Initializing the array
        // directly as [None; BLOCK_SIZES.len()] does not work because then the compiler
        // requires that Option<&'static mut ListNode> implements the Copy trait,
        // which it does not. This is a current limitation of the Rust compiler,
        // which might go away in the future.
        const EMPTY: Option<&'static mut ListNode> = None;
        FixedSizeBlockAllocator {
            list_heads: [EMPTY; BLOCK_SIZES.len()],
            fallback_allocator: linked_list_allocator::Heap::empty(),
        }
    }

    // Initialise the allocator with the given heap bounds
    // This function is unsafe because the caller must guarantee that the
    // given heap bounds are valid and that the heap is unused. This method
    // must be called only once
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.fallback_allocator.init(heap_start, heap_size);
    }

    // Allocates using the fallback allocator
    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        match self.fallback_allocator.allocate_first_fit(layout) {
            // The NonNull type is an abstraction for a raw pointer that is guaranteed
            // to be not the null pointer. By mapping the Ok case to the NonNull::as_ptr
            // method and the Err case to a null pointer, we can easily translate this
            // back to a *mut u8 type.
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => ptr::null_mut(),
        }
    }
}

// Choose an appropriate block size for the given allocator
// Returns an index into the `BLOCK_SIZES` array
fn list_index(layout: &Layout) -> Option<usize> {
    // The block must have at least the size and alignment required by the given Layout.
    // Since we defined that the block size is also its alignment,
    // this means that the required_block_size is the maximum of the layout's
    // size() and align() attributes. To find the next-larger block in the BLOCK_SIZES slice,
    // we first use the iter() method to get an iterator and then the position() method
    // to find the index of the first block that is as least as large as the required_block_size.
    let required_block_size = layout.size().max(layout.align());
    // Note that we don't return the block size itself, but the index into the BLOCK_SIZES slice.
    // The reason is that we want to use the returned index as an index into the list_heads array.
    BLOCK_SIZES.iter().position(|&s| s >= required_block_size)
}

unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // First, we use the Locked::lock method to get a mutable reference to the wrapped allocator instance.
        let mut allocator = self.lock();

        // Next, we call the list_index function we just defined to calculate the appropriate
        // block size for the given layout and get the corresponding index into the list_heads array.
        match list_index(&layout) {
            // If the list index is Some, we try to remove the first node in the corresponding list
            // started by list_heads[index] using the Option::take method.
            Some(index) => {
                match allocator.list_heads[index].take() {
                    // If the list is not empty, we enter the Some(node) branch of the match statement,
                    // where we point the head pointer of the list to the successor of the popped node
                    // (by using take again).
                    Some(node) => {
                        allocator.list_heads[index] = node.next.take();
                        // Finally, we return the popped node pointer as a *mut u8.
                        node as *mut ListNode as *mut u8
                    }
                    // no block exists in list => allocate new block
                    None => {
                        // first get the current block size from the BLOCK_SIZES slice
                        // and use it as both the size and the alignment for the new block.
                        let block_size = BLOCK_SIZES[index];
                        // only works if all block sizes are a power of two
                        let block_align = block_size;
                        // create a new Layout from it and call the fallback_alloc method to perform the allocation.
                        // The reason for adjusting the layout and alignment is that the block will be added
                        // to the block list on deallocation.
                        let layout = Layout::from_size_align(block_size, block_align).unwrap();
                        allocator.fallback_alloc(layout)
                    }
                }
            }
            // If this index is None, no block size fits for the allocation,
            // therefore we use the fallback_allocator using the fallback_alloc function.
            None => allocator.fallback_alloc(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        match list_index(&layout) {
            // If list_index returns a block index, we need to add the freed memory block to the list.
            Some(index) => {
                // create a new ListNode that points to the current list head (by using Option::take again).
                let new_node = ListNode {
                    next: allocator.list_heads[index].take(),
                };
                // verify the block has the size and alignment required for storing node
                assert!(mem::size_of::<ListNode>() <= BLOCK_SIZES[index]);
                assert!(mem::align_of::<ListNode>() <= BLOCK_SIZES[index]);
                // perform the write by converting the given *mut u8 pointer to a *mut ListNode pointer
                // and then calling the unsafe write method on it.
                let new_node_ptr = ptr as *mut ListNode;
                new_node_ptr.write(new_node);
                // The last step is to set the head pointer of the list, which is currently None since
                // we called take on it, to our newly written ListNode. For that we convert
                // the raw new_node_ptr to a mutable reference.
                allocator.list_heads[index] = Some(&mut *new_node_ptr);
            }
            // If the index is None, no fitting block size exists in BLOCK_SIZES, which indicates
            // that the allocation was created by the fallback allocator.
            // Therefore we use its deallocate to free the memory again.
            None => {
                let ptr = NonNull::new(ptr).unwrap();
                allocator.fallback_allocator.deallocate(ptr, layout);
            }
        }
    }
}
