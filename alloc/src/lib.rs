#![feature(
    arbitrary_self_types_pointers,
    atomic_ptr_null,
    const_trait_impl,
    pointer_is_aligned_to,
    ptr_alignment_type
)]
#![no_std]

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    hint::{cold_path, spin_loop, unreachable_unchecked},
    marker::PhantomData,
    mem::Alignment,
    ptr,
    sync::atomic::{
        AtomicPtr, AtomicUsize,
        Ordering::{Acquire, Relaxed, Release},
    },
};

use elpytios_abi::{ALLOC_ALIGNMENT, PAGE_SIZE};

/// # Safety
/// - Allocations of sizes up to [`ALLOC_ALIGNMENT`] must be aligned to the nearest power of two of
///   that size.
pub unsafe trait PageAllocator {
    fn alloc(&self, order: u32) -> Option<*mut u8>;

    unsafe fn dealloc(&self, ptr: *mut u8, order: u32);
}

const MICRO_SIZES: [usize; 8] = [8, 16, 24, 32, 48, 64, 96, 128];
const SMALL_SIZES: [usize; 8] = [192, 256, 384, 512, 768, 1024, 1536, 2048];
const MEDIUM_SIZES: [usize; 4] = [3072, 4096, 6144, 8192];
const LARGE_SIZES: [usize; 4] = [12288, 16384, 24576, 32768];

pub struct HeapAllocator<T: PageAllocator> {
    page_alloc: T,
    micro_bins: [AtomicPtr<MicroSegment<T>>; MICRO_SIZES.len()],
    small_bins: [AtomicPtr<SmallSegment<T>>; SMALL_SIZES.len()],
    medium_bins: [AtomicPtr<MediumSegment<T>>; MEDIUM_SIZES.len()],
    large_bins: [AtomicPtr<LargeSegment<T>>; LARGE_SIZES.len()],
}

#[repr(C, align(4096))]
struct Segment<T: PageAllocator, const N: usize> {
    data: UnsafeCell<[u8; N]>,
    meta: SegmentMeta,
    _marker: PhantomData<*const T>,
}

impl<T: PageAllocator, const N: usize> Segment<T, N> {
    const LOCK: usize = 1 << (usize::BITS - 1);
    const MASK: usize = !Self::LOCK;

    #[inline]
    fn new(page_alloc: &T, size_class: usize) -> *mut Self {
        // Ensure that segment allocations are always aligned
        _ = const {
            assert!(size_of::<Self>().is_power_of_two());
            assert!(ALLOC_ALIGNMENT.as_usize().is_multiple_of(size_of::<Self>()));
        };

        let this = page_alloc
            .alloc(size_of::<Self>().ilog2())
            .expect("Couldn't allocate pages for heap allocator")
            .cast::<Self>();
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
                head_and_lock: AtomicUsize::new(0),
                next: UnsafeCell::new(ptr::null_mut()),
                available: UnsafeCell::new(available),
                offset,
            });
        }

        this
    }

    // `detach()` is called while this segment is still locked
    unsafe fn alloc(self: *mut Self, size_class: usize, detach: impl FnOnce(*mut *mut Self)) -> *mut u8 {
        unsafe {
            let meta = &(*self).meta;
            let mut curr_head = meta.head_and_lock.load(Relaxed) & Self::MASK;

            loop {
                match meta
                    .head_and_lock
                    .compare_exchange_weak(curr_head, curr_head | Self::LOCK, Acquire, Relaxed)
                {
                    Ok(..) => {
                        break if let Some(new_available) = meta.available.get().read().checked_sub(1) {
                            let data = UnsafeCell::raw_get(&raw const (*self).data)
                                .cast::<u8>()
                                .byte_add(meta.offset + curr_head * size_class);

                            meta.available.get().write(new_available);
                            match new_available {
                                0 => {
                                    detach(meta.next.get().cast());
                                    meta.head_and_lock.store(0, Release);
                                    data
                                }
                                _ => {
                                    let next_head = data.cast::<u16>().read();
                                    meta.head_and_lock.store(next_head as usize, Release);
                                    data
                                }
                            }
                        } else {
                            meta.head_and_lock.store(curr_head, Release);
                            ptr::null_mut()
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

    // `resurrect()` is called while this segment is still locked
    unsafe fn dealloc(self: *mut Self, size_class: usize, at: *mut u8, resurrect: impl FnOnce()) {
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
                    Ok(..) => {
                        break match meta.available.get().read() {
                            0 => {
                                meta.available.get().write(1);
                                resurrect();
                                meta.head_and_lock.store(at_index, Release);
                            }
                            available => {
                                at.cast::<u16>().write(curr_head as u16);
                                meta.available.get().write(available + 1);
                                meta.head_and_lock.store(at_index, Release);
                            }
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
}

// `align(64)` fits the meta to a cache line
#[repr(C, align(64))]
struct SegmentMeta {
    head_and_lock: AtomicUsize,
    /// Synchronizes-with `head_and_lock`.
    next: UnsafeCell<*mut ()>,
    /// Synchronizes with top-level `head_ptr`.
    available: UnsafeCell<usize>,
    offset: usize,
}

type MicroSegment<T> = Segment<T, { PAGE_SIZE - size_of::<SegmentMeta>() }>;
type SmallSegment<T> = Segment<T, { (4 * PAGE_SIZE) - size_of::<SegmentMeta>() }>;
type MediumSegment<T> = Segment<T, { (16 * PAGE_SIZE) - size_of::<SegmentMeta>() }>;
type LargeSegment<T> = Segment<T, { (32 * PAGE_SIZE) - size_of::<SegmentMeta>() }>;

enum SizeClassify<'a, T: PageAllocator> {
    Micro(&'a AtomicPtr<MicroSegment<T>>, usize),
    Small(&'a AtomicPtr<SmallSegment<T>>, usize),
    Medium(&'a AtomicPtr<MediumSegment<T>>, usize),
    Large(&'a AtomicPtr<LargeSegment<T>>, usize),
    Huge,
}

impl<'a, T: PageAllocator> SizeClassify<'a, T> {
    #[inline]
    unsafe fn new(alloc: &'a HeapAllocator<T>, size: usize) -> Self {
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

impl<T: PageAllocator> HeapAllocator<T> {
    pub const fn new(page_alloc: T) -> Self {
        Self {
            page_alloc,
            micro_bins: [const { AtomicPtr::null() }; _],
            small_bins: [const { AtomicPtr::null() }; _],
            medium_bins: [const { AtomicPtr::null() }; _],
            large_bins: [const { AtomicPtr::null() }; _],
        }
    }

    fn alloc<const N: usize>(page_alloc: &T, head: &AtomicPtr<Segment<T, N>>, size_class: usize) -> *mut u8 {
        let mut head_ptr = head.load(Relaxed);
        loop {
            // HEAD is locked (least-significant bit is set)
            if !head_ptr.is_aligned() {
                spin_loop();

                head_ptr = head.load(Relaxed);
                continue
            }

            // HEAD is null, lock and allocate a new one
            // This races with `dealloc()`'s resurrection logic when HEAD is null
            if head_ptr.is_null() {
                match head.compare_exchange(head_ptr, head_ptr.wrapping_byte_add(1), Acquire, Relaxed) {
                    Ok(..) => {
                        let new = Segment::new(page_alloc, size_class);
                        head.store(new, Release);

                        head_ptr = new;
                    }
                    Err(curr_segment_ptr) => {
                        head_ptr = curr_segment_ptr;
                        spin_loop();

                        continue
                    }
                }
            }

            unsafe {
                break match head_ptr.alloc(size_class, |next| {
                    loop {
                        // Invariant:
                        // - HEAD is always `head_ptr`, either locked or unlocked
                        // - This is ensured because if `head_ptr` is not null, only `alloc()` ever changes HEAD directly
                        match head.compare_exchange_weak(head_ptr, head_ptr.wrapping_byte_add(1), Acquire, Relaxed) {
                            // Scenario A: Absolutely no detached segments is available:
                            //             - `next` is null, and `alloc()` will lock and allocate a new segment
                            // Scenario B: A detached segment tries to resurrects, but `alloc()` wins the race:
                            //             - `next` is null
                            //             - If `alloc()`'s new segment wins the race, `dealloc()` will set `next` of new HEAD
                            //             - If `dealloc()` wins the race, `next` will not be null
                            Ok(..) => {
                                head.store(ptr::replace(next, ptr::null_mut()), Release);
                                break
                            }
                            // Don't bother updating `head_ptr`, it won't change to a new segment
                            // It may be locked by `dealloc()`, however, so do a spin-loop
                            Err(..) => spin_loop(),
                        }
                    }
                }) {
                    at if !at.is_null() => at,
                    _ => {
                        spin_loop();
                        head_ptr = head.load(Relaxed);
                        continue
                    }
                }
            }
        }
    }

    fn dealloc<const N: usize>(head: &AtomicPtr<Segment<T, N>>, size_class: usize, at: *mut u8) {
        let segment_ptr = (at as usize & !(size_of::<Segment<T, N>>() - 1)) as *mut Segment<T, N>;
        unsafe {
            segment_ptr.dealloc(size_class, at, || {
                let mut head_ptr = head.load(Relaxed);
                loop {
                    if !head_ptr.is_aligned() {
                        spin_loop();

                        head_ptr = head.load(Relaxed);
                        continue
                    }

                    // If HEAD is null and `dealloc()` wins the race, set HEAD directly
                    if head_ptr.is_null() {
                        match head.compare_exchange(head_ptr, head_ptr.wrapping_byte_add(1), Acquire, Relaxed) {
                            Ok(..) => {
                                head.store(segment_ptr, Release);
                                break
                            }
                            Err(curr_head_ptr) => {
                                head_ptr = curr_head_ptr;
                                spin_loop();
                                continue
                            }
                        }
                    }

                    // Otherwise, set HEAD's `next` instead
                    match head.compare_exchange_weak(head_ptr, head_ptr.wrapping_byte_add(1), Acquire, Relaxed) {
                        Ok(..) => {
                            let prev_next = ptr::replace((*head_ptr).meta.next.get(), segment_ptr.cast());
                            (*segment_ptr).meta.next.get().write(prev_next);

                            head.store(head_ptr, Release);
                            break
                        }
                        Err(curr_head_ptr) => {
                            head_ptr = curr_head_ptr;
                            spin_loop();
                        }
                    }
                }
            });
        }
    }

    #[cold]
    fn alloc_huge(page_alloc: &T, layout: Layout) -> *mut u8 {
        if layout.alignment() <= ALLOC_ALIGNMENT {
            page_alloc
                .alloc(usize::BITS - (layout.size() - 1).leading_zeros())
                .unwrap_or(ptr::null_mut())
        } else {
            cold_path();
            ptr::null_mut()
        }
    }

    #[cold]
    unsafe fn dealloc_huge(page_alloc: &T, layout: Layout, at: *mut u8) {
        unsafe { page_alloc.dealloc(at, usize::BITS - (layout.size() - 1).leading_zeros()) }
    }

    #[inline]
    fn unionize(layout: Layout) -> Layout {
        let new_size = layout.size().next_multiple_of(size_of::<u16>());
        let new_align = layout.alignment().max(Alignment::of::<u16>());
        unsafe { Layout::from_size_alignment_unchecked(new_size, new_align).pad_to_align() }
    }
}

unsafe impl<T: PageAllocator> Sync for HeapAllocator<T> {}
unsafe impl<T: PageAllocator> GlobalAlloc for HeapAllocator<T> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let page_alloc = &self.page_alloc;
        let layout_padded = Self::unionize(layout);

        match unsafe { SizeClassify::new(self, layout_padded.size()) } {
            SizeClassify::Micro(ptr, size_class) => Self::alloc(page_alloc, ptr, size_class),
            SizeClassify::Small(ptr, size_class) => Self::alloc(page_alloc, ptr, size_class),
            SizeClassify::Medium(ptr, size_class) => {
                cold_path();
                Self::alloc(page_alloc, ptr, size_class)
            }
            SizeClassify::Large(ptr, size_class) => {
                cold_path();
                Self::alloc(page_alloc, ptr, size_class)
            }
            SizeClassify::Huge => Self::alloc_huge(page_alloc, layout),
        }
    }

    unsafe fn dealloc(&self, at: *mut u8, layout: Layout) {
        let layout_padded = Self::unionize(layout);
        unsafe {
            match SizeClassify::new(self, layout_padded.size()) {
                SizeClassify::Micro(ptr, size_class) => Self::dealloc(ptr, size_class, at),
                SizeClassify::Small(ptr, size_class) => Self::dealloc(ptr, size_class, at),
                SizeClassify::Medium(ptr, size_class) => {
                    cold_path();
                    Self::dealloc(ptr, size_class, at)
                }
                SizeClassify::Large(ptr, size_class) => {
                    cold_path();
                    Self::dealloc(ptr, size_class, at)
                }
                SizeClassify::Huge => Self::dealloc_huge(&self.page_alloc, layout, at),
            }
        }
    }
}
