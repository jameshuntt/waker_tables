// A task vtable with `Big`'s functions and `Small`'s type, written with no `unsafe`, then one
// `RegisteredTask::poll`, whose `# Safety` asks only for the task's own poll contract.
use std::any::TypeId;
use std::task::Poll;
use waker_tables::{RegisteredTask, UnsafeWakeableTask, UnsafeWakeableVTable, VTableKey, GLOBAL_VTABLE_REGISTRY};

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

fn small_type_id() -> TypeId { TypeId::of::<Small>() }
fn small_name() -> &'static str { "Small" }

// No unsafe: Big's functions, Small's type.
static LIAR: UnsafeWakeableVTable = UnsafeWakeableVTable {
    type_id: small_type_id,
    type_name: small_name,
    ..UnsafeWakeableVTable::of::<Big>()
};

fn main() {
    let key = VTableKey { segment: "probe", kind: "liar", version: 1 };
    GLOBAL_VTABLE_REGISTRY.register(key, &LIAR);
    let mut task = RegisteredTask::new(Box::new(Small(0)), key); // the type check passes
    // SAFETY: RegisteredTask::poll asks for the task's own poll contract; Small's poll has none.
    let _ = unsafe { task.poll() };
}
