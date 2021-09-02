use super::align_up;
use super::Locked;
use alloc::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::ptr;

struct ListNode {
    size: usize,
    // Optional pointer to the next node
    // The &'static mut type semantically describes an owned object behind a pointer.
    // Basically, it's a Box without a destructor that frees the object at the end of the scope.
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    // This is a const function, which will be required later when constructing a static linked list allocator.
    const fn new(size: usize) -> Self {
        ListNode { size, next: None }
    }

    //  calculate the start address of the represented region
    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    //  calculate the end address of the represented region
    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

pub struct LinkedListAllocator {
    head: ListNode,
}

impl LinkedListAllocator {
    // Like for the bump allocator, the new function doesn't initialize the allocator with the heap bounds.
    // In addition to maintaining API compatibility, the reason is that the initialization routine
    // requires to write a node to the heap memory, which can only happen at runtime.
    // The new function, however, needs to be a const function that can be evaluated at compile time,
    // because it will be used for initializing the ALLOCATOR static. For this reason,
    // we again provide a separate, non-constant init method.
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    // Initialize the allocator with the given heap bounds.
    // This function is unsafe because the caller must guarantee that the given
    // heap bounds are valid and that the heap is unused. This method must be
    // called only once.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.add_free_region(heap_start, heap_size);
    }

    // add the given memory region to the front of the list
    // The add_free_region method provides the fundamental push operation on the linked list.
    // We currently only call this method from init, but it will also be the central method
    // in our dealloc implementation. Remember, the dealloc method is called when an allocated
    // memory region is freed again. To keep track of this freed memory region, we want to push
    // it to the linked list.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // The method takes a memory region represented by an address and size as argument and
        // adds it to the front of the list. First, it ensures that the given region has the necessary size
        // and alignment for storing a ListNode. Then it creates the node and inserts it to the list

        // ensure that the freed region is capable of holding ListNode
        assert_eq!(align_up(addr, mem::align_of::<ListNode>()), addr);
        assert!(size >= mem::size_of::<ListNode>());

        // create a new list node and append it at the start of the list
        let mut node = ListNode::new(size);
        node.next = self.head.next.take();
        let node_ptr = addr as *mut ListNode;
        node_ptr.write(node);
        self.head.next = Some(&mut *node_ptr)
    }

    // Looks for a free region with the given size and alignment and removes it from the list.
    // Returns a tuple of the list node and the start address of the allocation.
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        // The method uses a current variable and a while let loop to iterate over the list elements.
        // At the beginning, current is set to the (dummy) head node. On each iteration,
        // it is then updated to the next field of the current node (in the else block).
        //  If the region is suitable for an allocation with the given size and alignment,
        // the region is removed from the list and returned together with the alloc_start address.
        // When the current.next pointer becomes None, the loop exits. This means that we iterated over
        // the whole list but found no region that is suitable for an allocation. In that case, we return None

        // reference to the current list node, updated for each iteration
        let mut current = &mut self.head;
        // look for a large enough memory region in the linked list
        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(&region, size, align) {
                // region suitable for allocation -> remove node from list
                let next = region.next.take();
                let ret = Some((current.next.take().unwrap(), alloc_start));
                current.next = next;
                return ret;
            } else {
                // region not suitable -> continue with next region
                current = current.next.as_mut().unwrap();
            }
        }

        // no suitable region found
        None
    }

    // Try to use the given region for an allocation with the given size and alignment.
    // returns an allocation start address on success.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        // First, the function calculates the start and end address of a potential allocation,
        // using the align_up function we defined earlier and the checked_add method. If an overflow occurs
        // or if the end address is behind the end address of the region,
        // the allocation doesn't fit in the region and we return an error.
        // The function performs a less obvious check after that. This check is necessary because most of the
        // time an allocation does not fit a suitable region perfectly, so that a part of the region remains
        // usable after the allocation. This part of the region must store its own ListNode after the allocation,
        // so it must be large enough to do so. The check verifies exactly that: either the allocation
        // fits perfectly (excess_size == 0) or the excess size is large enough to store a ListNode.

        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            // region to small
            return Err(());
        }

        // region suitable for allocation
        Ok(alloc_start)
    }

    // Adjust the given layout so tha the resulting allocated memory region is also capable of storing a ListNode.
    // Returns the adjusted size and alignment as a (size, align) tuple
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<ListNode>());

        (size, layout.align())
    }
}

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // start with layout adjustments and call the Mutex::lock function to receive a
        // mutable allocator reference. Then it use the find_region method to find a
        // suitable memory region for the allocation and remove it from the list.
        // If this doesn't succeed and None is returned, it returns null_mut to signal
        // an error as there is no suitable memory region.
        // In the success case, the find_region method returns a tuple of the suitable region
        // (no longer in the list) and the start address of the allocation.
        // Using alloc_start, the allocation size, and the end address of the region,
        // it calculates the end address of the allocation and the excess size again.
        // If the excess size is not null, it calls add_free_region to add the excess size of
        // the memory region back to the free list. Finally, it returns the alloc_start
        // address casted as a *mut u8 pointer.

        // perform layout adjustments to ensure that each allocated block is capable of storing a ListNode
        let (size, align) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.lock();

        if let Some((region, alloc_start)) = allocator.find_region(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 {
                allocator.add_free_region(alloc_end, excess_size);
            }
            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Perform some layout adjustments, and retrieves a &mut LinkedListAllocator reference
        // by calling the Mutex::lock function on the Locked wrapper.
        // Then it calls the add_free_region function to add the deallocated region to the free list.

        // perform layout adjustments to ensure that each allocated block is capable of storing a ListNode
        let (size, _) = LinkedListAllocator::size_align(layout);

        self.lock().add_free_region(ptr as usize, size)

        // The main problem of our implementation is that it only splits the heap into smaller blocks, but never merges them back together.
        // Instead of inserting freed memory blocks at the beginning of the linked list on deallocate,
        // it should always keep the list sorted by start address. This way, merging can be performed directly on
        // the deallocate call by examining the addresses and sizes of the two neighbour blocks in the list.
        // Of course, the deallocation operation is slower this way, but it prevents the heap fragmentation
    }
}
