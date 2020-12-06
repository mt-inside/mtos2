use super::Locked;
use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr;
use super::align_up;
use core::mem;

/* TODO:
 * - avoid fragmentation by inserting free ListNodes into the list in order of start address; deallocate() can then merge with adjacent blocks.
 */

struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        ListNode{ size, next: None }
    }

    fn start_addr(&self) -> usize {
        /* Think: ListNode doesn't actually contain pointers, it's just written at the start of a free
         * block. What it knows is how far away the end is from the start.
         * Thus if you know the start addr, you can calculate the end addr.
         * Although this struct doesn't "know" the start addr of the block, if you have a pointer to this,
         * you have the start addr of the free range.
         * Basically, this takes &self
         */
        self as *const Self as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}


pub struct LinkedListAllocator {
    head: ListNode,
}

impl LinkedListAllocator {
    // will be hard to refactor to RAII cause new() needs to be const for compile-time eval, but
    // init() can only be called at runtime when we've got the address of a page.
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.add_free_region(heap_start, heap_size);
    }

    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // TODO dunno why these are asserts? Will alloc() be making sure only compliant regions are
        // handed out?
        assert_eq!(align_up(addr, mem::align_of::<ListNode>()), addr);
        assert!(size >= mem::size_of::<ListNode>());

        let mut node = ListNode::new(size);
        node.next = self.head.next.take();
        let node_ptr = addr as *mut ListNode;
        node_ptr.write(node);
        self.head.next = Some(&mut *node_ptr);
    }

    fn find_region(&mut self, size: usize, align: usize)
        -> Option<(&'static mut ListNode, usize)>
    {
        let mut current = &mut self.head;

        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(&region, size, align) {
                // region suitable for allocation
                let next = region.next.take();
                let ret = Some((current.next.take().unwrap(), alloc_start));
                current.next = next;
                return ret;
            } else {
                // region not suitable => iterate
                current = current.next.as_mut().unwrap();
            }
        }

        // No suitable regions
        None
    }

    // Dunno why this returns Result<T, ()> rather than Option<T>.
    // All I can think of is so ? can be used in one place.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize)
        -> Result<usize, ()>
    {
        // Do we leak any gap between region's start and alloc_start? I don't think so because of size_align()
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            // region too small
            return Err(());
        }

        let excess_size = region.end_addr() - alloc_end;
        if excess_size > 0 && excess_size < mem::size_of::<ListNode>() {
            // if the allocation doesn't exactly fit, then allocation is defined as splitting the
            // region into a used and a remaining free part. That free part has to house a
            // ListNode.
            // This region is too small to hold that ListNode in addition to its allocation
            return Err(());
        }

        // region suitbale
        Ok(alloc_start)
    }

    /* Adjust the requested layout such that it's capable of holding a `ListNode`, as one day it'll
     * be deallocated so we'll want it to do that.
     * That means
     * - moving the alignment up to what `ListNode` requires
     * - ensuring we allocate at least that much space (returning more than the user asked for is not an error)
     */
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>()) // bump alignment of this block up to `ListNode`'s requirements
            .expect("adjusting alignment failed")
            .pad_to_align(); // bump size up so that it ends `ListNode`-aligned as well, thus the next block will be able to store a ListNode too.
        let size = layout.size().max(mem::size_of::<ListNode>());
        (size, layout.align())
    }
}


unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.lock();

        if let Some((region, alloc_start)) = allocator.find_region(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 { // guarenteed by find_region to be big enough to hold the free list element.
                allocator.add_free_region(alloc_end, excess_size);
            }
            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = LinkedListAllocator::size_align(layout);

        self.lock().add_free_region(ptr as usize, size)
    }
}
