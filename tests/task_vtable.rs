//! The task half: `derive_unsafe_wakeable_vtable!`, `UnsafeWakeablePtr`, `VTableRegistry`,
//! `RegisteredTask`. Tasks own heap data and count their drops, so a task freed twice or never
//! fails here, and Miri reports the leak.

use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Poll;

use waker_tables::{
    derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey,
    VTableRegistry, GLOBAL_VTABLE_REGISTRY,
};

/// Ready after `ready_after` polls; counts its wakes, its `drop_in_place` calls and its drops.
struct Countdown {
    ready_after: usize,
    polls: usize,
    log: Arc<Log>,
    owned: Vec<u64>,
}

#[derive(Default)]
struct Log {
    wakes: AtomicUsize,
    cleanups: AtomicUsize,
    drops: AtomicUsize,
}

impl Log {
    fn get(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }
}

impl Countdown {
    fn boxed(ready_after: usize, log: &Arc<Log>) -> Box<Self> {
        Box::new(Self { ready_after, polls: 0, log: log.clone(), owned: vec![7; 64] })
    }
}

impl Drop for Countdown {
    fn drop(&mut self) {
        assert_eq!(self.owned.len(), 64);
        self.log.drops.fetch_add(1, Ordering::SeqCst);
    }
}

unsafe impl UnsafeWakeableTask for Countdown {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls += 1;
        if self.polls >= self.ready_after { Poll::Ready(()) } else { Poll::Pending }
    }

    unsafe fn wake(&self) {
        self.log.wakes.fetch_add(1, Ordering::SeqCst);
    }

    unsafe fn drop_in_place(&mut self) {
        self.log.cleanups.fetch_add(1, Ordering::SeqCst);
    }
}

derive_unsafe_wakeable_vtable!(Countdown, COUNTDOWN_VTABLE);

fn key(kind: &'static str, version: u16) -> VTableKey {
    VTableKey { segment: "tests", kind, version }
}

#[test]
fn ptr_calls_go_to_the_task() {
    let log = Arc::new(Log::default());
    let data = Box::into_raw(Countdown::boxed(3, &log));
    let mut ptr = UnsafeWakeablePtr { data: data as *mut (), vtable: &COUNTDOWN_VTABLE };

    unsafe {
        assert_eq!(ptr.poll(), Poll::Pending);
        assert_eq!(ptr.poll(), Poll::Pending);
        assert_eq!(ptr.poll(), Poll::Ready(()));
        ptr.wake();
        ptr.drop_in_place();
    }
    assert_eq!(Log::get(&log.wakes), 1);
    assert_eq!(Log::get(&log.cleanups), 1);
    assert_eq!(Log::get(&log.drops), 0, "the bare ptr does not own the task");

    drop(unsafe { Box::from_raw(data) });
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn registered_task_runs_and_frees_once() {
    let k = key("countdown", 1);
    GLOBAL_VTABLE_REGISTRY.register(k, &COUNTDOWN_VTABLE);
    let log = Arc::new(Log::default());

    let mut task = RegisteredTask::new(Countdown::boxed(2, &log), k);
    assert_eq!(task.key, k);
    unsafe {
        assert_eq!(task.poll(), Poll::Pending);
        task.wake();
        assert_eq!(task.poll(), Poll::Ready(()));
    }
    assert_eq!(Log::get(&log.drops), 0);

    drop(task);
    assert_eq!(Log::get(&log.drops), 1);
    assert_eq!(Log::get(&log.cleanups), 0, "Drop frees; it does not call the task's drop_in_place");
    assert_eq!(Arc::strong_count(&log), 1);
}

#[test]
fn drop_in_place_then_drop_frees_once() {
    let k = key("countdown", 2);
    GLOBAL_VTABLE_REGISTRY.register(k, &COUNTDOWN_VTABLE);
    let log = Arc::new(Log::default());

    let mut task = RegisteredTask::new(Countdown::boxed(1, &log), k);
    unsafe { task.drop_in_place() };
    assert_eq!(Log::get(&log.cleanups), 1);
    assert_eq!(Log::get(&log.drops), 0);

    drop(task);
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn many_tasks_each_freed() {
    let k = key("countdown", 3);
    GLOBAL_VTABLE_REGISTRY.register(k, &COUNTDOWN_VTABLE);
    let log = Arc::new(Log::default());

    let mut tasks: Vec<_> = (1..=20).map(|n| RegisteredTask::new(Countdown::boxed(n, &log), k)).collect();
    for task in &mut tasks {
        while unsafe { task.poll() }.is_pending() {}
    }
    drop(tasks);
    assert_eq!(Log::get(&log.drops), 20);
    assert_eq!(Arc::strong_count(&log), 1);
}

#[test]
#[should_panic(expected = "vtable for key not registered")]
fn unregistered_key_panics() {
    let log = Arc::new(Log::default());
    let _ = RegisteredTask::new(Countdown::boxed(1, &log), key("never-registered", 1));
}

#[test]
fn unregistered_key_panics_without_leaking_the_task() {
    let log = Arc::new(Log::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        RegisteredTask::new(Countdown::boxed(1, &log), key("never-registered", 2))
    }));
    assert!(result.is_err());
    assert_eq!(Log::get(&log.drops), 1);
}

#[test]
fn registry_get_register_and_replace() {
    let registry = VTableRegistry::new();
    let k = key("local", 1);
    assert!(registry.get(&k).is_none());

    registry.register(k, &COUNTDOWN_VTABLE);
    assert!(std::ptr::eq(registry.get(&k).unwrap(), &COUNTDOWN_VTABLE));
    assert!(registry.get(&key("local", 2)).is_none(), "the version is part of the key");

    static OTHER: waker_tables::UnsafeWakeableVTable = waker_tables::UnsafeWakeableVTable {
        poll_fn: |_| Poll::Ready(()),
        wake_fn: |_| {},
        drop_fn: |_| {},
    };
    registry.register(k, &OTHER);
    assert!(std::ptr::eq(registry.get(&k).unwrap(), &OTHER), "register replaces");
}

#[test]
fn keys_compare_hash_and_order_by_all_three_fields() {
    let a = key("a", 1);
    assert_eq!(a, key("a", 1));
    assert_ne!(a, key("a", 2));
    assert_ne!(a, key("b", 1));
    assert_ne!(a, VTableKey { segment: "other", kind: "a", version: 1 });

    let hashed: HashSet<_> = [a, key("a", 1), key("a", 2)].into_iter().collect();
    assert_eq!(hashed.len(), 2);

    let ordered: Vec<_> = [key("b", 1), key("a", 2), key("a", 1)].into_iter().collect::<BTreeSet<_>>().into_iter().collect();
    assert_eq!(ordered, vec![key("a", 1), key("a", 2), key("b", 1)]);
}

#[test]
fn debug_output() {
    assert_eq!(
        format!("{:?}", key("k", 3)),
        "VTableKey { segment: \"tests\", kind: \"k\", version: 3 }"
    );
    assert_eq!(format!("{:?}", COUNTDOWN_VTABLE), "UnsafeWakeableVTable { .. }");
}
