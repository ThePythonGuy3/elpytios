mod phys;
pub use phys::*;

use crate::PAGE_SIZE;

const _: () = assert!(size_of::<usize>() == size_of::<u64>());

const fn assert_size_align<T>() {
    // `assert_eq!` is non-const
    assert!(size_of::<T>() == PAGE_SIZE);
    assert!(align_of::<T>() == PAGE_SIZE);
}
