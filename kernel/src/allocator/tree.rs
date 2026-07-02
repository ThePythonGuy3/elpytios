#[cfg(debug_assertions)]
use core::sync::atomic::{AtomicU32, Ordering::Relaxed};
use core::{
    alloc::{Layout, LayoutError},
    fmt,
    mem::MaybeUninit,
    ptr, slice,
};

use nonmax::NonMaxU32;

use crate::allocator::AllocBitset;

#[cfg(debug_assertions)]
static TREE_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy)]
pub enum TreeAllocError {
    Zero,
    InsufficientSpace { requested: usize },
}

impl fmt::Debug for TreeAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for TreeAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => write!(f, "Can't create a zero-sized allocation"),
            Self::InsufficientSpace { requested } => write!(f, "Tree can no longer contain allocation of size {requested}"),
        }
    }
}

/// A binary buddy tree, implemented with a split bitset and free lists. The tree operates on number
/// of "order," not leaf counts; i.e., the leaf count must be `2 ^ order`.
#[repr(C)]
pub struct AllocTree {
    max_order: u32,
    #[cfg(debug_assertions)]
    id: u32,
    // The offsets here are relative to the offset of `data`...
    nodes_offset: usize,
    split_bitset_offset: usize,
    data: AllocTreeData,
}

struct AllocTreeFields<'a> {
    max_order: u32,
    #[cfg(debug_assertions)]
    id: u32,
    free_lists: &'a mut [ListHead],
    list_nodes: &'a mut [ListNode],
    split_bitset: &'a mut AllocBitset,
}

impl AllocTree {
    pub unsafe fn new(at: *mut (), layout: AllocTreeLayout) -> *mut Self {
        let this = ptr::from_raw_parts_mut::<Self>(at, layout.size() - layout.free_nodes_offset_abs);
        unsafe {
            #[cfg(debug_assertions)]
            (&raw mut (*this).id).write(TREE_ID.fetch_add(1, Relaxed));

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
                #[cfg(debug_assertions)]
                id: self.id,
                max_order,
                free_lists: slice::from_raw_parts_mut(data.cast(), max_order as usize + 1),
                list_nodes: slice::from_raw_parts_mut(data.add(self.nodes_offset).cast(), 1 << max_order),
                split_bitset: ptr::from_raw_parts_mut::<AllocBitset>(data.add(self.split_bitset_offset), AllocBitset::size_for((1 << max_order) - 1))
                    .as_mut_unchecked(),
            }
        }
    }

    /// # Notes
    /// - This will round allocations up to the next power of two, so it's best to use power of twos
    ///   directly.
    pub fn alloc(&mut self, size: usize) -> Result<TreeAllocId, TreeAllocError> {
        if size == 0 {
            return Err(TreeAllocError::Zero)
        }

        let AllocTreeFields {
            #[cfg(debug_assertions)]
            id,
            max_order,
            free_lists,
            list_nodes,
            split_bitset,
        } = self.fields();

        let order = usize::BITS - (size - 1).leading_zeros();
        if order > max_order {
            return Err(TreeAllocError::InsufficientSpace { requested: size })
        }

        let mut current = None;
        for i in order..=max_order {
            if let Some(head) = free_lists[i as usize].head.take() {
                if let Some(next_in_head) = list_nodes[head.get() as usize].next.take() {
                    list_nodes[next_in_head.get() as usize].prev = None;
                    free_lists[i as usize].head = Some(next_in_head);
                }

                current = Some((head.get(), i));
                break
            }
        }

        let Some((index, mut current_order)) = current else { return Err(TreeAllocError::InsufficientSpace { requested: size }) };
        while current_order > order {
            let next_order = current_order - 1;

            let free = NonMaxU32::new(index + (1 << next_order)).expect("Allocation index >= u32::MAX");
            list_nodes[free.get() as usize].prev = None;

            if let Some(prev_head) = free_lists[next_order as usize].head.replace(free) {
                list_nodes[prev_head.get() as usize].prev = Some(free);
                list_nodes[free.get() as usize].next = Some(prev_head);
            } else {
                list_nodes[free.get() as usize].next = None;
            }

            current_order = next_order;
        }

        Ok(TreeAllocId {
            #[cfg(debug_assertions)]
            tree_id: id,
            index,
            order: current_order,
        })
    }

    pub const fn layout(count: usize) -> Result<AllocTreeLayout, LayoutError> {
        let max_order = count.ilog2();
        let taken = 1 << max_order;

        // `order`
        let layout = Layout::new::<u32>();
        cfg_select! {
            debug_assertions => {
                // `id`
                let (layout, ..) = layout.extend(Layout::new::<u32>())?;
            }
            _ => {}
        }
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
pub struct TreeAllocId {
    #[cfg(debug_assertions)]
    tree_id: u32,
    index: u32,
    order: u32,
}

impl TreeAllocId {
    #[inline]
    pub const fn index(&self) -> u32 {
        self.index
    }

    #[inline]
    pub const fn order(&self) -> u32 {
        self.order
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

// Safety notes: The repr and align must be the same as `FreeListHead`.
#[repr(C, align(4))]
struct AllocTreeData([MaybeUninit<u8>]);

#[derive(Clone, Copy)]
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
