use core::mem::size_of;

use crate::hw::param::PAGE_SIZE;
use super::{palloc::Page, palloc, pfree, VmError};

const MAX_CHUNK_SIZE: usize = 4080; // PAGE_SIZE - ZONE_HEADER_SIZE - HEADER_SIZE = 4096 - 8 - 8 = 4080.
const HEADER_SIZE: usize = size_of::<Header>();
const ZONE_SIZE: usize = 8;
const HEADER_USED: usize = 1 << 12; // Chunk is in use flag.

// 8 byte minimum allocation size,
// 4096-8-8=4080 byte maximum allocation size.
// Guarantee that address of header + header_size = start of data.
// Size must be <= 4080 Bytes.
// Bits 0-11 are size (2^0 - (2^12 - 1))
// Bit 12 is Used.
//
// Header.fields:
// ┌────────────────────────────────────┬─┬──────────────┐
// │    Unused / Reserved               │U│ Chunk Size   │
// └────────────────────────────────────┴─┴──────────────┘
// 63                                   12 11            0
//
#[repr(C)]
struct Header {
    fields: usize, // Could be a union?
}

// An allocation zone is the internal representation of a page.
// Each zone contains the address of the next zone (page aligned),
// plus the number of in use chunks within the zone (refs count).
//
// Zone.next:
// ┌──────────────────────────────────────┬──────────────┐
// │  next zone address (page aligned)    │ refs count   │
// └──────────────────────────────────────┴──────────────┘
// 63                                     11             0
//
#[repr(C)]
#[derive(Copy, Clone)]
struct Zone {
    base: *mut usize,   // This zone's address.
    next: usize,        // Next zone's address + this zone's ref count.
}

pub struct Kalloc {
    head: *mut usize, // Address of first zone.
    end: *mut usize,
}

#[derive(Debug)]
pub enum KallocError {
    MaxRefs,
    MinRefs,
    NullZone,
    OOM,
}

impl From<*mut usize> for Header {
    fn from(src: *mut usize) -> Self {
        let fields = unsafe { src.read() };
        Header { fields }
    }
}

impl Header {
    fn new(size: usize) -> Self {
        assert!(size <= MAX_CHUNK_SIZE);
        Header { fields: size }
    }

    fn chunk_size(&self) -> usize {
        self.fields & 0xFFF
    }

    fn is_free(&self) -> bool {
        self.fields & HEADER_USED == 0
    }

    fn set_used(&mut self) {
        self.fields = self.fields | HEADER_USED;
    }

    fn set_unused(&mut self) {
        self.fields = self.fields & !HEADER_USED;
    }

    // Clear size bits. Set size bits to size.
    fn set_size(&mut self, size: usize) {
        self.fields = (self.fields & !(0x1000 - 1)) | size;
    }

    // Unsafe write header data to memory at dest.
    fn write_to(&self, dest: *mut usize) {
        unsafe {
            dest.write(self.fields);
        }
    }

    // Takes an existing chunk and splits it into a chunk of 'new_size' + the remainder.
    fn split(&mut self, new_size: usize, cur_addr: *mut usize) -> (Header, *mut usize) {
        let old_size = self.chunk_size();
        let next_size = old_size - new_size;
        self.set_size(new_size);
        self.write_to(cur_addr);
        let next_addr = cur_addr.map_addr(|addr| addr + HEADER_SIZE + new_size);
        let next_header = Header { fields: next_size - HEADER_SIZE }; // make space for inserted header
        next_header.write_to(next_addr);
        (next_header, next_addr)
    }

    fn merge(&mut self, next: Self, next_addr: *mut usize) {
        assert!(next.is_free());
        assert!(self.is_free());
        let size = self.chunk_size() + HEADER_SIZE + next.chunk_size();
        self.set_size(size);
        //self.write_to(addr);
        unsafe { next_addr.write(0); }
    }
}

// Assumes the first usize of a zone is the zone header.
// Next usize is the chunk header.
impl From<*mut usize> for Zone {
    fn from(src: *mut usize) -> Self {
        Zone {
            base: src,
            next: unsafe { src.read() }
        }
    }
}

impl Zone {
    fn new(base: *mut usize) -> Self {
        Zone {
            base,
            next: 0x0,
        }
    }

    fn get_refs(&self) -> usize {
        self.next & (4095)
    }

    fn get_next(&self) -> Result<usize, KallocError> {
        let next_addr = self.next & !(PAGE_SIZE - 1);
        if next_addr == 0x0 {
            Err(KallocError::NullZone)
        } else {
            Ok(next_addr)
        }
    }

    // Read the next field to get the next zone address.
    // Discard this zone's refs count.
    // Write base address with next zone address and new refs count.
    #[inline(always)]
    unsafe fn write_refs(&mut self, new_count: usize) {
        let next_addr = match self.get_next() {
            Err(_) => 0x0,
            Ok(ptr) => ptr,
        };

        self.base.write(next_addr | new_count);
    }

    // Read the current next field to get the refs count.
    // Discard this zone's next addr.
    // Write base address with new next zone address and refs count.
    unsafe fn write_next(&mut self, new_next: *mut usize) {
        let refs = self.get_refs();
        self.base.write(new_next.addr() | refs);
    }

    fn increment_refs(&mut self) -> Result<(), KallocError> {
        let new_count = self.get_refs() + 1;
        if new_count > 510 {
            Err(KallocError::MaxRefs)
        } else {
            unsafe { self.write_refs(new_count); }
            Ok(())
        }
    }

    fn decrement_refs(&mut self) -> Result<usize, KallocError> {
        // Given a usize can't be < 0, I want to catch that and not cause a panic.
        // This may truly be unnecessary, but just want to be cautious.
        let new_count = self.get_refs() - 1;
        if (new_count as isize) < 0 {
            Err(KallocError::MinRefs)
        } else {
            unsafe { self.write_refs(new_count); }
            Ok(new_count)
        }
    }

    fn next_zone(&self) -> Result<Zone, KallocError> {
        if let Ok(addr) = self.get_next() {
            Ok(Zone::from(addr as *mut usize))
        } else {
            Err(KallocError::NullZone)
        }
    }

    // Only call from Kalloc.shrink_pool() to ensure this is not the first
    // zone in the pool.
    fn free_self(&mut self, mut prev_zone: Zone) {
        assert!(self.get_refs() == 0);
        // todo!("Relies on sequential page allocation.");
        // let prev_base = unsafe { self.base.byte_sub(0x1000) };
        // let mut prev_zone = Zone::from(prev_base);
        // // ^ BUG: not guaranteed sequential
        if let Ok(next_zone) = self.next_zone() {
            unsafe { prev_zone.write_next(next_zone.base); }
        } else {
            unsafe { prev_zone.write_next(0x0 as *mut usize); }
        }
        let _ = pfree(Page::from(self.base));
    }

    // Scan this zone for the first free chunk of size >= requested size.
    // First 8 bytes of a zone is the Zone.next field.
    // Second 8 bytes is the first header of the zone.
    fn scan(&mut self, size: usize) -> Option<*mut usize> {
        // Round to a 8 byte granularity
        let size = if size % 8 != 0 {
            (size + 7) & !7
        } else {
            size
        };

        // Start and end (start + PAGE_SIZE) bounds of zone.
        let (mut curr, end) = unsafe { (self.base.add(1), self.base.add(PAGE_SIZE/8)) };
        // Get the first header in the zone.
        let mut head = Header::from(curr);

        while curr < end {
            let chunk_size = head.chunk_size();
            if chunk_size < size || !head.is_free() {
                let (mut prev, trail) = (head, curr);
                curr = curr.map_addr(|addr| addr + HEADER_SIZE + chunk_size);
                head = Header::from(curr);

                // TODO: Is not pretty, make pretty.
                if prev.is_free() && head.is_free() {
                    prev.merge(head, curr);
                    prev.write_to(trail);
                    (head, curr) = (prev, trail);
                }
            } else {
                alloc_chunk(size, curr, self, &mut head);
                return Some(curr.map_addr(|addr| addr + HEADER_SIZE))
            }
        }
        None
    }
}

fn alloc_chunk(size: usize, ptr: *mut usize, zone: &mut Zone, head: &mut Header) {
    zone.increment_refs().expect("Maximum zone allocation limit exceeded.");
    head.set_used();
    head.write_to(ptr);

    if size != head.chunk_size() {
        let (_, _) = head.split(size, ptr);
        //next.write_to(next_addr);
    }
}

unsafe fn write_zone_header_pair(zone: &Zone, header: &Header) {
    let base = zone.base;
    base.write(zone.next);
    base.add(1).write(header.fields);
}

impl Kalloc {
    pub fn new(start: Page) -> Self {
        // Make sure start of allocation pool is page aligned.
        assert_eq!(start.addr.addr() & (PAGE_SIZE - 1), 0);
        // New page is the first zone in the Kalloc pool.
        let zone = Zone::new(start.addr);
        let head = Header::new(MAX_CHUNK_SIZE);
        unsafe { write_zone_header_pair(&zone, &head); }
        Kalloc {
            head: start.addr,
            end: start.addr.map_addr(|addr| addr + 0x1000),
        }
    }

    fn grow_pool(&self, tail: &mut Zone) -> Result<(Zone, Header), VmError> {
        let page = palloc()?;
        unsafe { tail.write_next(page.addr); }
        let zone = Zone::new(page.addr);
        let head = Header::new(MAX_CHUNK_SIZE);
        unsafe { write_zone_header_pair(&zone, &head); }
        Ok((zone, head))
    }

    fn shrink_pool(&self, mut to_free: Zone) {
        if to_free.base != self.head {
            let mut curr = Zone::new(self.head);

            while let Ok(next) = curr.next_zone() {
                if to_free.base == next.base {
                    // found it
                    to_free.free_self(curr);
                    return;
                } else {
                    curr = next;
                }
            }
            panic!("Tried to free a zone that wasn't in the list...")
        }
    }

    /// Finds the first fit for the requested size.
    /// 1. Scan first zone from first to last for a free chunk that fits.
    /// 2a. If success: Return chunk's starting address (*mut usize).
    /// 2b. Else, move to next zone and go back to step 1.
    /// 3. If no zone had a fit, then try to allocate a new zone (palloc()).
    /// 4. If success, go to step 2a. Else, fail with OOM.
    pub fn alloc(&mut self, size: usize) -> Result<*mut usize, KallocError> {
        let curr = self.head;
        let end = self.end.map_addr(|addr| addr - 0x1000);
        let mut zone = Zone::from(curr);
        let mut trail = zone;

        while zone.base <= end {
            if let Some(ptr) = zone.scan(size) {
                return Ok(ptr)
            } else {
                zone = zone.next_zone()?;
            }
        }

        match self.grow_pool(&mut trail) {
             Ok((mut zone, mut head)) => {
                 let ptr = zone.base.map_addr(|addr| addr + ZONE_SIZE + HEADER_SIZE);
                 alloc_chunk(size, ptr, &mut zone, &mut head);
                 Ok(ptr)
             },
             Err(_) => Err(KallocError::OOM),
        }
    }

    // TODO if you call alloc in order and then free in order this
    // doesn't merge, as you can't merge backwards. Consider a merging
    // pass when allocting.
    pub fn free<T>(&mut self, ptr: *mut T) {
        let ptr: *mut usize = ptr.cast();
        // Assume that round down to nearest page is the current zone base addr.
        let mut zone = Zone::from(ptr.map_addr(|addr| addr & !(PAGE_SIZE - 1)));
        let head_ptr = ptr.map_addr(|addr| addr - HEADER_SIZE);
        let mut head = Header::from(head_ptr);
        assert!(!head.is_free(), "Kalloc double free.");
        head.set_unused();

        if let Ok(count) = zone.decrement_refs() {
            if count == 0 {
                // this is costly, as it's a list traversal
                self.shrink_pool(zone);
            }
        } else {
            panic!("Negative zone refs count: {}", zone.get_refs())
        }

        let next_ptr = ptr.map_addr(|addr| addr + head.chunk_size());
        let next = Header::from(next_ptr);
        if next.is_free() {
            // back to back free, merge
            //head.set_size(head.chunk_size() + HEADER_SIZE + next.chunk_size())
            head.merge(next, next_ptr);
        }
        head.write_to(head_ptr);
    }
}
