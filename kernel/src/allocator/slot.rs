use core::{
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZeroU16,
    ops::{Deref, DerefMut},
    ptr::{self, NonNull},
};

use elpytios_bootinfo::PAGE_SIZE;

use crate::{
    allocator::AllocId,
    spin_sync::SpinMutex,
    statics::{get_phys_alloc, phys_to_virt},
};

pub struct SlotAllocator<T> {
    slots: SpinMutex<Option<[NonNull<Slot<T>>; 2]>>,
}

impl<T> SlotAllocator<T> {
    #[inline]
    pub const fn new() -> Self {
        Self { slots: SpinMutex::new(None) }
    }

    pub fn alloc(&self, mut item: T) -> SlotId<'_, T> {
        let mut slots = self.slots.lock();
        let [free, ..] = slots.get_or_insert_with(|| {
            let slot = Slot::new();
            [slot, slot]
        });

        let ptr = loop {
            match unsafe { free.as_mut() }.alloc(item) {
                Ok(ptr) => break ptr,
                Err((next_item, next_free)) => {
                    item = next_item;
                    *free = next_free
                }
            }
        };

        SlotId {
            alloc: self,
            slot: *free,
            ptr,
        }
    }

    unsafe fn dealloc(&self, mut slot: NonNull<Slot<T>>, ptr: NonNull<T>) {
        let mut slots = self.slots.lock();
        unsafe {
            slot.as_mut().dealloc(ptr);
            slots.as_mut().unwrap_unchecked()[0] = slot;
        }
    }
}

pub struct SlotId<'a, T> {
    alloc: &'a SlotAllocator<T>,
    slot: NonNull<Slot<T>>,
    ptr: NonNull<T>,
}

impl<T> SlotId<'_, T> {
    #[inline]
    pub fn into_inner(self) -> T {
        unsafe {
            let out = self.ptr.read();
            self.alloc.dealloc(self.slot, self.ptr);
            out
        }
    }
}

impl<T> Drop for SlotId<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            self.ptr.drop_in_place();
            self.alloc.dealloc(self.slot, self.ptr);
        }
    }
}

impl<T> Deref for SlotId<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for SlotId<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

#[repr(C, align(4096))]
struct Slot<T> {
    data: [MaybeUninit<u8>; PAGE_SIZE],
    _marker: PhantomData<[Entry<T>]>,
}

impl<T> Slot<T> {
    const LEN: usize = {
        let len = (PAGE_SIZE - size_of::<SlotMeta<T>>()) / size_of::<Entry<T>>();
        assert!(len != 0, "`T` is too large!");
        len
    };

    fn new() -> NonNull<Self> {
        let id = get_phys_alloc().lock().alloc(1).expect("Couldn't allocate a page for slot allocator");
        unsafe {
            let ptr = phys_to_virt(id.addr()).ptr_mut::<Self>();
            let (entries, meta) = ptr.fields();
            meta.write(SlotMeta {
                id,
                len: SlotLen::Available { free: 0 },
            });

            for i in 0..Self::LEN {
                entries.add(i).write(Entry {
                    free: NonZeroU16::new(((i + 1) % Self::LEN) as u16),
                });
            }

            NonNull::new_unchecked(ptr)
        }
    }

    #[inline]
    unsafe fn fields(self: *mut Self) -> (*mut Entry<T>, *mut SlotMeta<T>) {
        unsafe { (self.cast(), self.byte_add(Self::LEN * size_of::<Entry<T>>()).cast()) }
    }

    fn alloc(&mut self, item: T) -> Result<NonNull<T>, (T, NonNull<Self>)> {
        unsafe {
            let (entries, meta) = (&raw mut *self).fields();
            match &mut (*meta).len {
                SlotLen::Full { next_slot } => Err((item, *next_slot)),
                SlotLen::Available { free } => Ok({
                    let out = entries.add(*free as usize);
                    match ptr::replace(out, Entry {
                        taken: ManuallyDrop::new(item),
                    })
                    .free
                    {
                        Some(next_free) => *free = next_free.get(),
                        None => (*meta).len = SlotLen::Full { next_slot: Self::new() },
                    }

                    NonNull::new_unchecked(out.cast())
                }),
            }
        }
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<T>) {
        unsafe {
            let (entries, meta) = (&raw mut *self).fields();
            let ptr = ptr.as_ptr().cast::<Entry<T>>();

            let index = ptr.offset_from_unsigned(entries) as u16;
            match &mut (*meta).len {
                SlotLen::Full { .. } => {
                    ptr.write(Entry { free: None });
                    (*meta).len = SlotLen::Available { free: index };
                }
                SlotLen::Available { free } => {
                    // `free` must be the lowest index
                    let free_ptr = entries.add(*free as usize);
                    if *free < index {
                        // From: head:free  --> [next_free]
                        // To  : head:free  --> index       --> [next_free]
                        ptr.write(ptr::replace(free_ptr, Entry {
                            free: Some(NonZeroU16::new_unchecked(index)),
                        }));
                    } else {
                        // From: head:free  --> [next_free]
                        // To  : head:index --> free        --> [next_free]
                        ptr.write(Entry {
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
struct SlotMeta<T> {
    id: AllocId,
    len: SlotLen<T>,
}

enum SlotLen<T> {
    Full { next_slot: NonNull<Slot<T>> },
    Available { free: u16 },
}

union Entry<T> {
    taken: ManuallyDrop<T>,
    free: Option<NonZeroU16>,
}
