use alloc::alloc::{alloc, handle_alloc_error};
use core::{alloc::LayoutError, fmt};

use elpytios_abi::{PAGE_LAYOUT, PAGE_SIZE};
use elpytios_bootinfo::paddr::PAddr;

use crate::{
    statics::virt_to_phys,
    vaddr::{VAddr, VFlags, VirtualMap, VirtualMapError},
};

mod imp {
    cfg_select! {
        target_arch = "x86_64" => {
            mod x86_64;
            pub use x86_64::*;
        }
        _ => {
            compile_error!("Unsupported architecture");
        }
    }
}
pub use imp::schedule;

mod process;
mod queue;
pub use process::*;
pub use queue::*;

#[derive(Clone)]
pub enum TaskCreateError {
    Layout(LayoutError),
    VMap(VirtualMapError),
}

impl fmt::Debug for TaskCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for TaskCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout(e) => write!(f, "Couldn't allocate memory for task memory: {e}"),
            Self::VMap(e) => write!(f, "Couldn't virtual-map task memory: {e}"),
        }
    }
}

impl From<LayoutError> for TaskCreateError {
    #[inline]
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

impl From<VirtualMapError> for TaskCreateError {
    #[inline]
    fn from(value: VirtualMapError) -> Self {
        Self::VMap(value)
    }
}

pub struct Task {
    user_stack: *mut u8,
    kernel_stack: *mut u8,
    virtual_map: VirtualMap,
    virtual_map_phys: PAddr,
    inner: imp::Task,
}

impl Task {
    pub const USER_STACK_PAGES: usize = 15;
    pub const KERNEL_STACK_PAGES: usize = 1;

    pub unsafe fn new(mut addr_start: VAddr, entry: VAddr, virtual_map: VirtualMap, virtual_map_phys: PAddr) -> Result<Self, TaskCreateError> {
        let mut next_addr = |count| {
            let prev = addr_start;
            addr_start = addr_start.byte_add(count * PAGE_SIZE);
            prev
        };

        unsafe {
            let user_stack_layout = PAGE_LAYOUT.repeat_packed(Self::USER_STACK_PAGES + 1)?;
            let user_stack = alloc(user_stack_layout);
            if user_stack.is_null() {
                handle_alloc_error(user_stack_layout)
            }

            let kernel_stack_layout = PAGE_LAYOUT.repeat_packed(Self::KERNEL_STACK_PAGES + 1)?;
            let kernel_stack = alloc(kernel_stack_layout);
            if kernel_stack.is_null() {
                handle_alloc_error(kernel_stack_layout)
            }

            let user_stack_lower = next_addr(Self::USER_STACK_PAGES);
            virtual_map.map(
                virt_to_phys(VAddr::new(user_stack.addr() + PAGE_SIZE)),
                user_stack_lower,
                Self::USER_STACK_PAGES,
                VFlags::USER_MODE | VFlags::WRITABLE,
            )?;

            let inner = imp::Task::new(
                entry,
                user_stack.add((Self::USER_STACK_PAGES + 1) * PAGE_SIZE),
                user_stack_lower.byte_add((Self::USER_STACK_PAGES + 1) * PAGE_SIZE),
                kernel_stack.add((Self::KERNEL_STACK_PAGES + 1) * PAGE_SIZE),
            );

            Ok(Self {
                user_stack,
                kernel_stack,
                virtual_map,
                virtual_map_phys,
                inner,
            })
        }
    }
}
