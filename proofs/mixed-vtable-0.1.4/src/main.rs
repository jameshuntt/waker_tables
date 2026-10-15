// 0.1.4's task vtable has three `pub` fields and no type. Safe code builds one with `Big`'s poll,
// `RegisteredTask::new` takes it for a `Box<Small>`, and one `poll` follows, whose `# Safety`
// asks only for exclusive access.
use std::task::Poll;
use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, UnsafeWakeableVTable, VTableKey, GLOBAL_VTABLE_REGISTRY};

pub struct Small(u8);
pub struct Big([u64; 8]);

// SAFETY: touches only self.
unsafe impl UnsafeWakeableTask for Small {
    unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
// SAFETY: touches only self.
unsafe impl UnsafeWakeableTask for Big {
    unsafe fn poll(&mut self) -> Poll<()> { self.0[7] += 1; Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}
derive_unsafe_wakeable_vtable!(Small, SMALL_VTABLE);
derive_unsafe_wakeable_vtable!(Big, BIG_VTABLE);

// No unsafe: Big's poll with Small's wake and drop.
static MIXED: UnsafeWakeableVTable = UnsafeWakeableVTable {
    poll_fn: BIG_VTABLE.poll_fn,
    wake_fn: SMALL_VTABLE.wake_fn,
    drop_fn: SMALL_VTABLE.drop_fn,
};

fn main() {
    let key = VTableKey { segment: "probe", kind: "small", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(key, &MIXED);
    let mut task = RegisteredTask::new(Box::new(Small(0)), key);
    // SAFETY: RegisteredTask::poll asks only for exclusive access, which `&mut task` gives.
    let _ = unsafe { task.poll() };
}
