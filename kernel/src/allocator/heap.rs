use core::{
    self,
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    hint::{cold_path, spin_loop, unreachable_unchecked},
    mem::Alignment,
    sync::atomic::{
        AtomicPtr, AtomicUsize,
        Ordering::{Acquire, Relaxed, Release},
    },
};

use elpytios_bootinfo::PAGE_SIZE;
use log::debug;

use crate::{
    allocator::PHYS_ALLOC_ALIGNMENT,
    statics::{get_phys_alloc, phys_to_virt},
};

const MICRO_SIZES: [usize; 8] = [8, 16, 24, 32, 48, 64, 96, 128];
const SMALL_SIZES: [usize; 8] = [192, 256, 384, 512, 768, 1024, 1536, 2048];
const MEDIUM_SIZES: [usize; 4] = [3072, 4096, 6144, 8192];
const LARGE_SIZES: [usize; 4] = [12288, 16384, 24576, 32768];

pub struct HeapAllocator {
    micro_bins: [AtomicPtr<MicroSegment>; MICRO_SIZES.len()],
    small_bins: [AtomicPtr<SmallSegment>; SMALL_SIZES.len()],
    medium_bins: [AtomicPtr<MediumSegment>; MEDIUM_SIZES.len()],
    large_bins: [AtomicPtr<LargeSegment>; LARGE_SIZES.len()],
}

#[repr(C, align(4096))]
struct Segment<const N: usize> {
    data: UnsafeCell<[u8; N]>,
    meta: SegmentMeta,
}

impl<const N: usize> Segment<N> {
    const LOCK: usize = 1 << (usize::BITS - 1);
    const MASK: usize = !Self::LOCK;

    #[inline]
    fn new(size_class: usize) -> *mut Self {
        // Ensure that segment allocations are always aligned
        _ = const {
            assert!(size_of::<Self>().is_power_of_two());
            assert!(PHYS_ALLOC_ALIGNMENT.as_usize().is_multiple_of(size_of::<Self>()));
        };

        let addr = get_phys_alloc()
            .lock()
            .alloc(size_of::<Self>().ilog2())
            .expect("Couldn't allocate pages for heap allocator");
        let this = phys_to_virt(addr).ptr_mut::<Self>();
        debug_assert!(
            this.is_aligned_to(size_of::<Self>()),
            "Physical page allocator didn't allocate an aligned heap"
        );

        unsafe {
            let offset = UnsafeCell::raw_get(&raw const (*this).data) as usize % size_class;
            let available = ((N - offset) / size_class).min(1 << u16::BITS);

            let data = UnsafeCell::raw_get(&raw const (*this).data).cast::<u8>().byte_add(offset as usize);
            for i in 0..available {
                data.byte_add(i as usize * size_class).cast::<u16>().write(((i + 1) % available) as u16);
            }

            (&raw mut (*this).meta).write(SegmentMeta {
                next: AtomicPtr::null(),
                head_and_lock: AtomicUsize::new(0),
                available: UnsafeCell::new(available),
                offset,
            });

            debug!("New segment for size class {size_class}: Available={available}, Offset={offset}");
        }

        this
    }

    unsafe fn alloc(self: *mut Self, size_class: usize, new_head: impl FnOnce(*mut Self)) -> *mut u8 {
        #[inline(always)]
        fn new_head_never<T>(_: *mut T) {
            cfg_select! {
                debug_assertions => {
                    unreachable!("Newly allocated page was immediately full, somehow")
                }
                not(debug_assertions) => {
                    unsafe { core::hint::assert_unchecked() }
                }
            }
        }

        unsafe {
            let meta = &(*self).meta;
            let mut curr_head = meta.head_and_lock.load(Relaxed) & Self::MASK;

            loop {
                if let next = meta.next.load(Relaxed)
                    && !next.is_null()
                {
                    return next.cast::<Self>().alloc(size_class, new_head)
                }

                match meta
                    .head_and_lock
                    .compare_exchange_weak(curr_head, curr_head | Self::LOCK, Acquire, Relaxed)
                {
                    Ok(..) => {
                        if let Some(new_available) = meta.available.get().read().checked_sub(1) {
                            let data = UnsafeCell::raw_get(&raw const (*self).data)
                                .cast::<u8>()
                                .byte_add(meta.offset + curr_head * size_class);

                            meta.available.get().write(new_available);
                            if new_available == 0 {
                                cold_path();
                                meta.head_and_lock.store(0, Release);
                            } else {
                                let next_head = data.cast::<u16>().read();
                                meta.head_and_lock.store(next_head as usize, Release);
                            }

                            return data
                        } else if let next = meta.next.load(Relaxed)
                            && next.is_null()
                        {
                            cold_path();

                            let new_segment = Self::new(size_class);
                            let result = new_segment.alloc(size_class, new_head_never::<Self>);
                            new_head(new_segment);

                            meta.next.store(new_segment.cast(), Relaxed);
                            meta.head_and_lock.store(curr_head, Release);
                            return result
                        }
                    }
                    Err(updated_head) => {
                        curr_head = updated_head & Self::MASK;
                        spin_loop();
                    }
                }
            }
        }
    }

    unsafe fn dealloc(self: *mut Self, size_class: usize, at: *mut u8) {
        unsafe {
            let meta = &(*self).meta;
            let mut curr_head = meta.head_and_lock.load(Relaxed) & Self::MASK;

            let base = UnsafeCell::raw_get(&raw const (*self).data).cast::<u8>().byte_add(meta.offset);
            let at_index = at.byte_offset_from_unsigned(base) / size_class;

            loop {
                match meta
                    .head_and_lock
                    .compare_exchange_weak(curr_head, curr_head | Self::LOCK, Acquire, Relaxed)
                {
                    Ok(..) => match meta.available.get().read() {
                        // Set head directly to the freed node
                        0 => {
                            meta.available.get().write(1);
                            meta.head_and_lock.store(at_index, Release);
                            break
                        }
                        // Connect head from the freed node to the last freed noed
                        available => {
                            at.cast::<u16>().write(curr_head as u16);
                            meta.available.get().write(available + 1);
                            meta.head_and_lock.store(at_index, Release);
                            break
                        }
                    },
                    Err(updated_head) => {
                        curr_head = updated_head & Self::MASK;
                        spin_loop();
                    }
                }
            }
        }
    }
}

// `align(64)` fits the meta to a cache line
#[repr(C, align(64))]
struct SegmentMeta {
    next: AtomicPtr<()>,
    head_and_lock: AtomicUsize,
    available: UnsafeCell<usize>,
    offset: usize,
}

type MicroSegment = Segment<{ PAGE_SIZE - size_of::<SegmentMeta>() }>;
type SmallSegment = Segment<{ (4 * PAGE_SIZE) - size_of::<SegmentMeta>() }>;
type MediumSegment = Segment<{ (16 * PAGE_SIZE) - size_of::<SegmentMeta>() }>;
type LargeSegment = Segment<{ (32 * PAGE_SIZE) - size_of::<SegmentMeta>() }>;

enum SizeClassify<'a> {
    Micro(&'a AtomicPtr<MicroSegment>, usize),
    Small(&'a AtomicPtr<SmallSegment>, usize),
    Medium(&'a AtomicPtr<MediumSegment>, usize),
    Large(&'a AtomicPtr<LargeSegment>, usize),
    Huge,
}

impl<'a> SizeClassify<'a> {
    #[inline]
    unsafe fn new(alloc: &'a HeapAllocator, size: usize) -> Self {
        #[inline]
        unsafe fn get<'a, const N: usize, T, R>(
            ptrs: &'a [AtomicPtr<T>],
            size: usize,
            segment_sizes: [usize; N],
            f: impl FnOnce(&'a AtomicPtr<T>, usize) -> R,
        ) -> R {
            unsafe {
                let i = segment_sizes
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, segment_size)| Some((i, segment_size.checked_sub(size)?)))
                    .min_by_key(|&(.., segment_size)| segment_size)
                    .unwrap_unchecked()
                    .0;

                f(ptrs.get_unchecked(i), *segment_sizes.get_unchecked(i))
            }
        }

        const MICRO_LARGEST: usize = MICRO_SIZES[MICRO_SIZES.len() - 1];
        const SMALL_LARGEST: usize = SMALL_SIZES[SMALL_SIZES.len() - 1];
        const MEDIUM_LARGEST: usize = MEDIUM_SIZES[MEDIUM_SIZES.len() - 1];
        const LARGE_LARGEST: usize = LARGE_SIZES[LARGE_SIZES.len() - 1];

        unsafe {
            match size {
                0 => unreachable_unchecked(),
                ..=MICRO_LARGEST => get(&alloc.micro_bins, size, MICRO_SIZES, Self::Micro),
                ..=SMALL_LARGEST => get(&alloc.small_bins, size, SMALL_SIZES, Self::Small),
                ..=MEDIUM_LARGEST => get(&alloc.medium_bins, size, MEDIUM_SIZES, Self::Medium),
                ..=LARGE_LARGEST => get(&alloc.large_bins, size, LARGE_SIZES, Self::Large),
                _ => Self::Huge,
            }
        }
    }
}

impl HeapAllocator {
    pub const fn new() -> Self {
        Self {
            micro_bins: [const { AtomicPtr::null() }; _],
            small_bins: [const { AtomicPtr::null() }; _],
            medium_bins: [const { AtomicPtr::null() }; _],
            large_bins: [const { AtomicPtr::null() }; _],
        }
    }

    fn alloc<const N: usize>(ptr: &AtomicPtr<Segment<N>>, size_class: usize) -> *mut u8 {
        let mut segment_ptr = ptr.load(Acquire);
        if segment_ptr.is_null() {
            cold_path();
            loop {
                // 0th bit is used as allocation lock
                match ptr.compare_exchange(segment_ptr, segment_ptr.wrapping_byte_add(1), Acquire, Relaxed) {
                    Ok(..) => {
                        let new = Segment::new(size_class);
                        ptr.store(new, Release);

                        segment_ptr = new;
                        break
                    }
                    Err(curr_ptr) => {
                        segment_ptr = curr_ptr;
                        break
                    }
                }
            }
        }

        // If 0th bit is set, means the segment is still being allocated
        if !segment_ptr.is_aligned() {
            cold_path();
            loop {
                spin_loop();
                segment_ptr = ptr.load(Acquire);
                if segment_ptr.is_aligned() {
                    break
                }
            }
        }

        unsafe { segment_ptr.alloc(size_class, |new_head| ptr.store(new_head, Release)) }
    }

    #[inline]
    const fn unionize(layout: Layout) -> Layout {
        let new_size = layout.size().next_multiple_of(size_of::<u16>());
        let new_align = layout.alignment().max(Alignment::of::<u16>());
        unsafe { Layout::from_size_alignment_unchecked(new_size, new_align).pad_to_align() }
    }
}

unsafe impl Sync for HeapAllocator {}
unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let layout = Self::unionize(layout);
        match unsafe { SizeClassify::new(self, layout.size()) } {
            SizeClassify::Micro(ptr, size_class) => Self::alloc(ptr, size_class),
            SizeClassify::Small(ptr, size_class) => Self::alloc(ptr, size_class),
            SizeClassify::Medium(ptr, size_class) => {
                cold_path();
                Self::alloc(ptr, size_class)
            }
            SizeClassify::Large(ptr, size_class) => {
                cold_path();
                Self::alloc(ptr, size_class)
            }
            SizeClassify::Huge => {
                cold_path();
                todo!("huge object alloc")
            }
        }
    }

    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        let layout = Self::unionize(layout);

        #[inline]
        unsafe fn dealloc<const N: usize>(head: &AtomicPtr<Segment<N>>, size_class: usize, at: *mut u8) {
            let segment = (at as usize & !(size_of::<Segment<N>>() - 1)) as *mut Segment<N>;
            unsafe { segment.dealloc(size_class, at) }
        }

        unsafe {
            match SizeClassify::new(self, layout.size()) {
                SizeClassify::Micro(ptr, size_class) => dealloc(ptr, size_class, at),
                SizeClassify::Small(ptr, size_class) => dealloc(ptr, size_class, at),
                SizeClassify::Medium(ptr, size_class) => {
                    cold_path();
                    dealloc(ptr, size_class, at)
                }
                SizeClassify::Large(ptr, size_class) => {
                    cold_path();
                    dealloc(ptr, size_class, at)
                }
                SizeClassify::Huge => {
                    cold_path();
                    todo!("huge object dealloc")
                }
            }
        }
    }
}
