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

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::{
    registers::control::Cr3,
    structures::paging::page_table::FrameError,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};

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

// Translates the given virtual address to the mapped physical address, or `None` if the address is not mapped.
// This function is unsafe because the caller must guarantee that the complete physical memory
// is mapped to virtual memory at the passed `physical_memory_offset`.
pub unsafe fn translate_addr(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    translate_addr_inner(addr, physical_memory_offset)
}

fn translate_addr_inner(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
    // read active level 4 frame from the CR3 register
    let (level_4_page_table_frame, _) = Cr3::read();

    let table_indexes = [
        addr.p4_index(),
        addr.p3_index(),
        addr.p2_index(),
        addr.p1_index(),
    ];

    let mut frame = level_4_page_table_frame;

    // traverse multi level page table
    for &index in &table_indexes {
        // convert the frame into a page table reference
        let virt = physical_memory_offset + frame.start_address().as_u64();
        let table_ptr: *const PageTable = virt.as_ptr();
        let table = unsafe { &*table_ptr };

        // read the page table entry and update `frame`
        let entry = &table[index];
        frame = match entry.frame() {
            Ok(frame) => frame,
            Err(FrameError::FrameNotPresent) => return None,
            Err(FrameError::HugeFrame) => panic!("huge pages are not supported"),
        };
    }

    // calculate physical address by adding page offset
    Some(frame.start_address() + u64::from(addr.page_offset()))
}

// Initialize a new OffsetPageTable.
// This function is unsafe because the caller must guarantee that the
// complete physical memory is mapped to virtual memory at the passed
// `physical_memory_offset`. Also, this function must be only called once
// to avoid aliasing `&mut` references (which is undefined behaviour).
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    // The bootloader maps the complete physical memory at a virtual address specified by
    // the physical_memory_offset variable, so we can use the OffsetPageTable type.
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

// maps a given virtual page to 0xb8000, the physical frame of the VGA text buffer.
// We choose that frame because it allows us to easily test if the mapping was created correctly:
// We just need to write to the newly mapped page and see whether we see the write appear on the screen.
pub fn create_example_mapping(
    page: Page,
    mapper: &mut OffsetPageTable,
    // The trait is generic over the PageSize trait to work with both standard 4KiB pages and huge
    // 2MiB/1GiB pages. We only want to create a 4KiB mapping, so we set the generic parameter to Size4KiB
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let frame = PhysFrame::containing_address(PhysAddr::new(0xb8000));
    // set the PRESENT flag because it is required for all valid entries
    // and the WRITABLE flag to make the mapped page writeable.
    let flags = Flags::PRESENT | Flags::WRITABLE;

    let map_to_result = unsafe {
        // FIXME: this is not safe, because the caller must ensure that the frame is not already in use.
        // The reason for this is that mapping the same frame twice could result in undefined behaviour.
        // only using for testing
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}

// A dummy FrameAllocator that always returns `None`
pub struct EmptyFrameAllocator;

// Implementing the FrameAllocator is unsafe because the implementer must guarantee
// that the allocator yields only unused frames. Otherwise undefined behaviour might occur
unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        None
    }
}

// FrameAllocator that returns usable frames from the bootloader's memory map
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {
    // Create a FrameAllocator from the passed memory map.
    // The init function initializes a BootInfoFrameAllocator with a given memory map.
    // The next field is initialized with 0 and will be increased for every frame allocation
    // to avoid returning the same frame twice. Since we don't know if the usable frames
    // of the memory map were already used somewhere else, our init function must be unsafe
    // to require additional guarantees from the caller.
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    // Auxiliary method that converts the memory map into an iterator of usable frames:
    // The return type of the function uses the impl Trait feature.
    // This way, we can specify that we return some type that implements the Iterator trait with
    // item type PhysFrame, but don't need to name the concrete return type.
    // This is important here because we can't name the concrete type since it depends on unnamable closure types.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        // Then we use the filter method to skip any reserved or otherwise unavailable regions.
        // The bootloader updates the memory map for all the mappings it creates, so frames that
        // are used by our kernel (code, data or stack) or to store the boot information are
        // already marked as InUse or similar. Thus we can be sure that Usable frames are not used somewhere else.
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        // Next, we use flat_map to transform the address ranges into an iterator of frame start addresses,
        // choosing every 4096th address using step_by. Since 4096 bytes (= 4 KiB) is the page size,
        // we get the start address of each frame. The bootloader page aligns all usable memory areas
        // so that we don't need any alignment or rounding code here. By using flat_map instead of map,
        // we get an Iterator<Item = u64> instead of an Iterator<Item = Iterator<Item = u64>>
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    // This implementation is not quite optimal since it recreates the usable_frame allocator on every allocation.
    // It would be better to directly store the iterator as a struct field instead. Then we wouldn't need
    // the nth method and could just call next on every allocation. The problem with this approach is that
    // it's not possible to store an impl Trait type in a struct field currently.
    // It might work someday when named existential types are fully implemented.
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        // First use the usable_frames method to get an iterator of usable frames from the memory map.
        // Then, use the Iterator::nth function to get the frame with index self.next (thereby skipping (self.next - 1) frames)
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
