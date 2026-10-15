// readme.md Example 4 from 0.1.4 (e5b534e), lines 263-311, unchanged.
use std::collections::VecDeque;
use std::task::Poll;

use waker_tables::{UnsafeWakeablePtr, UnsafeWakeableTask, derive_unsafe_wakeable_vtable};

struct TickTask {
    ticks_left: u32,
}

unsafe impl UnsafeWakeableTask for TickTask {
    unsafe fn poll(&mut self) -> Poll<()> {
        if self.ticks_left == 0 {
            Poll::Ready(())
        } else {
            self.ticks_left -= 1;
            Poll::Pending
        }
    }

    unsafe fn wake(&self) {
        // In this toy scheduler, wake is a no-op.
        // In a real one, you'd push self back onto the run queue.
    }

    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(TickTask, TICK_VTABLE);

fn main() {
    // run queue holds erased tasks
    let mut q: VecDeque<UnsafeWakeablePtr> = VecDeque::new();

    // enqueue one task
    let boxed = Box::new(TickTask { ticks_left: 3 });
    q.push_back(UnsafeWakeablePtr {
        data: Box::into_raw(boxed) as *mut (),
        vtable: &TICK_VTABLE,
    });

    // drive until ready
    while let Some(mut t) = q.pop_front() {
        let p = unsafe { t.poll() };
        match p {
            Poll::Ready(()) => unsafe { t.drop_in_place() },
            Poll::Pending => q.push_back(t),
        }
    }
}
