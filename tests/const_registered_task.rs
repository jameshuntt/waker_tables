//! `RegisteredTask::from_raw_parts` is a `const fn`, so a task can be built in a `const`, as 0.1.4's
//! struct literal `RegisteredTask { key, ptr }` could, and a `const` task can fill a fixed table
//! with `[TASK; N]`. e5833df's `from_raw_parts` is not `const`, so this file does not build there
//! (E0015: cannot call non-const associated function in constants).

use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};

/// A zero-sized task, so every slot can point at the same dangling address. It counts its polls
/// in a static.
struct Tick;

static TICKS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: the functions touch no memory through `self`.
unsafe impl UnsafeWakeableTask for Tick {
    unsafe fn poll(&mut self) -> Poll<()> {
        TICKS.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(())
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(Tick, TICK_VTABLE);

const KEY: VTableKey = VTableKey { segment: "const_registered_task", kind: "tick", version: 1 };

// SAFETY: `Tick` is zero-sized, so a dangling, aligned, non-null pointer is valid for reads and
// writes of it for as long as any copy of this task exists, and it touches no memory to share.
const TICK: RegisteredTask = unsafe {
    RegisteredTask::from_raw_parts(
        KEY,
        UnsafeWakeablePtr { data: NonNull::<Tick>::dangling().as_ptr() as *mut (), vtable: &TICK_VTABLE },
    )
};

/// A table built in a `const` too.
const TABLE: [RegisteredTask; 4] = [TICK; 4];

#[test]
fn a_const_task_fills_a_fixed_table() {
    let mut table = TABLE;
    let mut ready = 0;
    for slot in &mut table {
        assert_eq!(slot.key(), KEY);
        assert!(slot.ptr().vtable.is_for::<Tick>());
        // SAFETY: Tick's functions assume nothing.
        if unsafe { slot.poll() }.is_ready() {
            ready += 1;
        }
    }
    assert_eq!(ready, 4);
    assert_eq!(TICKS.load(Ordering::SeqCst), 4);
}
