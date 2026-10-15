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
- `raw_waker_vtable`, if you override it, returns a `'static` copy of `T`'s own vtable (the default)

`derive_arc_unsafe_wakeable!(T, method)` keeps all of these for you. It calls `method` from a plain
(not `unsafe`) fn, so the method must be safe: an `unsafe fn` is refused at compile time (E0133).

`waker_from_arc` always uses `T`'s own vtable. To choose one per waker, as 0.1's
`waker_from_arc(arc, &VTABLE)` did, call `unsafe fn waker_from_arc_with_vtable(arc, &VTABLE)`. You
promise the vtable reads the data pointer as `T`'s `Arc` by the rules above, and that its clone
returns a `RawWaker` whose vtable does too. `declare_raw_waker_vtable!(T, NAME)` declares such a
vtable. `raw_waker_from_ptr_with_vtable(ptr, &VTABLE)` is safe, as 0.1's `raw_waker_from_ptr` was: it
only builds the `RawWaker`, and the same promise comes with the `unsafe` `Waker::from_raw` that turns
it into a `Waker`.

This is **only about ownership and refcount correctness**, not scheduling.

### 2) `UnsafeWakeableTask` (manual task vtable)

You promise that:

- the `data: *mut ()` matches the vtable you call it with
- you uphold any exclusivity invariants around polling (like you would with futures)

`RegisteredTask` takes the first promise off your hands: the vtable records its task type and
`RegisteredTask::new` refuses a key registered for another type. A bare `UnsafeWakeablePtr` does not
check; its methods are `unsafe` for that reason.

Two `unsafe` constructors do what 0.1 allowed: `UnsafeWakeableVTable::from_raw_parts::<T>(poll, wake, drop)`
builds a vtable by hand and records it as `T`'s, and `RegisteredTask::from_raw_parts(key, ptr)` wraps a
task over memory you own, with no type check. `RegisteredTask` never frees such a task, and `into_raw_parts` hands any
task's pointer back without freeing it; `owns_its_task()`, read before that call, says whether the pointer is a `Box<T>`
you must then free. Which memory a task may sit in is written once, under "The data pointer" in `UnsafeWakeablePtr`'s
docs.

This crate intentionally does not enforce a scheduler model — that’s your job.

---

## Crate overview

- `UnsafeWakeable` — wire your own type into `RawWakerVTable`
- `derive_arc_unsafe_wakeable!` — convenience macro for the `Arc<T>` case
- `waker_from_arc` — build a real `Waker` from `Arc<T>`; the vtable is `T`'s own (`T::raw_waker_vtable()`)
- `waker_from_arc_with_vtable` — `unsafe`: the same, with a vtable you choose
- `raw_waker_from_ptr_with_vtable` — a `RawWaker` on a vtable you choose; safe, and `Waker::from_raw` is the `unsafe` step
- `declare_raw_waker_vtable!` — a `'static` vtable for `T` whose clones stay on it (0.1's macro)

- `UnsafeWakeableTask` — “task object” contract: `poll/wake/drop_in_place`
- `derive_unsafe_wakeable_vtable!` — generate a `'static` function-pointer vtable
- `UnsafeWakeableVTable::from_raw_parts` — `unsafe`: build one by hand; `poll_fn()`, `wake_fn()`, `drop_fn()` read it
- `UnsafeWakeablePtr` — thin “fat pointer” pair (`data + &'static vtable`)
- `VTableRegistry` + `VTableKey` — segment/kind/version mapping to vtables
- `RegisteredTask` — owns a boxed task: checks its vtable's type, runs it, frees it on drop;
  `from_raw_parts` (`const unsafe`) and `into_raw_parts` for memory you own; `owns_its_task` tells the two apart

---

## Quick start

Add the crate:

```toml
[dependencies]
waker_tables = "0.4"
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
- `UnsafeWakeableVTable` is five function pointers: `poll/wake/drop`, plus `type_id/type_name`,
  which `RegisteredTask::new` uses for its type check.
- Everything is designed for **fast dispatch** with predictable branches.

```rust
use std::mem::size_of;
use waker_tables::{UnsafeWakeablePtr, UnsafeWakeableVTable};

assert_eq!(size_of::<UnsafeWakeablePtr>(), 2 * size_of::<usize>());
assert_eq!(size_of::<UnsafeWakeableVTable>(), 5 * size_of::<usize>());
```

If you benchmark, remember:
- cold runs can include I/O/page-fault noise
- pinning tasks/threads and stable CPU governor matters
- `mmap` workloads fluctuate depending on page cache state

---

## Common pitfalls

- **Mixing pointer + vtable**: calling the wrong vtable for the wrong type is UB.  
  Use `RegisteredTask::new`: it refuses a key whose vtable is for another type
  (`RegisteredTask::from_raw_parts` is `unsafe` and does not check). From safe code wakers cannot
  mix: `waker_from_arc` uses `T`'s own vtable, `waker_from_arc_with_vtable` is `unsafe`, and a
  `RawWaker` from `raw_waker_from_ptr_with_vtable` becomes a `Waker` only through the `unsafe`
  `Waker::from_raw`.

- **Freeing tasks**: a bare `UnsafeWakeablePtr` does not own its task; `drop_in_place` runs the task's
  cleanup hook but does not free it. `RegisteredTask` frees a task from `new` on drop, never one from
  `from_raw_parts`, and `owns_its_task()` says which a task is. Read it before `into_raw_parts`: if it
  was `true`, the `Box<T>` is now yours to free (the result is `#[must_use]`); if `false`, the memory
  belongs to whoever called `from_raw_parts`, and only they free it.

- **Polling concurrently**: if your task is not internally synchronized, you must ensure exclusive polling (just like futures).

- **RawWaker refcounts**: if you implement `UnsafeWakeable` manually, the *only* acceptable mental model is:  
  “every pointer is an `Arc<T>` raw pointer; clone increments; drop decrements.”

- **`will_wake`**: `Waker::will_wake` compares vtable addresses. A type wired with
  `derive_arc_unsafe_wakeable!` carries one `static` vtable, so a waker and its clones always
  `will_wake` each other. A manual impl that keeps the default `raw_waker_vtable` gets a reference to
  a constant, which an optimized build may copy, so there `will_wake` is best-effort and can be
  `false` between a waker and its own clone. A waker built on a `declare_raw_waker_vtable!` static
  keeps that static through every clone.

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

## 0.4.0
0.4.0 puts back five things 0.1.4 could do. Claude's earlier releases of this crate (0.2.0 and 0.3.0)
took them out, and none of them had to go. Each one whose 0.1.4 form let safe code cause Undefined
Behavior comes back as an `unsafe fn`, and its `# Safety` section says what the caller must guarantee;
the others come back safe, as they were. Safe code still cannot reach that Undefined Behavior: doctests
show the compiler refusing it. Every safe function and type of 0.3.1 is unchanged.

`declare_raw_waker_vtable!` is back to its 0.1.4 form, and that breaks code that built on 0.3.1 (two
uses in one module, or comparing the static with `T::raw_waker_vtable()`), so this is 0.4.0, not a 0.3
patch release: a `"0.3"` requirement does not pick it up. The same work was first prepared as 0.3.2; that
number went instead to the `wake_by_ref` fix below, published on the 0.3 line. Line numbers below are in the 0.4.0 tree.

- `derive_arc_unsafe_wakeable!`'s generated `wake_by_ref` names the trait (`<Self as UnsafeWakeable>::wake`
  and `::clone_ptr`). 0.3.1 wrote `Self::wake(Self::clone_ptr(ptr))`, which picks a type's own function
  first: a type with its own safe `clone_ptr` had it used in place of the trait's (a use-after-free under
  Miri with no `unsafe` in the user's code), and a wake method named `wake`, as 0.1.4 allowed, did not
  compile. Fixed for the 0.3 line in 0.3.2 too. `tests/inherent_names.rs` covers both.
- **A vtable chosen per waker.** 0.1.4 had `waker_from_arc(arc, &VTABLE)` and
  `raw_waker_from_ptr(ptr, &VTABLE)`, both safe; 0.2.0 dropped the vtable argument. Now
  `unsafe fn waker_from_arc_with_vtable(arc, &VTABLE)` and `fn raw_waker_from_ptr_with_vtable(ptr, &VTABLE)`
  (src/lib.rs lines 377–426 and 428–493). `waker_from_arc_with_vtable` is `unsafe` because in 0.1.4 safe
  code could pass `B`'s vtable with an `Arc<A>`, `waker_from_arc` turned that into a `Waker` itself,
  and `B`'s functions then read an `A` as a `B`. `raw_waker_from_ptr_with_vtable` stays safe, as it
  was: it only builds the `RawWaker`, and turning that into a `Waker` already takes the `unsafe`
  `Waker::from_raw`. Tests: tests/waker_with_vtable.rs.
- **`declare_raw_waker_vtable!` as in 0.1.4** (src/lib.rs lines 244–325). The static's clone returns a
  `RawWaker` on that same static, so a waker built on it keeps it through `Waker::clone` and `Context`
  clones, and `will_wake` holds between them. From 0.2.0 to 0.3.1 the static's clone moved the waker to
  the type's own vtable, a different static. The four helper fns `__waker_clone`, `__waker_wake`,
  `__waker_wake_by_ref` and `__waker_drop` are back at the call site with 0.1.4's names and signatures,
  so the macro can again be used only once per module (0.3.1 allowed more), and its static no longer
  compares equal to `T::raw_waker_vtable()`; that is why this is 0.4.0. Declaring it is safe; a waker on
  it needs `waker_from_arc_with_vtable` or `Waker::from_raw`, both `unsafe`. Tests:
  tests/declared_vtable.rs (line 46 is the clone test), tests/declared_vtable_helpers.rs,
  tests/release_version.rs.
- **Task vtables built by hand, and read.** 0.1.4's `UnsafeWakeableVTable` was a struct literal with `pub`
  fields `poll_fn`, `wake_fn`, `drop_fn`; 0.2.0 added type fields and 0.3.0 made all of them private.
  The fields stay private. `UnsafeWakeableVTable::from_raw_parts::<T>(poll_fn, wake_fn, drop_fn)` is a
  `const unsafe fn` that works in a `static` and records `T` as the vtable's type, and `poll_fn()`,
  `wake_fn()`, `drop_fn()` read the functions back (safe: calling them needs `unsafe`) (src/lib.rs lines
  584–677). It is `unsafe` because `RegisteredTask::new` trusts the recorded type: a vtable safe code
  built by hand could record `A` with `B`'s functions, and `new` would accept a `Box<A>` for it. With
  such a vtable the function that runs is the one passed in, so the `# Safety` sections of `poll`,
  `wake` and `drop_in_place` on `UnsafeWakeablePtr` and `RegisteredTask` name the contract of the
  vtable's function: `T`'s `UnsafeWakeableTask` method for a vtable from `of` or
  `derive_unsafe_wakeable_vtable!`, the hand-built function's own for one from `from_raw_parts`
  (src/lib.rs lines 732–775 and 1164–1203). Tests: tests/task_vtable_parts.rs,
  tests/safety_contracts.rs.
- **A `RegisteredTask` over memory you own, and its pointer taken back out.** In 0.1.4 `key` and `ptr`
  were `pub`, a task could be built as `RegisteredTask { key, ptr }` over any memory, also in a `const`,
  `ptr` could be moved out, and nothing was freed. 0.2.0 made the fields private and made
  `RegisteredTask` free its task. Now `const unsafe fn RegisteredTask::from_raw_parts(key, ptr)` builds
  one that `RegisteredTask`'s `Drop` never frees; it works in a `const`, and a `const` task fills a
  table as `[TASK; N]`. `fn into_raw_parts(self) -> (VTableKey, UnsafeWakeablePtr)` hands the pointer
  back without freeing it. Its result does not say who owns the memory, so `fn owns_its_task(&self) ->
  bool`, read before that call, does: `true` for a task made by `new`, whose `Box<T>` you then own and
  must free, `false` for one made by `from_raw_parts`, whose memory belongs to whoever called it. The docs of
  `into_raw_parts` and its `#[must_use]` text say to check it first (src/lib.rs lines 960–982 and
  1015–1151). `from_raw_parts` is `unsafe` because `poll`, `wake` and `drop_in_place` trust the pointer
  and vtable to match; with `pub` fields safe code could pair a task with another type's vtable. Its
  `# Safety` asks for a pointer valid for reads and writes as the type the vtable's functions read it
  as (from `Box::into_raw` or `&mut x as *mut _`, never derived from `&x`), since `poll` and
  `drop_in_place` write through it. `new` and its type check are unchanged. Tests:
  tests/registered_task_parts.rs, tests/owns_its_task.rs, tests/const_registered_task.rs,
  tests/safety_contracts.rs.
- **A task under the key of the type it starts with.** 0.1.4's `RegisteredTask::new` did not check
  the type, so a `#[repr(transparent)]` wrapper ran under its inner type's key, and a `#[repr(C)]` task
  whose first field is a header type ran under the header's key, several task types sharing the
  header's vtable. 0.2.0 added the check. `new` keeps it, and `from_raw_parts` above takes such tasks.
  The rule is written once, under "The data pointer" in `UnsafeWakeablePtr`'s docs (src/lib.rs lines
  703–723). The `# Safety` sections of `RegisteredTask::from_raw_parts`, of `poll` and `wake` on
  `UnsafeWakeablePtr` (`drop_in_place` is as `poll`), and of `UnsafeWakeableVTable::from_raw_parts` state
  it and link there: the memory is valid for reads and writes as the type the vtable's functions read
  it as.
  That memory may be that type itself, a `#[repr(transparent)]` wrapper of it, or a `#[repr(C)]` struct
  whose first field is that type. Two types that only share a size and alignment are not enough: an
  `A(u8)` holding 2, read as a `B(bool)`, is Undefined Behavior. Neither is a struct without
  `#[repr(C)]`, whose first field need not be at its start. Tests: tests/same_layout_key.rs,
  tests/repr_c_header.rs, tests/safety_contracts.rs.

One change since 0.1.4 stays: `derive_unsafe_wakeable_vtable!` calls the trait's `poll`, `wake` and
`drop_in_place`, never an inherent method of the same name. 0.1.4's macro wrote `(*ptr).poll()`, and
Rust picks an inherent `poll` over the trait's, so a type with both ran the inherent one through its
vtable. 0.2.0 fixed that; it is now documented on the macro, with a doctest (src/lib.rs lines 791–820).

## 0.3.1

- `derive_arc_unsafe_wakeable!` no longer lets safe code call an `unsafe fn`. 0.3.0 called your method
  inside the body of the `unsafe fn wake` it generated, so naming an `unsafe fn` method (your own, or
  one reached through `Deref`, such as `Option::unwrap_unchecked`) compiled with no `unsafe` anywhere
  in your code, and Miri reported Undefined Behavior. 0.3.1 calls the method from a plain fn, so that
  is a compile error (E0133). A safe method works exactly as before.
- A waker and its clones now carry one vtable address, so `will_wake` between them is `true` in
  release builds too. In 0.3.0 the vtable was a constant, an optimized build could hold two copies of
  it, and `cargo test --release` failed on `clone.will_wake(&waker)`. `UnsafeWakeable` has a new
  provided method, `raw_waker_vtable()`; `derive_arc_unsafe_wakeable!` overrides it with one `static`
  per type, and `waker_from_arc`, `raw_waker_from_ptr` and waker clones all use it. A manual impl
  keeps the default (0.3.0's constant), so for it `will_wake` stays best-effort. CI now also runs
  `cargo test --release`.
- `derive_arc_unsafe_wakeable!` now needs a sized type. 0.3.0 accepted an unsized one, but no
  waker could be built from it.
- The `block_on` example honors its tick count: `Ticks::start(n, ..)` returns `n`. In 0.3.0 its future
  checked a literal 3, so 2 hung forever and 4 returned 3.
- The README said `UnsafeWakeableVTable` is three function pointers; it is five, and a doctest now
  checks both sizes.

## 0.3.0

- `UnsafeWakeableVTable`'s fields are private: build it only with `UnsafeWakeableVTable::of::<T>()`
  or `derive_unsafe_wakeable_vtable!`. In 0.2 a vtable could be written by hand claiming one type while
  holding another type's functions, which `RegisteredTask::new`'s check could not catch. `type_name()`
  reads the name.

## 0.2.0

- `waker_from_arc(arc)` takes no vtable: it builds `T`'s own, so a waker can never run another
  type's functions. `raw_waker_from_ptr(ptr)` likewise. `declare_raw_waker_vtable!` still compiles
  and declares that same vtable.
- `UnsafeWakeableVTable` records its task type; build it with
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

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
