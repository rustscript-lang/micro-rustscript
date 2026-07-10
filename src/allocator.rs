use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;
use core::ptr;

struct PlatformAllocator;

unsafe extern "C" {
    fn rustscript_platform_alloc(size: usize, align: usize) -> *mut u8;
    fn rustscript_platform_dealloc(pointer: *mut u8, size: usize, align: usize);
}

unsafe impl GlobalAlloc for PlatformAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return ptr::NonNull::<u8>::dangling().as_ptr();
        }
        unsafe { rustscript_platform_alloc(layout.size(), layout.align()) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if layout.size() != 0 {
            unsafe { rustscript_platform_dealloc(pointer, layout.size(), layout.align()) };
        }
    }
}

#[global_allocator]
static ALLOCATOR: PlatformAllocator = PlatformAllocator;

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
