mod bitmap;
mod heap;
#[cfg(target_arch = "x86_64")] mod mapping;
mod paging;
mod phys;

#[cfg(target_arch = "x86_64")]
pub use bitmap::{frame_allocator, with_frame_allocator};

#[cfg(target_arch = "x86_64")]
pub use mapping::{alloc_pages, free_pages, create_mapper};

#[cfg(target_arch = "x86_64")]
pub use paging::{active_page_table, create_page_table};

pub use phys::{phys_addr, PhysBuf};

use crate::sys::boot::MemoryMap;
use crate::sys::pic;
use crate::sys::x86::addr::{PhysAddr, VirtAddr};

use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Once;

#[cfg(target_arch = "x86_64")]
use x86_64::structures::paging::{OffsetPageTable, Translate};

#[allow(static_mut_refs)]
#[cfg(target_arch = "x86_64")]
static mut MAPPER: Once<OffsetPageTable<'static>> = Once::new();

static PHYS_MEM_OFFSET: Once<usize> = Once::new();
static MEMORY_SIZE: AtomicUsize = AtomicUsize::new(0);

pub fn init(memory_map: &MemoryMap, offset: u64) {
    // Keep the timer interrupt to have accurate boot time measurement but mask
    // the keyboard interrupt that would create a panic if a key is pressed
    // during memory allocation otherwise.
    pic::mask(pic::KBD_IRQ);

    let mut memory_size = 0;
    let mut last_end_addr = 0;
    for region in memory_map.iter() {
        let start_addr = region.addr;
        let end_addr = region.addr + region.size;
        let hole = start_addr - last_end_addr;
        if hole > 0 && start_addr < (1 << 20) {
            memory_size += hole; // Count BIOS memory
        }
        log!(
            "MEM [{:#016X}-{:#016X}] {:?}", // "({} KB)"
            start_addr, end_addr - 1, region.kind //, size >> 10
        );
        if region.is_addressable() {
            // On i686 the maximum amount of memory addressable is around 3 GB
            // because some of it will be mapped above the 4 GB limit.
            memory_size += region.size;
        }
        last_end_addr = end_addr;
    }

    // FIXME: There are two small reserved areas at the end of the physical
    // memory that should be removed from the count to be fully accurate but
    // their sizes and location vary depending on the amount of RAM on the
    // system. It doesn't affect the count in megabytes.
    log!("RAM {} MB", memory_size >> 20);

    // TODO: Only count usable memory and use SMBIOS to report the RAM
    MEMORY_SIZE.store(memory_size as usize, Ordering::Relaxed);

    PHYS_MEM_OFFSET.call_once(|| offset as usize);

    // TODO: Pick a space in the lowest usable region for DMA

    #[cfg(target_arch = "x86")]
    {
        let mut memory_map = memory_map.clone();

        // Reserve the second half of the largest usable region for the heap
        let (heap_addr, heap_size) = {
            let region = memory_map.iter_mut().
                filter(|region| region.is_usable()).
                max_by_key(|region| region.size).
                expect("not usable region");

            let size = region.size / 2;
            let addr = region.addr + size;

            region.size = size;

            (addr, size)
        };

        bitmap::init_frame_allocator(&memory_map);
        heap::init_alloc(heap_addr as *mut u8, heap_size as usize);
        paging::init();
    }

    #[cfg(target_arch = "x86_64")] // TODO: Remove
    {
        #[allow(static_mut_refs)]
        unsafe {
            MAPPER.call_once(|| OffsetPageTable::new(
                paging::active_page_table(),
                VirtAddr::new(offset as usize).into(),
            ))
        };

        bitmap::init_frame_allocator(memory_map);
        heap::init_heap().expect("heap initialization failed");
    }

    pic::unmask(pic::KBD_IRQ);
}

pub fn phys_mem_offset() -> usize {
    unsafe { *PHYS_MEM_OFFSET.get_unchecked() }
}

#[cfg(target_arch = "x86_64")] // TODO: Remove
pub fn mapper() -> &'static mut OffsetPageTable<'static> {
    #[allow(static_mut_refs)]
    unsafe { MAPPER.get_mut_unchecked() }
}

pub fn memory_size() -> usize {
    MEMORY_SIZE.load(Ordering::Relaxed)
}

/// Size of the region handed to the global allocator at boot.
///
/// This is memory the kernel *reserved*, not memory that is in use: at boot
/// the allocator has handed out almost none of it.
pub fn heap_capacity() -> usize {
    heap::heap_size()
}

/// Physical bytes the kernel has actually committed.
///
/// Every frame the frame allocator has handed out is committed, because those
/// frames are mapped and can no longer be given to anyone else. The heap's
/// frames are counted in there, but the allocator only owns part of the space
/// they cover, so its unused remainder is subtracted back out.
fn committed_bytes(used_frames: usize, frame_size: usize, heap_free: usize) -> usize {
    used_frames
        .saturating_mul(frame_size)
        .saturating_sub(heap_free)
}

/// Memory in use: committed frames, less the unused part of the heap.
///
/// Reporting the *whole* heap reservation as used makes the figure a constant
/// fraction of RAM — half, since the heap is sized as half of memory — no
/// matter how much RAM the machine actually has.
pub fn memory_used() -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        let used_frames = with_frame_allocator(|a| a.used_frames());
        committed_bytes(used_frames, crate::sys::x86::page::PAGE_SIZE, heap::heap_free())
    }

    // No bitmap frame allocator on this target: the only thing we can account
    // for is what the global allocator has handed out.
    #[cfg(not(target_arch = "x86_64"))]
    {
        heap::heap_used()
    }
}

/// Memory not in use, i.e. the rest of RAM.
pub fn memory_free() -> usize {
    memory_size().saturating_sub(memory_used())
}

pub fn phys_to_virt(addr: PhysAddr) -> VirtAddr {
    VirtAddr::new(phys_mem_offset() + addr.as_usize())
}

#[cfg(target_arch = "x86")]
pub fn virt_to_phys(addr: VirtAddr) -> Option<PhysAddr> {
    Some(PhysAddr::new(addr.as_usize()))
}

#[cfg(target_arch = "x86_64")]
pub fn virt_to_phys(addr: VirtAddr) -> Option<PhysAddr> {
    mapper().translate_addr(addr.into()).map(|x| x.into())
}

#[test_case]
fn test_committed_bytes_counts_frames() {
    // Nothing consumed, nothing reserved for the heap.
    assert_eq!(committed_bytes(10, 4096, 0), 10 * 4096);
}

#[test_case]
fn test_committed_bytes_excludes_free_heap() {
    // All 10 committed frames belong to a heap the allocator has not touched:
    // reserved, but not in use.
    assert_eq!(committed_bytes(10, 4096, 10 * 4096), 0);
}

#[test_case]
fn test_committed_bytes_counts_partially_used_heap() {
    // 10 frames committed, 1 page of heap still free -> 9 pages in use.
    assert_eq!(committed_bytes(10, 4096, 4096), 9 * 4096);
}

#[test_case]
fn test_committed_bytes_never_underflows() {
    // Must not underflow if the heap ever claims more free than is committed.
    assert_eq!(committed_bytes(0, 4096, 999_999), 0);
}

#[test_case]
fn test_memory_used_is_not_the_heap_reservation() {
    let used = memory_used();
    let free = memory_free();
    let total = memory_size();
    let heap = heap_capacity();

    printk!(
        "MEM total={} B used={} B free={} B heap_reserved={} B\n",
        total, used, free, heap
    );

    // The kernel reserves half of RAM for the heap at boot but the allocator
    // has handed out almost none of it, so in-use must stay well under the
    // reservation instead of tracking it one-for-one.
    assert!(used < heap / 2, "in use is not below the heap reservation");

    // The two figures must still account for all of RAM, so the status bar
    // percentage stays coherent.
    assert!(used + free == total, "in use plus free does not equal total");
}
