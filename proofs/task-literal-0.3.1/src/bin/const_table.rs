// A `RegisteredTask` written as a struct literal in a `const`, filling a fixed table as
// `[TICK; 4]`. Same source on 0.1.4 and 0.3.1.
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};

/// Zero-sized, so every slot can point at the same dangling address. It counts polls in a static.
pub struct Tick;
static TICKS: AtomicUsize = AtomicUsize::new(0);
// SAFETY: the functions touch no memory through `self`.
unsafe impl UnsafeWakeableTask for Tick {
    unsafe fn poll(&mut self) -> Poll<()> { TICKS.fetch_add(1, Ordering::SeqCst); Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Tick, TICK_VTABLE);

const KEY: VTableKey = VTableKey { segment: "probe", kind: "tick", version: 1 };
const TICK: RegisteredTask = RegisteredTask {
    key: KEY,
    ptr: UnsafeWakeablePtr { data: NonNull::<Tick>::dangling().as_ptr() as *mut (), vtable: &TICK_VTABLE },
};
const TABLE: [RegisteredTask; 4] = [TICK; 4];

fn main() {
    let mut table = TABLE;
    for slot in &mut table {
        // SAFETY: Tick's functions assume nothing, and a dangling pointer is valid for a zero-sized type.
        let _ = unsafe { slot.poll() };
    }
    println!("const table of {} tasks, polls = {}", table.len(), TICKS.load(Ordering::SeqCst));
}
