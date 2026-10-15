//! A round-robin task table on `RegisteredTask`: two task types, each registered under its
//! own key, polled until every one is ready, then freed. A third spawn uses a key registered
//! for the other type and is refused.
//!
//!   cargo run --example task_table

use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY,
};

static FREED: AtomicUsize = AtomicUsize::new(0);

/// Ready after counting down to zero.
struct Countdown {
    name: &'static str,
    left: u32,
}

/// Ready after summing `n` numbers, one per poll.
struct Summer {
    n: u64,
    next: u64,
    sum: u64,
}

impl Drop for Countdown {
    fn drop(&mut self) {
        FREED.fetch_add(1, Ordering::SeqCst);
    }
}

impl Drop for Summer {
    fn drop(&mut self) {
        FREED.fetch_add(1, Ordering::SeqCst);
    }
}

// SAFETY: the functions only touch `self`; they assume nothing else.
unsafe impl UnsafeWakeableTask for Countdown {
    unsafe fn poll(&mut self) -> Poll<()> {
        if self.left == 0 {
            println!("  {} done", self.name);
            return Poll::Ready(());
        }
        self.left -= 1;
        Poll::Pending
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

// SAFETY: as Countdown.
unsafe impl UnsafeWakeableTask for Summer {
    unsafe fn poll(&mut self) -> Poll<()> {
        if self.next > self.n {
            println!("  sum of 1..={} is {}", self.n, self.sum);
            return Poll::Ready(());
        }
        self.sum += self.next;
        self.next += 1;
        Poll::Pending
    }

    unsafe fn wake(&self) {}

    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(Countdown, COUNTDOWN_VTABLE);
derive_unsafe_wakeable_vtable!(Summer, SUMMER_VTABLE);

const COUNTDOWN: VTableKey = VTableKey { segment: "demo", kind: "countdown", version: 1 };
const SUMMER: VTableKey = VTableKey { segment: "demo", kind: "summer", version: 1 };

fn main() {
    GLOBAL_VTABLE_REGISTRY.register(COUNTDOWN, &COUNTDOWN_VTABLE);
    GLOBAL_VTABLE_REGISTRY.register(SUMMER, &SUMMER_VTABLE);

    let mut table = vec![
        RegisteredTask::new(Box::new(Countdown { name: "short", left: 2 }), COUNTDOWN),
        RegisteredTask::new(Box::new(Summer { n: 10, next: 1, sum: 0 }), SUMMER),
        RegisteredTask::new(Box::new(Countdown { name: "long", left: 5 }), COUNTDOWN),
    ];

    let mut round = 0;
    while !table.is_empty() {
        round += 1;
        println!("round {round}: {} task(s)", table.len());
        // SAFETY: the tasks' poll functions assume nothing beyond exclusive access.
        table.retain_mut(|task| unsafe { task.poll() }.is_pending());
    }
    println!("all done after {round} rounds; {} task(s) freed", FREED.load(Ordering::SeqCst));
    assert_eq!(FREED.load(Ordering::SeqCst), 3);

    let refused = std::panic::catch_unwind(|| {
        RegisteredTask::new(Box::new(Countdown { name: "wrong key", left: 1 }), SUMMER)
    });
    println!("a Countdown under the Summer key is refused: {}", refused.is_err());
    assert!(refused.is_err());
    assert_eq!(FREED.load(Ordering::SeqCst), 4, "the refused task is freed too");
}
