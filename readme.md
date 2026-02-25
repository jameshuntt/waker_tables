# waker_tables

A tiny crate for **manual vtables** and **RawWaker plumbing**.

`waker_tables` gives you two building blocks:

1. **`UnsafeWakeable` + `RawWakerVTable` helpers**  
   For when you want a `std::task::Waker` backed by your own type (usually `Arc<T>`) without fighting `RawWaker` boilerplate.

2. **`UnsafeWakeableTask` + `UnsafeWakeableVTable` + registry**  
   For when you want a *cheap*, *type-erased*, *manually-polled* “task object” with function pointers (`poll/wake/drop`) — perfect for custom schedulers, runtimes, staged pipelines, actor systems, or any “task graph” that is not `Future`-shaped.

This crate is intentionally low-level: it gives you **correct pointer/refcount wiring** and **fast type erasure**, while *you* own scheduling, queues, waking policy, and invariants.

---

## Why this exists

If you’re building anything like:

- a custom runtime / scheduler
- a stage graph / orchestration engine
- a task system that isn’t `Future` (or doesn’t want trait objects everywhere)
- a “fat pointer” task handle that can be passed across subsystems cheaply
- a registry keyed by protocol version (segment/kind/version)

…then you eventually want:

- stable function-pointer vtables
- raw-pointer tasks with explicit drop and wake routes
- ability to store tasks in intrusive queues / slabs / rings without dynamic dispatch costs

`waker_tables` is the “do it once, do it correctly” layer.

---

## Safety model (read this)

There are two “unsafe contracts” in this crate:

### 1) `UnsafeWakeable` (RawWaker contract)

You promise that the `*const Self` used by the vtable obeys **`Arc::into_raw` / `Arc::from_raw` refcount rules**.

- `clone_ptr` must produce a pointer owning +1 strong `Arc` ref
- `drop_ptr` must balance exactly one strong ref
- `wake` consumes the ref (like `Waker::wake`)
- `wake_by_ref` must not consume the original ref

This is **only about ownership and refcount correctness**, not scheduling.

### 2) `UnsafeWakeableTask` (manual task vtable)

You promise that:

- the `data: *mut ()` matches the vtable you call it with
- you uphold any exclusivity invariants around polling (like you would with futures)

This crate intentionally does not enforce a scheduler model — that’s your job.

---

## Crate overview

- `UnsafeWakeable` — wire your own type into `RawWakerVTable`
- `derive_arc_unsafe_wakeable!` — convenience macro for the `Arc<T>` case
- `declare_raw_waker_vtable!` — generate a `'static` `RawWakerVTable` for your type
- `waker_from_arc` — build a real `Waker` from `Arc<T>` + vtable

- `UnsafeWakeableTask` — “task object” contract: `poll/wake/drop_in_place`
- `derive_unsafe_wakeable_vtable!` — generate a `'static` function-pointer vtable
- `UnsafeWakeablePtr` — thin “fat pointer” pair (`data + &'static vtable`)
- `VTableRegistry` + `VTableKey` — segment/kind/version mapping to vtables
- `RegisteredTask` — safer wrapper coupling a `VTableKey` with a task pointer

---

## Quick start

Add the crate:

```text
[dependencies]
waker_tables = "0.x"
```

---

## Example 1: A `Waker` backed by your `Arc<T>` (RawWaker route)

This is the “I want `Waker` to call *my scheduler*” path.

```rust
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::task::Waker;

use waker_tables::{
    UnsafeWakeable,
    derive_arc_unsafe_wakeable,
    declare_raw_waker_vtable,
    waker_from_arc,
};

struct MyTask {
    wakes: AtomicUsize,
}

impl MyTask {
    pub fn on_wake(self: Arc<Self>) {
        // This is where you'd enqueue into a run queue / notify an event loop / etc.
        self.wakes.fetch_add(1, Ordering::Relaxed);
    }
}

// Wire Arc<MyTask> into the UnsafeWakeable contract.
derive_arc_unsafe_wakeable!(MyTask, on_wake);

// Generate the vtable once.
declare_raw_waker_vtable!(MyTask, MYTASK_RAW_VTABLE);

fn make_waker(task: Arc<MyTask>) -> Waker {
    waker_from_arc(task, &MYTASK_RAW_VTABLE)
}

fn main() {
    let task = Arc::new(MyTask { wakes: AtomicUsize::new(0) });
    let w = make_waker(task.clone());

    // by-ref wake
    w.wake_by_ref();
    // consuming wake
    w.wake();

    assert!(task.wakes.load(Ordering::Relaxed) >= 2);
}
```

**When to use this:** you have a scheduler that is Waker-driven (polling futures, bridging into async ecosystems, etc.).

---

## Example 2: A manual “task object” with a function-pointer vtable

This is the “I want a fast erased task pointer” path.

```rust
use std::task::Poll;
use std::sync::atomic::{AtomicUsize, Ordering};

use waker_tables::{
    UnsafeWakeableTask,
    UnsafeWakeablePtr,
    derive_unsafe_wakeable_vtable,
};

struct CounterTask {
    polls: AtomicUsize,
}

unsafe impl UnsafeWakeableTask for CounterTask {
    unsafe fn poll(&mut self) -> Poll<()> {
        self.polls.fetch_add(1, Ordering::Relaxed);
        Poll::Pending
    }

    unsafe fn wake(&self) {
        // In a real scheduler: enqueue / notify / etc.
    }

    unsafe fn drop_in_place(&mut self) {
        // If you had manual resources, you'd clean them here.
    }
}

// Generate a static vtable for the type.
derive_unsafe_wakeable_vtable!(CounterTask, COUNTER_VTABLE);

fn main() {
    let boxed = Box::new(CounterTask { polls: AtomicUsize::new(0) });

    // Erase the type behind (data ptr + vtable).
    let mut ptr = UnsafeWakeablePtr {
        data: Box::into_raw(boxed) as *mut (),
        vtable: &COUNTER_VTABLE,
    };

    unsafe {
        let _ = ptr.poll();
        ptr.wake();
        ptr.drop_in_place();
    }
}
```

**When to use this:** you’re building your own runtime that does not want `dyn Future` overhead, or you want “task handles” that are cheap to pass around.

---

## Example 3: Register task vtables by protocol key (segment/kind/version)

This gives you a “dispatch table” for heterogeneous tasks across subsystems.

```rust
use std::task::Poll;

use waker_tables::{
    VTableKey,
    GLOBAL_VTABLE_REGISTRY,
    RegisteredTask,
    UnsafeWakeableTask,
    derive_unsafe_wakeable_vtable,
};

struct EtlTask;
struct SearchTask;

unsafe impl UnsafeWakeableTask for EtlTask {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}

unsafe impl UnsafeWakeableTask for SearchTask {
    unsafe fn poll(&mut self) -> Poll<()> { Poll::Pending }
    unsafe fn wake(&self) {}
    unsafe fn drop_in_place(&mut self) {}
}

derive_unsafe_wakeable_vtable!(EtlTask, ETL_VTABLE);
derive_unsafe_wakeable_vtable!(SearchTask, SEARCH_VTABLE);

fn main() {
    let etl_key = VTableKey { segment: "etl", kind: "parse", version: 1 };
    let search_key = VTableKey { segment: "search", kind: "merge", version: 1 };

    GLOBAL_VTABLE_REGISTRY.register(etl_key, &ETL_VTABLE);
    GLOBAL_VTABLE_REGISTRY.register(search_key, &SEARCH_VTABLE);

    let etl = RegisteredTask::new(Box::new(EtlTask), etl_key);
    let mut search = RegisteredTask::new(Box::new(SearchTask), search_key);

    unsafe {
        // etl: ready
        let _ = etl.ptr.vtable.poll_fn(etl.ptr.data);

        // search: pending
        let _ = search.poll();
        search.wake();
        search.drop_in_place();
    }
}
```

**When to use this:** you have multiple “job families” and want versioned rollout / protocol evolution without generic soup.

---

## Example 4: A tiny scheduler sketch (single-thread “run queue”)

This shows how `UnsafeWakeableTask` can drive a basic scheduler. It’s intentionally minimal.

```rust
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
```

---

## Performance notes

- `UnsafeWakeablePtr` is basically a manually-managed fat pointer: **two machine words**.
- `UnsafeWakeableVTable` is a set of three function pointers: `poll/wake/drop`.
- Everything is designed for **fast dispatch** with predictable branches.

If you benchmark, remember:
- cold runs can include I/O/page-fault noise
- pinning tasks/threads and stable CPU governor matters
- `mmap` workloads fluctuate depending on page cache state

---

## Common pitfalls

- **Mixing pointer + vtable**: calling the wrong vtable for the wrong type is UB.  
  Use `RegisteredTask` to couple `VTableKey` with the pointer.

- **Polling concurrently**: if your task is not internally synchronized, you must ensure exclusive polling (just like futures).

- **RawWaker refcounts**: if you implement `UnsafeWakeable` manually, the *only* acceptable mental model is:  
  “every pointer is an `Arc<T>` raw pointer; clone increments; drop decrements.”

---

## When you should NOT use this crate

If you are perfectly happy with:
- `Box<dyn Future<Output = ()>>`
- `tokio::spawn`
- `futures::task::ArcWake`
- normal `dyn Trait` object dispatch

…then you probably don’t need `waker_tables`.

This crate is for people who want to own their runtime boundaries.

---

## License

MIT / Apache-2.0 (pick whatever your repo uses)