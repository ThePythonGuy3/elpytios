use core::{
    alloc::{Layout, LayoutError},
    fmt,
    hint::assert_unchecked,
    mem::MaybeUninit,
    ptr, slice,
};

use nonmax::NonMaxU32;

#[derive(Clone, Copy)]
pub enum TreeAllocError {
    InsufficientSpace { requested_order: u32 },
}

impl fmt::Debug for TreeAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for TreeAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientSpace { requested_order } => write!(f, "Tree can no longer contain allocation of size 2^{requested_order}"),
        }
    }
}

/// A binary buddy tree, implemented with a split bitset and free lists. The tree operates on number
/// of "order," not leaf counts; i.e., the leaf count must be `2 ^ order`.
#[repr(C)]
pub struct AllocTree {
    max_order: u32,
    // The offsets here are relative to the offset of `data`...
    nodes_offset: usize,
    split_bitset_offset: usize,
    data: AllocTreeData,
}

struct AllocTreeFields<'a> {
    max_order: u32,
    free_lists: FreeLists<'a>,
    list_nodes: &'a mut [ListNode],
    split_bitset: &'a mut AllocBitset,
}

impl AllocTree {
    pub unsafe fn new(at: *mut (), layout: AllocTreeLayout) -> *mut Self {
        let this = ptr::from_raw_parts_mut::<Self>(at, layout.size() - layout.free_nodes_offset_abs);
        unsafe {
            (&raw mut (*this).max_order).write(layout.max_order);
            (&raw mut (*this).nodes_offset).write(layout.nodes_offset_abs - layout.free_nodes_offset_abs);
            (&raw mut (*this).split_bitset_offset).write(layout.split_bitset_offset_abs - layout.free_nodes_offset_abs);

            let free_lists = (&raw mut (*this).data.0).as_mut_ptr().cast::<ListHead>();
            for i in 0..=layout.max_order as usize {
                free_lists.add(i).write(ListHead {
                    head: if i == layout.max_order as usize { Some(NonMaxU32::ZERO) } else { None },
                });
            }

            let list_nodes = (&raw mut (*this).data.0)
                .as_mut_ptr()
                .add(layout.nodes_offset_abs - layout.free_nodes_offset_abs)
                .cast::<ListNode>();
            for i in 0..1 << layout.max_order {
                list_nodes.add(i).write(ListNode { prev: None, next: None });
            }

            (&raw mut (*this).data.0)
                .as_mut_ptr()
                .add(layout.split_bitset_offset_abs - layout.free_nodes_offset_abs)
                .cast::<u32>()
                .write_bytes(0, AllocBitset::size_for((1 << layout.max_order) - 1));
        }

        this
    }

    #[inline]
    fn fields(&mut self) -> AllocTreeFields<'_> {
        let max_order = self.max_order;
        let data = self.data.0.as_mut_ptr();
        unsafe {
            AllocTreeFields {
                max_order,
                free_lists: FreeLists {
                    heads: slice::from_raw_parts_mut(data.cast(), max_order as usize + 1),
                },
                list_nodes: slice::from_raw_parts_mut(data.add(self.nodes_offset).cast(), 1 << max_order),
                split_bitset: ptr::from_raw_parts_mut::<AllocBitset>(data.add(self.split_bitset_offset), AllocBitset::size_for((1 << max_order) - 1))
                    .as_mut_unchecked(),
            }
        }
    }

    #[inline]
    pub fn node_count(&self) -> u32 {
        1 << self.max_order
    }

    pub fn alloc(&mut self, order: u32) -> Result<u32, TreeAllocError> {
        let AllocTreeFields {
            max_order,
            free_lists,
            list_nodes,
            split_bitset,
        } = self.fields();

        if order > max_order {
            return Err(TreeAllocError::InsufficientSpace { requested_order: order })
        }

        let mut current = None;
        for i in order..=max_order {
            if let Some(head) = free_lists.heads[i as usize].head.take() {
                if let Some(new_head) = list_nodes[head.get() as usize].next.take() {
                    list_nodes[new_head.get() as usize].prev = None;
                    free_lists.heads[i as usize].head = Some(new_head);
                }

                current = Some((head.get(), i));
                break
            }
        }

        let Some((index, mut current_order)) = current else {
            return Err(TreeAllocError::InsufficientSpace { requested_order: order })
        };

        if current_order < max_order {
            unsafe {
                split_bitset.get_and_toggle(Self::bit_index(index, current_order, max_order));
            }
        }

        while current_order > order {
            let next_order = current_order - 1;

            let free = NonMaxU32::new(index + (1 << next_order)).expect("Allocation index >= u32::MAX");
            list_nodes[free.get() as usize].prev = None;

            if let Some(prev_head) = free_lists.heads[next_order as usize].head.replace(free) {
                list_nodes[prev_head.get() as usize].prev = Some(free);
                list_nodes[free.get() as usize].next = Some(prev_head);
            } else {
                list_nodes[free.get() as usize].next = None;
            }

            current_order = next_order;
            unsafe {
                // False: Either both buddies are occupied or both are allocated
                // True:  Exactly one buddy is occupied
                split_bitset.get_and_toggle(Self::bit_index(index, current_order, max_order));
            }
        }

        Ok(index)
    }

    pub unsafe fn dealloc(&mut self, mut index: u32, mut order: u32) {
        let AllocTreeFields {
            max_order,
            free_lists,
            list_nodes,
            split_bitset,
        } = self.fields();

        unsafe {
            assert_unchecked(index.is_multiple_of(1 << order));
            assert_unchecked(index < 1 << max_order);
            assert_unchecked(order <= max_order);
        }

        while order < max_order {
            // Was false, now true: Can't merge, exactly one buddy is still occupied
            // Was true, now false: Can merge, no buddies are occupied
            let can_merge = unsafe { split_bitset.get_and_toggle(Self::bit_index(index, order, max_order)) };
            if can_merge {
                let buddy_index = index ^ (1 << order);

                // Remove the buddy from the free list
                let buddy_node = &mut list_nodes[buddy_index as usize];
                match [buddy_node.prev.take(), buddy_node.next.take()] {
                    // `prev.is_none()` means this is the head in the free list`
                    [None, new_head] => {
                        free_lists.heads[order as usize].head = new_head;
                        if let Some(new_head) = new_head {
                            list_nodes[new_head.get() as usize].prev = None;
                        }
                    }
                    [Some(prev), next] => {
                        list_nodes[prev.get() as usize].next = next;
                        if let Some(next) = next {
                            list_nodes[next.get() as usize].prev = Some(prev);
                        }
                    }
                }

                index &= !(1 << order);
                order += 1;
            } else {
                break
            }
        }

        list_nodes[index as usize].prev = None;
        if let Some(prev_head) = free_lists.heads[order as usize].head.replace(unsafe { NonMaxU32::new_unchecked(index) }) {
            list_nodes[prev_head.get() as usize].prev = NonMaxU32::new(index);
            list_nodes[index as usize].next = Some(prev_head);
        }
    }

    #[inline]
    fn bit_index(index: u32, order: u32, max_order: u32) -> u32 {
        debug_assert!(order < max_order, "`order` ({order}) must be less than `max_order` ({max_order})");

        let layer_base = (1 << (max_order - order - 1)) - 1;
        let pair_index = index >> (order + 1);

        layer_base + pair_index
    }

    pub const fn layout(count: usize) -> Result<AllocTreeLayout, LayoutError> {
        let max_order = count.ilog2();
        let taken = 1 << max_order;

        // `order`
        let layout = Layout::new::<u32>();
        // `nodes_offset`
        let (layout, ..) = layout.extend(Layout::new::<usize>())?;
        // `split_bitset_offset`
        let (layout, ..) = layout.extend(Layout::new::<usize>())?;

        // `free_nodes`
        let (layout, free_nodes_offset_abs) = layout.extend(Layout::array::<ListHead>((max_order + 1) as usize)?)?;
        // `nodes`split_bitset_offset
        let (layout, nodes_offset_abs) = layout.extend(Layout::array::<ListNode>(taken)?)?;
        // `split_bitset`
        let (layout, split_bitset_offset_abs) = layout.extend(Layout::array::<u32>(AllocBitset::size_for(taken - 1))?)?;
        let layout = layout.pad_to_align();

        Ok(AllocTreeLayout {
            layout,
            free_nodes_offset_abs,
            nodes_offset_abs,
            split_bitset_offset_abs,
            max_order,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AllocTreeLayout {
    layout: Layout,
    // ..while the offsets here are absolute.
    free_nodes_offset_abs: usize,
    nodes_offset_abs: usize,
    split_bitset_offset_abs: usize,
    max_order: u32,
}

impl AllocTreeLayout {
    #[inline]
    pub const fn node_count(&self) -> usize {
        1 << self.max_order
    }

    #[inline]
    pub const fn size(&self) -> usize {
        self.layout.size()
    }

    #[inline]
    pub const fn align(&self) -> usize {
        self.layout.align()
    }
}

// Safety notes: The repr and align must be the same as `ListHead`.
#[repr(C, align(4))]
struct AllocTreeData([MaybeUninit<u8>]);

#[repr(transparent)]
struct FreeLists<'a> {
    heads: &'a mut [ListHead],
}

#[derive(Debug, Clone, Copy)]
#[repr(C, align(4))]
struct ListHead {
    head: Option<NonMaxU32>,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ListNode {
    prev: Option<NonMaxU32>,
    next: Option<NonMaxU32>,
}

#[repr(transparent)]
struct AllocBitset([u32]);
impl AllocBitset {
    #[inline]
    const fn size_for(bits: usize) -> usize {
        bits.div_ceil(u32::BITS as usize)
    }

    #[inline]
    unsafe fn get_and_toggle(&mut self, bit: u32) -> bool {
        let block_index = bit / u32::BITS;
        let block_bit = 1 << (bit & (u32::BITS - 1));

        let block = unsafe { self.0.get_unchecked_mut(block_index as usize) };
        let old_block = *block;

        *block = old_block ^ block_bit;
        old_block & block_bit != 0
    }
}
