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

`RegisteredTask` takes the first promise off your hands: the vtable records its task type and
`RegisteredTask::new` refuses a key registered for another type. A bare `UnsafeWakeablePtr` does not
check; its methods are `unsafe` for that reason.

This crate intentionally does not enforce a scheduler model — that’s your job.

---

## Crate overview

- `UnsafeWakeable` — wire your own type into `RawWakerVTable`
- `derive_arc_unsafe_wakeable!` — convenience macro for the `Arc<T>` case
- `waker_from_arc` — build a real `Waker` from `Arc<T>`; the vtable is built from `T`
- `declare_raw_waker_vtable!` — a `'static` copy of that vtable (kept for 0.1 code)

- `UnsafeWakeableTask` — “task object” contract: `poll/wake/drop_in_place`
- `derive_unsafe_wakeable_vtable!` — generate a `'static` function-pointer vtable
- `UnsafeWakeablePtr` — thin “fat pointer” pair (`data + &'static vtable`)
- `VTableRegistry` + `VTableKey` — segment/kind/version mapping to vtables
- `RegisteredTask` — owns a boxed task: checks its vtable's type, runs it, frees it on drop

---

## Quick start

Add the crate:

```toml
[dependencies]
waker_tables = "0.2"
```

---

## Example 1: A `Waker` backed by your `Arc<T>` (RawWaker route)

This is the “I want `Waker` to call *my scheduler*” path.

```rust
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::task::Waker;

use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};

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

fn make_waker(task: Arc<MyTask>) -> Waker {
    waker_from_arc(task)
}

fn main() {
    let task = Arc::new(MyTask { wakes: AtomicUsize::new(0) });
    let w = make_waker(task.clone());

    // by-ref wake
    w.wake_by_ref();
    // consuming wake
    w.wake();

    assert_eq!(task.wakes.load(Ordering::Relaxed), 2);
    assert_eq!(Arc::strong_count(&task), 1);
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
        // The bare pointer does not own the task: free it yourself (RegisteredTask does this for you).
        drop(Box::from_raw(ptr.data as *mut CounterTask));
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

    let mut etl = RegisteredTask::new(Box::new(EtlTask), etl_key);
    let mut search = RegisteredTask::new(Box::new(SearchTask), search_key);

    unsafe {
        assert_eq!(etl.poll(), Poll::Ready(()));

        assert_eq!(search.poll(), Poll::Pending);
        search.wake();
        search.drop_in_place();
    }

    // A key registered for another type is refused.
    let wrong = std::panic::catch_unwind(|| RegisteredTask::new(Box::new(EtlTask), search_key));
    assert!(wrong.is_err());

    // Dropping a RegisteredTask frees its task.
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
            Poll::Ready(()) => unsafe {
                t.drop_in_place();
                drop(Box::from_raw(t.data as *mut TickTask));
            },
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
  Use `RegisteredTask`: it refuses a key whose vtable is for another type. Wakers cannot mix:
  `waker_from_arc` builds `T`'s own vtable.

- **Freeing tasks**: a bare `UnsafeWakeablePtr` does not own its task; `drop_in_place` runs the task's
  cleanup hook but does not free it. `RegisteredTask` frees on drop.

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

## 0.2.0

- `waker_from_arc(arc)` takes no vtable: it builds `T`'s own, so a waker can never run another
  type's functions. `raw_waker_from_ptr(ptr)` likewise. `declare_raw_waker_vtable!` still compiles
  and declares that same vtable.
- `UnsafeWakeableVTable` records its task type (`type_id`, `type_name`); build it with
  `UnsafeWakeableVTable::of::<T>()` or `derive_unsafe_wakeable_vtable!`.
- `RegisteredTask::new` panics on a key registered for another type, and `RegisteredTask` frees its
  task on drop (0.1 never freed it). Its fields are private: use `key()` and `ptr()`.
- `SmartWakerPtr` frees its ref on drop; `to_arc` and `to_cloned_arc` are safe.

## Examples

```bash
cargo run --example block_on     # a whole executor on waker_from_arc, woken by a timer thread
cargo run --example task_table   # a round-robin table of RegisteredTasks of two types
cargo run --example admission_executor  # five jobs, at most two in flight, gated by uh-tom-mick's AtomicAdmission
```

## Testing

```bash
cargo test
cargo +nightly miri test
MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-many-seeds=0..8" cargo +nightly miri test
```

Miri reports a leaked task or a reference counted wrong; the tests check every `Arc` count and every
task drop. The code blocks in this README run as doctests.

## License

MIT & Apache-2.0
