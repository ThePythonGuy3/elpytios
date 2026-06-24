use bitflags::bitflags;

#[repr(C)]
pub struct BuddyTree {
    size: usize,
    children: [BuddyTreeNode],
}

bitflags! {
    #[repr(transparent)]
    struct BuddyTreeNode: u8 {
        const HAS_CHILDREN = 1 << 0;
        const IS_OCCUPIED = 1 << 1;
    }
}
