use core::{
    marker::PhantomData,
    mem::{self, ManuallyDrop, MaybeUninit},
    num::NonZeroU16,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

use elpytios_bootinfo::PAGE_SIZE;

use crate::{
    spin_sync::SpinMutex,
    statics::{get_phys_alloc, phys_to_virt},
};

#[repr(C)]
pub struct SlabAllocator<T> {
    slots: SpinMutex<Option<[NonNull<Slab<T>>; 2]>>,
}

unsafe impl<T> Sync for SlabAllocator<T> {}
impl<T> SlabAllocator<T> {
    #[inline]
    pub const fn new() -> Self {
        Self { slots: SpinMutex::new(None) }
    }

    #[inline]
    fn as_uninit(&self) -> &SlabAllocator<MaybeUninit<T>> {
        unsafe { mem::transmute(self) }
    }

    #[inline]
    pub fn alloc(&self, item: T) -> SlabId<'_, T> {
        let mut slab = self.alloc_uninit();
        slab.write(item);
        unsafe { SlabId::assume_init(slab) }
    }

    pub fn alloc_uninit(&self) -> SlabId<'_, MaybeUninit<T>> {
        let mut slots = self.slots.lock();
        let [free, ..] = slots.get_or_insert_with(|| {
            let slot = Slab::new();
            [slot, slot]
        });

        let ptr = loop {
            match unsafe { free.as_mut() }.alloc() {
                Ok(ptr) => break ptr,
                Err(next_free) => *free = next_free,
            }
        };

        SlabId {
            alloc: self.as_uninit(),
            slot: free.cast(),
            ptr: ptr.cast(),
        }
    }

    unsafe fn dealloc(&self, mut slot: NonNull<Slab<T>>, ptr: NonNull<T>) {
        let mut slots = self.slots.lock();
        unsafe {
            slot.as_mut().dealloc(ptr);
            slots.as_mut().unwrap_unchecked()[0] = slot;
        }
    }
}

impl<T> SlabAllocator<MaybeUninit<T>> {
    #[inline]
    fn assume_init(&self) -> &SlabAllocator<T> {
        unsafe { mem::transmute(self) }
    }
}

#[repr(C)]
pub struct SlabId<'a, T> {
    alloc: &'a SlabAllocator<T>,
    slot: NonNull<Slab<T>>,
    ptr: NonNull<T>,
}

impl<'a, T> SlabId<'a, T> {
    #[inline]
    pub fn into_inner(this: Self) -> T {
        let this = ManuallyDrop::new(this);
        unsafe {
            let out = this.ptr.read();
            this.alloc.dealloc(this.slot, this.ptr);
            out
        }
    }

    #[inline]
    pub fn leak(this: Self) -> &'a mut T {
        let mut this = ManuallyDrop::new(this);
        unsafe { this.ptr.as_mut() }
    }
}

impl<'a, T> SlabId<'a, MaybeUninit<T>> {
    #[inline]
    pub unsafe fn assume_init(this: Self) -> SlabId<'a, T> {
        let this = ManuallyDrop::new(this);
        SlabId {
            alloc: this.alloc.assume_init(),
            slot: this.slot.cast(),
            ptr: this.ptr.cast(),
        }
    }
}

impl<T> Drop for SlabId<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            self.ptr.drop_in_place();
            self.alloc.dealloc(self.slot, self.ptr);
        }
    }
}

impl<T> Deref for SlabId<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for SlabId<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

#[repr(C, align(4096))]
struct Slab<T> {
    data: [MaybeUninit<u8>; PAGE_SIZE],
    _marker: PhantomData<[Slot<T>]>,
}

impl<T> Slab<T> {
    const LEN: usize = {
        let len = (PAGE_SIZE - size_of::<SlabMeta<T>>()) / size_of::<Slot<T>>();
        assert!(len != 0, "`T` is too large!");
        len
    };

    fn new() -> NonNull<Self> {
        let addr = get_phys_alloc().lock().alloc(0).expect("Couldn't allocate a page for slot allocator");
        unsafe {
            let ptr = phys_to_virt(addr).ptr_mut::<Self>();
            let (entries, meta) = ptr.fields();
            meta.write(SlabMeta::Available { free: 0 });

            for i in 0..Self::LEN {
                entries.add(i).write(Slot {
                    free: NonZeroU16::new(((i + 1) % Self::LEN) as u16),
                });
            }

            NonNull::new_unchecked(ptr)
        }
    }

    #[inline]
    unsafe fn fields(self: *mut Self) -> (*mut Slot<T>, *mut SlabMeta<T>) {
        unsafe { (self.cast(), self.byte_add(Self::LEN * size_of::<Slot<T>>()).cast()) }
    }

    fn alloc(&mut self) -> Result<NonNull<T>, NonNull<Self>> {
        unsafe {
            let (entries, meta) = (&raw mut *self).fields();
            let meta = meta.as_mut_unchecked();
            match meta {
                SlabMeta::Full { next_slot } => Err(*next_slot),
                SlabMeta::Available { free } => Ok({
                    let out = entries.add(*free as usize);
                    match (*out).free {
                        Some(next_free) => *free = next_free.get(),
                        None => *meta = SlabMeta::Full { next_slot: Self::new() },
                    }

                    NonNull::new_unchecked(&raw mut (*out).taken as *mut T)
                }),
            }
        }
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<T>) {
        unsafe {
            let (entries, meta) = (&raw mut *self).fields();
            let meta = meta.as_mut_unchecked();
            let ptr = ptr.as_ptr().cast::<Slot<T>>();

            let index = ptr.offset_from_unsigned(entries) as u16;
            match meta {
                SlabMeta::Full { .. } => {
                    ptr.write(Slot { free: None });
                    *meta = SlabMeta::Available { free: index };
                }
                SlabMeta::Available { free } => {
                    // `free` must be the lowest index
                    let free_ptr = entries.add(*free as usize);
                    if *free < index {
                        // From: head:free  --> [next_free]
                        // To  : head:free  --> index       --> [next_free]
                        ptr.write(ptr::replace(free_ptr, Slot {
                            free: Some(NonZeroU16::new_unchecked(index)),
                        }));
                    } else {
                        // From: head:free  --> [next_free]
                        // To  : head:index --> free        --> [next_free]
                        ptr.write(Slot {
                            free: Some(NonZeroU16::new_unchecked(*free)),
                        });
                        *free = index;
                    }
                }
            }
        }
    }
}

#[repr(C)]
enum SlabMeta<T> {
    Full { next_slot: NonNull<Slab<T>> },
    Available { free: u16 },
}

union Slot<T> {
    taken: ManuallyDrop<T>,
    free: Option<NonZeroU16>,
}
