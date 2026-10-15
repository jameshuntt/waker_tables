// waker_tables/src/lib.rs

//! Custom raw waker logic: hand-built vtables for wakers and for tasks.
//!
//! Two halves:
//!
//! * **Wakers.** Put [`UnsafeWakeable`] on an `Arc`-backed type (usually with
//!   [`derive_arc_unsafe_wakeable!`]) and [`waker_from_arc`] turns an `Arc<T>` into a
//!   [`Waker`]. The `RawWakerVTable` is `T`'s own ([`UnsafeWakeable::raw_waker_vtable`]),
//!   so from safe code a waker can only ever run its own type's functions.
//!   [`waker_from_arc_with_vtable`] (`unsafe`) takes a vtable you choose, such as one
//!   declared with [`declare_raw_waker_vtable!`].
//! * **Tasks.** Put [`UnsafeWakeableTask`] on a task type, declare its vtable with
//!   [`derive_unsafe_wakeable_vtable!`], register it under a [`VTableKey`], and
//!   [`RegisteredTask`] owns the boxed task, runs it through the vtable, and frees it
//!   when dropped. The vtable records its task type, and `RegisteredTask::new` refuses a
//!   key whose vtable belongs to another type. The `unsafe` constructors
//!   [`UnsafeWakeableVTable::from_raw_parts`] and [`RegisteredTask::from_raw_parts`] build
//!   a vtable by hand and a task over memory you own.
//!
//! ```
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};
//!
//! struct Signal { wakes: AtomicUsize }
//! impl Signal {
//!     fn on_wake(self: Arc<Self>) { self.wakes.fetch_add(1, Ordering::SeqCst); }
//! }
//! derive_arc_unsafe_wakeable!(Signal, on_wake);
//!
//! let signal = Arc::new(Signal { wakes: AtomicUsize::new(0) });
//! let waker = waker_from_arc(signal.clone());
//! waker.wake_by_ref();
//! waker.wake();
//! assert_eq!(signal.wakes.load(Ordering::SeqCst), 2);
//! assert_eq!(Arc::strong_count(&signal), 1);
//! ```

#![deny(missing_docs, unsafe_op_in_unsafe_fn)]

use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::sync::{Arc, PoisonError, RwLock};
use std::task::{Poll, RawWaker, RawWakerVTable, Waker};

use std::sync::LazyLock;

/* ================================
   1. RawWaker / UnsafeWakeable
   ================================ */

/// # Safety
///
/// Implementors promise that:
/// - The `ptr: *const Self` passed to these fns was created via `Arc<Self>`
///   and follows the `Arc::into_raw / Arc::from_raw` conventions.
/// - `clone_ptr` returns a new `*const Self` owning +1 strong `Arc` ref.
/// - `drop_ptr` balances exactly one `clone_ptr` / `Arc::into_raw` and
///   must never be called twice for the same pointer.
/// - `raw_waker_vtable`, if overridden, returns a `'static` copy of its default: the vtable
///   whose functions read `data` as `*const Self`.
///
/// This is **purely about refcount and memory ownership**, not scheduling.
/// Scheduling logic (enqueueing, routing, etc.) lives in your own types.
pub unsafe trait UnsafeWakeable: Send + Sync + 'static {
    /// Wake and give up the ref `ptr` owns.
    ///
    /// # Safety
    /// `ptr` owns one strong ref of an `Arc<Self>`; it is not used again.
    unsafe fn wake(ptr: *const Self);

    /// Wake without giving up the ref `ptr` owns.
    ///
    /// # Safety
    /// `ptr` points at a live `Arc<Self>` whose ref the caller keeps.
    unsafe fn wake_by_ref(ptr: *const Self);

    /// Take one more strong ref and return a pointer owning it.
    ///
    /// # Safety
    /// `ptr` points at a live `Arc<Self>`.
    unsafe fn clone_ptr(ptr: *const Self) -> *const Self;

    /// Give up the ref `ptr` owns.
    ///
    /// # Safety
    /// `ptr` owns one strong ref of an `Arc<Self>`; it is not used again.
    unsafe fn drop_ptr(ptr: *const Self);

    /// The `RawWakerVTable` every waker of `Self` carries: [`waker_from_arc`],
    /// [`raw_waker_from_ptr`] and every clone of such a waker take it from here.
    ///
    /// `Waker::will_wake` compares vtable addresses. [`derive_arc_unsafe_wakeable!`] overrides
    /// this with one `static` per type, so a waker and its clones always `will_wake` each other.
    /// The default returns a reference to a constant, and an optimized build may hold more than
    /// one copy of it, so for an impl that keeps the default `will_wake` is best-effort: a waker
    /// and its own clone can report `false`.
    fn raw_waker_vtable() -> &'static RawWakerVTable
    where
        Self: Sized,
    {
        &RawWakerVTableOf::<Self>::VTABLE
    }
}

/// Helper macro for the common `Arc<T>`-backed case.
///
/// You implement your *behavior* on `Arc<T>` and this macro wires
/// the raw pointer plumbing correctly.
///
/// The method is called on an `Arc<T>` from a plain (not `unsafe`) fn, so it must be a safe
/// method: an `unsafe fn` is refused with E0133 (examples below). Every waker of `T` carries
/// one `static` vtable, so a waker and its clones always `will_wake` each other.
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};
///
/// struct SmartPinTask { wakes: AtomicUsize }
///
/// impl SmartPinTask {
///     pub fn on_wake(self: Arc<Self>) {
///         self.wakes.fetch_add(1, Ordering::SeqCst);
///     }
/// }
///
/// derive_arc_unsafe_wakeable!(SmartPinTask, on_wake);
///
/// let task = Arc::new(SmartPinTask { wakes: AtomicUsize::new(0) });
/// waker_from_arc(task.clone()).wake();
/// assert_eq!(task.wakes.load(Ordering::SeqCst), 1);
/// ```
///
/// An `unsafe fn` method is refused:
///
/// ```compile_fail,E0133
/// use std::sync::Arc;
/// use waker_tables::derive_arc_unsafe_wakeable;
///
/// struct OnExecutorThread;
/// impl OnExecutorThread {
///     /// # Safety
///     /// Only on the executor thread.
///     unsafe fn on_wake(self: Arc<Self>) {}
/// }
/// derive_arc_unsafe_wakeable!(OnExecutorThread, on_wake);
/// ```
///
/// ```compile_fail,E0133
/// use std::ops::Deref;
/// use std::sync::Arc;
/// use waker_tables::{derive_arc_unsafe_wakeable, waker_from_arc};
///
/// struct Empty;
/// impl Deref for Empty {
///     type Target = Option<u8>;
///     fn deref(&self) -> &Option<u8> { &None }
/// }
/// derive_arc_unsafe_wakeable!(Empty, unwrap_unchecked);
/// waker_from_arc(Arc::new(Empty)).wake();
/// ```
#[macro_export]
macro_rules! derive_arc_unsafe_wakeable {
    ($t:ty, $wake_method:ident) => {
        // SAFETY: every fn below moves exactly the one strong ref its contract names, the user's
        // method runs only inside the safe fn `call` (so an `unsafe fn` method is refused), and
        // `raw_waker_vtable` returns a static holding `$t`'s own vtable.
        unsafe impl $crate::UnsafeWakeable for $t {
            unsafe fn wake(ptr: *const Self) {
                // Not `unsafe`: naming an `unsafe fn` method fails here with E0133.
                fn call(arc: ::std::sync::Arc<$t>) {
                    arc.$wake_method();
                }
                // SAFETY: `ptr` owns one strong ref (trait contract); it is consumed here.
                call(unsafe { ::std::sync::Arc::from_raw(ptr) });
            }

            unsafe fn wake_by_ref(ptr: *const Self) {
                // SAFETY: `ptr` points at a live Arc (trait contract); `clone_ptr` takes one more
                // ref and `wake` consumes that one, so the caller's ref is untouched.
                unsafe { <Self as $crate::UnsafeWakeable>::wake(<Self as $crate::UnsafeWakeable>::clone_ptr(ptr)) }
            }

            unsafe fn clone_ptr(ptr: *const Self) -> *const Self {
                // SAFETY: `ptr` points at a live Arc; the count goes up by exactly one.
                unsafe { ::std::sync::Arc::increment_strong_count(ptr) };
                ptr
            }

            unsafe fn drop_ptr(ptr: *const Self) {
                // SAFETY: `ptr` owns one strong ref (trait contract); it is released here.
                unsafe { ::std::sync::Arc::decrement_strong_count(ptr) };
            }

            fn raw_waker_vtable() -> &'static ::std::task::RawWakerVTable {
                // One static per type: every waker of `$t` carries this one address.
                static VTABLE: ::std::task::RawWakerVTable = $crate::RawWakerVTableOf::<$t>::VTABLE;
                &VTABLE
            }
        }
    };
}

/// The `RawWakerVTable` of `T`, built from `T`'s [`UnsafeWakeable`] functions.
#[doc(hidden)]
pub struct RawWakerVTableOf<T>(PhantomData<T>);

impl<T: UnsafeWakeable> RawWakerVTableOf<T> {
    /// The vtable; its fns read `data` as `*const T`.
    pub const VTABLE: RawWakerVTable =
        RawWakerVTable::new(raw_clone::<T>, raw_wake::<T>, raw_wake_by_ref::<T>, raw_drop::<T>);
}

// SAFETY (all four): a RawWaker built by this crate carries `*const T` from `Arc::into_raw`
// alongside `T::raw_waker_vtable()`, which holds these four functions for `T` (trait contract),
// never another type's.
unsafe fn raw_clone<T: UnsafeWakeable>(data: *const ()) -> RawWaker {
    // SAFETY: see above; the new ref belongs to the returned RawWaker.
    let ptr = unsafe { T::clone_ptr(data as *const T) };
    RawWaker::new(ptr as *const (), T::raw_waker_vtable())
}

unsafe fn raw_wake<T: UnsafeWakeable>(data: *const ()) {
    // SAFETY: see above; the Waker gives up its ref.
    unsafe { T::wake(data as *const T) }
}

unsafe fn raw_wake_by_ref<T: UnsafeWakeable>(data: *const ()) {
    // SAFETY: see above; the Waker keeps its ref.
    unsafe { T::wake_by_ref(data as *const T) }
}

unsafe fn raw_drop<T: UnsafeWakeable>(data: *const ()) {
    // SAFETY: see above; the Waker gives up its ref.
    unsafe { T::drop_ptr(data as *const T) }
}

/// Declare a `'static` `RawWakerVTable` for a specific `UnsafeWakeable` type.
///
/// This gives you a `'static` vtable that you can reuse everywhere, as in 0.1: build a waker on
/// it with [`waker_from_arc_with_vtable`] (0.1's `waker_from_arc(arc, &NAME)`). The static's clone
/// function returns a `RawWaker` that points at this same static, so a waker built on it keeps it
/// through every clone and `will_wake` holds between them.
///
/// The four functions in the static are left at the call site, with 0.1's names and signatures:
/// `unsafe fn __waker_clone(data: *const ()) -> RawWaker`, `unsafe fn __waker_wake(data: *const ())`,
/// `unsafe fn __waker_wake_by_ref(data: *const ())` and `unsafe fn __waker_drop(data: *const ())`.
/// Each reads `data` as the `*const T` of an `Arc<T>` and runs `T`'s [`UnsafeWakeable`] function of
/// the same name (`clone_ptr`, `wake`, `wake_by_ref`, `drop_ptr`). Because the names are fixed, the
/// macro can be used once per module.
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use std::task::{Context, Waker};
/// use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc_with_vtable};
///
/// struct SmartPinTask { wakes: AtomicUsize }
/// impl SmartPinTask {
///     fn on_wake(self: Arc<Self>) { self.wakes.fetch_add(1, Ordering::SeqCst); }
/// }
/// derive_arc_unsafe_wakeable!(SmartPinTask, on_wake);
///
/// declare_raw_waker_vtable!(SmartPinTask, SMARTPIN_TASK_RAW_VTABLE);
///
/// fn make_waker(task: Arc<SmartPinTask>) -> Waker {
///     // SAFETY: the declared static's functions read `data` as an `Arc<SmartPinTask>` under its
///     // `UnsafeWakeable` rules, and its clone returns a RawWaker on the same static.
///     unsafe { waker_from_arc_with_vtable(task, &SMARTPIN_TASK_RAW_VTABLE) }
/// }
///
/// let task = Arc::new(SmartPinTask { wakes: AtomicUsize::new(0) });
/// let waker = make_waker(task.clone());
/// let clone = Context::from_waker(&waker).waker().clone();
/// assert!(std::ptr::eq(clone.vtable(), &SMARTPIN_TASK_RAW_VTABLE));
/// assert!(clone.will_wake(&waker));
/// clone.wake();
/// waker.wake();
/// assert_eq!(task.wakes.load(Ordering::SeqCst), 2);
/// assert_eq!(Arc::strong_count(&task), 1);
/// ```
#[macro_export]
macro_rules! declare_raw_waker_vtable {
    ($t:ty, $name:ident) => {
        unsafe fn __waker_clone(data: *const ()) -> ::std::task::RawWaker {
            let ptr = data as *const $t;
            // SAFETY: a waker on `$name` carries the `*const $t` of a live `Arc<$t>` (the contract
            // of `waker_from_arc_with_vtable` / `Waker::from_raw`); `clone_ptr` returns a pointer
            // owning one more strong ref, which the returned RawWaker takes, on this same static.
            let new_ptr = unsafe { <$t as $crate::UnsafeWakeable>::clone_ptr(ptr) };
            ::std::task::RawWaker::new(new_ptr as *const (), &$name)
        }

        unsafe fn __waker_wake(data: *const ()) {
            let ptr = data as *const $t;
            // SAFETY: `ptr` owns one strong ref of an `Arc<$t>` (as above); the waker gives it up.
            unsafe { <$t as $crate::UnsafeWakeable>::wake(ptr) };
        }

        unsafe fn __waker_wake_by_ref(data: *const ()) {
            let ptr = data as *const $t;
            // SAFETY: `ptr` points at a live `Arc<$t>` (as above); the waker keeps its ref.
            unsafe { <$t as $crate::UnsafeWakeable>::wake_by_ref(ptr) };
        }

        unsafe fn __waker_drop(data: *const ()) {
            let ptr = data as *const $t;
            // SAFETY: `ptr` owns one strong ref of an `Arc<$t>` (as above); the waker gives it up.
            unsafe { <$t as $crate::UnsafeWakeable>::drop_ptr(ptr) };
        }

        pub static $name: ::std::task::RawWakerVTable = ::std::task::RawWakerVTable::new(
            __waker_clone,
            __waker_wake,
            __waker_wake_by_ref,
            __waker_drop,
        );
    };
}

/// An `Arc<T>` held as a raw pointer, freed when dropped.
#[repr(transparent)]
pub struct SmartWakerPtr<T: UnsafeWakeable>(*const T);

impl<T: UnsafeWakeable> SmartWakerPtr<T> {
    /// Hold `arc`'s strong ref as a raw pointer.
    pub fn from_arc(arc: Arc<T>) -> Self {
        Self(Arc::into_raw(arc))
    }

    /// Give the ref back as an `Arc`.
    pub fn to_arc(self) -> Arc<T> {
        let this = ManuallyDrop::new(self);
        // SAFETY: the pointer came from `Arc::into_raw` in `from_arc` (the field is private),
        // and ManuallyDrop keeps `Drop` from releasing the same ref again.
        unsafe { Arc::from_raw(this.0) }
    }

    /// Clone the underlying `Arc` without giving up this one.
    pub fn to_cloned_arc(&self) -> Arc<T> {
        // SAFETY: the pointer came from `Arc::into_raw` in `from_arc` and this ref is still held.
        unsafe {
            Arc::increment_strong_count(self.0);
            Arc::from_raw(self.0)
        }
    }
}

impl<T: UnsafeWakeable> Drop for SmartWakerPtr<T> {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `Arc::into_raw` in `from_arc` and owns one strong ref.
        unsafe { Arc::decrement_strong_count(self.0) }
    }
}

/// Build a `RawWaker` for `T` from a raw `Arc` pointer.
///
/// The `RawWaker` is inert until `Waker::from_raw`, which is where `ptr` must own a strong
/// ref of an `Arc<T>`.
pub fn raw_waker_from_ptr<T: UnsafeWakeable>(ptr: *const T) -> RawWaker {
    RawWaker::new(ptr as *const (), T::raw_waker_vtable())
}

/// Build a `Waker` from an `Arc<T>`; the waker owns that strong ref.
pub fn waker_from_arc<T: UnsafeWakeable>(arc: Arc<T>) -> Waker {
    let ptr = Arc::into_raw(arc);
    // SAFETY: `ptr` owns one strong ref of an `Arc<T>` and the vtable is `T`'s own.
    unsafe { Waker::from_raw(raw_waker_from_ptr(ptr)) }
}

/// Build a `RawWaker` for `T` from a raw `Arc` pointer and a vtable you choose (0.1's
/// `raw_waker_from_ptr(ptr, &VTABLE)`, safe as it was there).
///
/// It is safe because the `RawWaker` is inert: std's `RawWaker::new`, which builds it, is safe too.
///
/// # Turning the result into a `Waker`
///
/// That takes `Waker::from_raw`, which is `unsafe`. Its caller must guarantee that `ptr` owns one
/// strong ref of an `Arc<T>` and that `vtable` meets the contract in the `# Safety` section of
/// [`waker_from_arc_with_vtable`], which does both steps in one call.
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use std::task::Waker;
/// use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, raw_waker_from_ptr_with_vtable};
///
/// struct Signal { wakes: AtomicUsize }
/// impl Signal {
///     fn on_wake(self: Arc<Self>) { self.wakes.fetch_add(1, Ordering::SeqCst); }
/// }
/// derive_arc_unsafe_wakeable!(Signal, on_wake);
/// declare_raw_waker_vtable!(Signal, SIGNAL_VTABLE);
///
/// let signal = Arc::new(Signal { wakes: AtomicUsize::new(0) });
/// // No `unsafe` here, as in 0.1.
/// let raw = raw_waker_from_ptr_with_vtable(Arc::into_raw(signal.clone()), &SIGNAL_VTABLE);
/// // SAFETY: `raw` owns one strong ref of an Arc<Signal>, and SIGNAL_VTABLE is declared for Signal.
/// let waker = unsafe { Waker::from_raw(raw) };
/// assert!(std::ptr::eq(waker.vtable(), &SIGNAL_VTABLE));
/// waker.wake();
/// assert_eq!(signal.wakes.load(Ordering::SeqCst), 1);
/// assert_eq!(Arc::strong_count(&signal), 1);
/// ```
///
/// [`raw_waker_from_ptr`] takes no vtable:
///
/// ```compile_fail,E0061
/// # use std::sync::Arc;
/// # use waker_tables::{derive_arc_unsafe_wakeable, raw_waker_from_ptr, UnsafeWakeable};
/// # struct A; impl A { fn on_wake(self: Arc<Self>) {} } derive_arc_unsafe_wakeable!(A, on_wake);
/// # struct B; impl B { fn on_wake(self: Arc<Self>) {} } derive_arc_unsafe_wakeable!(B, on_wake);
/// let raw = raw_waker_from_ptr(Arc::into_raw(Arc::new(A)), B::raw_waker_vtable());
/// ```
pub fn raw_waker_from_ptr_with_vtable<T: UnsafeWakeable>(
    ptr: *const T,
    vtable: &'static RawWakerVTable,
) -> RawWaker {
    RawWaker::new(ptr as *const (), vtable)
}

/// Build a `Waker` from an `Arc<T>` and a vtable you choose (0.1's `waker_from_arc(arc, &VTABLE)`);
/// the waker owns that strong ref.
///
/// # Safety
///
/// `vtable` must be sound for a data pointer that is the `*const T` of an `Arc<T>`, under
/// [`UnsafeWakeable`]'s refcount rules:
/// - `clone` takes one more strong ref and returns a `RawWaker` owning it, whose vtable meets this
///   same contract;
/// - `wake` and `drop` give up the one strong ref the data pointer owns; `wake_by_ref` keeps it;
/// - each function may be called from any thread, since `Waker` is `Send + Sync`.
///
/// `T::raw_waker_vtable()` meets it, and so does a static declared with
/// [`declare_raw_waker_vtable!`] for `T`.
///
/// ```
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc_with_vtable};
///
/// struct Signal { wakes: AtomicUsize }
/// impl Signal {
///     fn on_wake(self: Arc<Self>) { self.wakes.fetch_add(1, Ordering::SeqCst); }
/// }
/// derive_arc_unsafe_wakeable!(Signal, on_wake);
/// declare_raw_waker_vtable!(Signal, SIGNAL_VTABLE);
///
/// let signal = Arc::new(Signal { wakes: AtomicUsize::new(0) });
/// // SAFETY: SIGNAL_VTABLE is declared for Signal.
/// let waker = unsafe { waker_from_arc_with_vtable(signal.clone(), &SIGNAL_VTABLE) };
/// assert!(std::ptr::eq(waker.vtable(), &SIGNAL_VTABLE));
/// waker.wake();
/// assert_eq!(signal.wakes.load(Ordering::SeqCst), 1);
/// assert_eq!(Arc::strong_count(&signal), 1);
/// ```
///
/// Safe code cannot pass a vtable, so it cannot hand `A`'s `Arc` to `B`'s functions (in 0.1 it
/// could, which was Undefined Behavior): calling this outside `unsafe` is refused, and
/// [`waker_from_arc`] takes no vtable.
///
/// ```compile_fail,E0133
/// # use std::sync::Arc;
/// # use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc_with_vtable};
/// # struct A; impl A { fn on_wake(self: Arc<Self>) {} } derive_arc_unsafe_wakeable!(A, on_wake);
/// # struct B(u64); impl B { fn on_wake(self: Arc<Self>) {} } derive_arc_unsafe_wakeable!(B, on_wake);
/// declare_raw_waker_vtable!(B, B_VTABLE);
/// waker_from_arc_with_vtable(Arc::new(A), &B_VTABLE).wake();
/// ```
///
/// ```compile_fail,E0061
/// # use std::sync::Arc;
/// # use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable, waker_from_arc};
/// # struct A; impl A { fn on_wake(self: Arc<Self>) {} } derive_arc_unsafe_wakeable!(A, on_wake);
/// # struct B(u64); impl B { fn on_wake(self: Arc<Self>) {} } derive_arc_unsafe_wakeable!(B, on_wake);
/// declare_raw_waker_vtable!(B, B_VTABLE);
/// waker_from_arc(Arc::new(A), &B_VTABLE).wake();
/// ```
pub unsafe fn waker_from_arc_with_vtable<T: UnsafeWakeable>(
    arc: Arc<T>,
    vtable: &'static RawWakerVTable,
) -> Waker {
    let raw = raw_waker_from_ptr_with_vtable(Arc::into_raw(arc), vtable);
    // SAFETY: `raw`'s data pointer owns one strong ref of an `Arc<T>`, and `vtable` reads it as one
    // under `UnsafeWakeable`'s rules (the caller's contract above).
    unsafe { Waker::from_raw(raw) }
}

/* ================================
   2. Task vtable (poll/wake/drop)
   ================================ */

/// Trait describing an object that can be polled + woken + dropped
/// via a manual vtable.
///
/// This is your **abstract task**, not necessarily a `Future`.
///
/// # Safety
/// The three functions are called through [`UnsafeWakeableVTable`] on a pointer to a live
/// `Self`; `poll` and `drop_in_place` with exclusive access. What each function may assume
/// beyond that is the implementor's own contract, and callers of [`RegisteredTask`]'s
/// `unsafe` methods uphold it.
pub unsafe trait UnsafeWakeableTask {
    /// Advance the task.
    ///
    /// # Safety
    /// The implementor's contract.
    unsafe fn poll(&mut self) -> Poll<()>;

    /// Signal the task.
    ///
    /// # Safety
    /// The implementor's contract.
    unsafe fn wake(&self);

    /// The task's own cleanup hook. It does not free the task; [`RegisteredTask`] frees it
    /// when dropped.
    ///
    /// # Safety
    /// The implementor's contract.
    unsafe fn drop_in_place(&mut self);
}

/// Function-pointer vtable for `UnsafeWakeableTask`.
///
/// Build it with [`UnsafeWakeableVTable::of`] (or [`derive_unsafe_wakeable_vtable!`]): it
/// records the task type, and [`RegisteredTask::new`] checks it. To build one by hand from your
/// own three functions, use the `unsafe` [`UnsafeWakeableVTable::from_raw_parts`]; read the
/// functions back with [`poll_fn`](Self::poll_fn), [`wake_fn`](Self::wake_fn) and
/// [`drop_fn`](Self::drop_fn). The fields are private, so safe code cannot make a vtable claim
/// one type while holding another type's functions:
///
/// ```compile_fail,E0451
/// # use waker_tables::UnsafeWakeableVTable;
/// struct A;
/// struct B;
/// # unsafe impl waker_tables::UnsafeWakeableTask for A { unsafe fn poll(&mut self) -> std::task::Poll<()> { std::task::Poll::Ready(()) } unsafe fn wake(&self) {} unsafe fn drop_in_place(&mut self) {} }
/// # unsafe impl waker_tables::UnsafeWakeableTask for B { unsafe fn poll(&mut self) -> std::task::Poll<()> { std::task::Poll::Ready(()) } unsafe fn wake(&self) {} unsafe fn drop_in_place(&mut self) {} }
/// fn a_type_id() -> std::any::TypeId { std::any::TypeId::of::<A>() }
/// // B's functions under A's type: refused, the fields are private.
/// static LIAR: UnsafeWakeableVTable = UnsafeWakeableVTable {
///     type_id: a_type_id,
///     ..UnsafeWakeableVTable::of::<B>()
/// };
/// ```
pub struct UnsafeWakeableVTable {
    poll_fn: unsafe fn(*mut ()) -> Poll<()>,
    wake_fn: unsafe fn(*const ()),
    drop_fn: unsafe fn(*mut ()),
    type_id: fn() -> TypeId,
    type_name: fn() -> &'static str,
}

unsafe fn task_poll<T: UnsafeWakeableTask>(data: *mut ()) -> Poll<()> {
    // SAFETY: the caller passes a live, exclusively held `T` (UnsafeWakeablePtr::poll).
    unsafe { (*(data as *mut T)).poll() }
}

unsafe fn task_wake<T: UnsafeWakeableTask>(data: *const ()) {
    // SAFETY: the caller passes a live `T` (UnsafeWakeablePtr::wake).
    unsafe { (*(data as *const T)).wake() }
}

unsafe fn task_drop_in_place<T: UnsafeWakeableTask>(data: *mut ()) {
    // SAFETY: the caller passes a live, exclusively held `T` (UnsafeWakeablePtr::drop_in_place).
    unsafe { (*(data as *mut T)).drop_in_place() }
}

impl UnsafeWakeableVTable {
    /// The vtable of `T`: its functions run `T`'s [`UnsafeWakeableTask`] functions.
    pub const fn of<T: UnsafeWakeableTask + 'static>() -> Self {
        // SAFETY: each function reads its pointer as a `T` with the access `from_raw_parts` names
        // and calls `T`'s trait function, whose own contract the caller of `poll`/`wake`/
        // `drop_in_place` upholds.
        unsafe { Self::from_raw_parts::<T>(task_poll::<T>, task_wake::<T>, task_drop_in_place::<T>) }
    }

    /// A vtable built by hand from three functions, recorded as `T`'s (0.1 wrote this as a struct
    /// literal with `pub` fields). `RegisteredTask::new` accepts it for a `Box<T>`. Usable in a
    /// `static`:
    ///
    /// ```
    /// use std::task::Poll;
    /// use waker_tables::{RegisteredTask, UnsafeWakeableTask, UnsafeWakeableVTable, VTableKey, GLOBAL_VTABLE_REGISTRY};
    ///
    /// struct Steps { polls: u32 }
    /// // SAFETY: the functions only touch `self`.
    /// unsafe impl UnsafeWakeableTask for Steps {
    ///     unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) }
    ///     unsafe fn wake(&self) {}
    ///     unsafe fn drop_in_place(&mut self) {}
    /// }
    ///
    /// unsafe fn poll_twice(data: *mut ()) -> Poll<()> {
    ///     // SAFETY: from_raw_parts' contract: `data` is a live Steps, held exclusively.
    ///     let steps = unsafe { &mut *(data as *mut Steps) };
    ///     steps.polls += 1;
    ///     if steps.polls == 2 { Poll::Ready(()) } else { Poll::Pending }
    /// }
    /// unsafe fn ignore(_: *const ()) {}
    /// unsafe fn cleanup(_: *mut ()) {}
    ///
    /// // SAFETY: each function reads its pointer as a live Steps with the access from_raw_parts names.
    /// static STEPS_VTABLE: UnsafeWakeableVTable =
    ///     unsafe { UnsafeWakeableVTable::from_raw_parts::<Steps>(poll_twice, ignore, cleanup) };
    ///
    /// let key = VTableKey { segment: "docs", kind: "steps", version: 1 };
    /// GLOBAL_VTABLE_REGISTRY.register(key, &STEPS_VTABLE);
    /// let mut task = RegisteredTask::new(Box::new(Steps { polls: 0 }), key);
    /// // SAFETY: Steps' functions assume nothing beyond exclusive access.
    /// unsafe {
    ///     assert_eq!(task.poll(), Poll::Pending);
    ///     assert_eq!(task.poll(), Poll::Ready(()));
    /// }
    /// ```
    ///
    /// # Safety
    ///
    /// Each function reads its data pointer as a `T`, under the access the vtable's callers
    /// ([`UnsafeWakeablePtr`] and [`RegisteredTask`]) promise: `poll_fn` and `drop_fn` get a pointer
    /// valid for reads and writes as a `T`, that nothing else uses during the call; `wake_fn` gets a
    /// pointer valid for reads as a `T`. The memory there holds a valid `T`, but it may be a larger
    /// value that starts with one, such as a `#[repr(C)]` struct whose first field is a `T`
    /// ([the data pointer](UnsafeWakeablePtr#the-data-pointer)), so a function touches only the `T`
    /// at the pointer unless its own contract says more. Beyond that, each function may rely only
    /// on its own documented contract: the `# Safety` sections of the `unsafe` `poll`, `wake` and
    /// `drop_in_place` methods name the vtable's function's contract, and their callers uphold it.
    ///
    /// Safe code cannot build a vtable by hand: calling this outside `unsafe` is refused, and the
    /// fields are private (the `E0451` example on [`UnsafeWakeableVTable`]).
    ///
    /// ```compile_fail,E0133
    /// # use std::task::Poll;
    /// # use waker_tables::UnsafeWakeableVTable;
    /// struct A;
    /// struct B(u64);
    /// unsafe fn b_poll(data: *mut ()) -> Poll<()> { unsafe { (*(data as *mut B)).0 += 1 }; Poll::Ready(()) }
    /// unsafe fn b_wake(_: *const ()) {}
    /// unsafe fn b_drop(_: *mut ()) {}
    /// // B's functions recorded as A's: refused outside `unsafe`.
    /// static LIAR: UnsafeWakeableVTable = UnsafeWakeableVTable::from_raw_parts::<A>(b_poll, b_wake, b_drop);
    /// ```
    pub const unsafe fn from_raw_parts<T: 'static>(
        poll_fn: unsafe fn(*mut ()) -> Poll<()>,
        wake_fn: unsafe fn(*const ()),
        drop_fn: unsafe fn(*mut ()),
    ) -> Self {
        Self {
            poll_fn,
            wake_fn,
            drop_fn,
            type_id: TypeId::of::<T>,
            type_name: std::any::type_name::<T>,
        }
    }

    /// The poll function. Calling it is `unsafe`, under [`UnsafeWakeablePtr::poll`]'s contract.
    pub const fn poll_fn(&self) -> unsafe fn(*mut ()) -> Poll<()> {
        self.poll_fn
    }

    /// The wake function. Calling it is `unsafe`, under [`UnsafeWakeablePtr::wake`]'s contract.
    pub const fn wake_fn(&self) -> unsafe fn(*const ()) {
        self.wake_fn
    }

    /// The cleanup function. Calling it is `unsafe`, under [`UnsafeWakeablePtr::drop_in_place`]'s
    /// contract.
    pub const fn drop_fn(&self) -> unsafe fn(*mut ()) {
        self.drop_fn
    }

    /// Whether this vtable reads its data as `T`.
    pub fn is_for<T: 'static>(&self) -> bool {
        (self.type_id)() == TypeId::of::<T>()
    }

    /// The name of the task type this vtable was built for.
    pub fn type_name(&self) -> &'static str {
        (self.type_name)()
    }
}

impl fmt::Debug for UnsafeWakeableVTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnsafeWakeableVTable")
            .field("type_name", &(self.type_name)())
            .finish_non_exhaustive()
    }
}

/// Thin pointer + vtable pair.
///
/// This is the "fat pointer" equivalent you can pass around cheaply. It does not own the
/// task; [`RegisteredTask`] does.
///
/// # The data pointer
///
/// `vtable`'s functions read `data` as one type: the `T` of [`UnsafeWakeableVTable::of`] or
/// [`UnsafeWakeableVTable::from_raw_parts`]. The `unsafe` functions that run them
/// ([`poll`](Self::poll), [`wake`](Self::wake), [`drop_in_place`](Self::drop_in_place) and
/// [`RegisteredTask::from_raw_parts`]) ask for `data` to be valid for reads, and for `poll` and
/// `drop_in_place` also for writes, as that type: not null, aligned for it, and holding a valid
/// value of it. Memory that meets this:
///
/// - that type itself;
/// - a `#[repr(transparent)]` wrapper of it;
/// - a `#[repr(C)]` struct whose first field is that type. The functions of a vtable from `of`
///   touch only that field, so several task types that start with it can share its vtable and key.
///
/// Memory that does not:
///
/// - a type that only has the same size and alignment: an `A(u8)` holding 2, read as a `B(bool)`,
///   is Undefined Behavior;
/// - a struct without `#[repr(C)]`, whose first field need not be at its start;
/// - for writes, a pointer derived from a shared reference `&x`. Take it from `Box::into_raw` or
///   `&mut x as *mut _`.
#[repr(C)]
pub struct UnsafeWakeablePtr {
    /// The task.
    pub data: *mut (),
    /// The task type's vtable.
    pub vtable: &'static UnsafeWakeableVTable,
}

impl UnsafeWakeablePtr {
    /// Runs the vtable's `poll` function on `self.data`.
    ///
    /// # Safety
    /// `self.data` is valid for reads and writes as the type `self.vtable`'s functions read it as
    /// ([the data pointer](UnsafeWakeablePtr#the-data-pointer) lists the memory that qualifies), and
    /// nothing else uses it during the call. Plus the contract of the vtable's `poll`
    /// function ([`UnsafeWakeableVTable::poll_fn`]): for a vtable from [`UnsafeWakeableVTable::of`]
    /// or [`derive_unsafe_wakeable_vtable!`], that is `T`'s [`UnsafeWakeableTask::poll`]; for one
    /// from [`UnsafeWakeableVTable::from_raw_parts`], it is the contract of the function passed as
    /// `poll_fn`.
    pub unsafe fn poll(&mut self) -> Poll<()> {
        // SAFETY: the caller's contract above.
        unsafe { (self.vtable.poll_fn)(self.data) }
    }

    /// Runs the vtable's `wake` function on `self.data`.
    ///
    /// # Safety
    /// `self.data` is valid for reads as the type `self.vtable`'s functions read it as
    /// ([the data pointer](UnsafeWakeablePtr#the-data-pointer) lists the memory that qualifies).
    /// Plus the contract of the vtable's `wake` function ([`UnsafeWakeableVTable::wake_fn`]): for a
    /// vtable from [`UnsafeWakeableVTable::of`] or [`derive_unsafe_wakeable_vtable!`], that is `T`'s
    /// [`UnsafeWakeableTask::wake`]; for one from [`UnsafeWakeableVTable::from_raw_parts`], it is
    /// the contract of the function passed as `wake_fn`.
    pub unsafe fn wake(&self) {
        // SAFETY: the caller's contract above.
        unsafe { (self.vtable.wake_fn)(self.data as *const ()) }
    }

    /// Runs the vtable's `drop` function, the task's cleanup hook, on `self.data`; it does not
    /// free the task.
    ///
    /// # Safety
    /// As [`UnsafeWakeablePtr::poll`], with the contract of the vtable's `drop` function
    /// ([`UnsafeWakeableVTable::drop_fn`]) in place of its `poll` function: for a vtable from
    /// [`UnsafeWakeableVTable::of`] or [`derive_unsafe_wakeable_vtable!`], that is `T`'s
    /// [`UnsafeWakeableTask::drop_in_place`]; for one from [`UnsafeWakeableVTable::from_raw_parts`],
    /// it is the contract of the function passed as `drop_fn`.
    pub unsafe fn drop_in_place(&mut self) {
        // SAFETY: the caller's contract above.
        unsafe { (self.vtable.drop_fn)(self.data) }
    }
}

impl fmt::Debug for UnsafeWakeablePtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We can’t see inside the vtable safely here, but pointer-level
        // introspection is still very useful when debugging schedulers.
        f.debug_struct("UnsafeWakeablePtr")
            .field("data", &self.data)
            .field("type_name", &(self.vtable.type_name)())
            .finish()
    }
}

/// Derive an `UnsafeWakeableVTable` for a concrete type that implements
/// `UnsafeWakeableTask`.
///
/// The vtable calls the `UnsafeWakeableTask` trait's `poll`, `wake` and `drop_in_place`, never an
/// inherent method of the same name. 0.1's macro wrote `(*ptr).poll()` on the concrete type, and
/// Rust picks an inherent `poll` over the trait's, so a type with both ran the inherent one through
/// its vtable. Since 0.2.0 the macro expands to [`UnsafeWakeableVTable::of`], whose functions are
/// generic over `T: UnsafeWakeableTask` and can only see the trait's. This is a fix, not a removal,
/// and it stays:
///
/// ```
/// use std::task::Poll;
/// use waker_tables::{derive_unsafe_wakeable_vtable, UnsafeWakeablePtr, UnsafeWakeableTask};
///
/// struct Shadowed { trait_polls: u32, inherent_polls: u32 }
/// impl Shadowed {
///     fn poll(&mut self) -> Poll<()> { self.inherent_polls += 1; Poll::Ready(()) }
/// }
/// // SAFETY: the functions only touch `self`.
/// unsafe impl UnsafeWakeableTask for Shadowed {
///     unsafe fn poll(&mut self) -> Poll<()> { self.trait_polls += 1; Poll::Pending }
///     unsafe fn wake(&self) {}
///     unsafe fn drop_in_place(&mut self) {}
/// }
/// derive_unsafe_wakeable_vtable!(Shadowed, SHADOWED_VTABLE);
///
/// let mut task = Shadowed { trait_polls: 0, inherent_polls: 0 };
/// let mut ptr = UnsafeWakeablePtr { data: &mut task as *mut Shadowed as *mut (), vtable: &SHADOWED_VTABLE };
/// // SAFETY: `ptr` holds a live Shadowed with its own vtable, used only here.
/// assert_eq!(unsafe { ptr.poll() }, Poll::Pending, "the trait's poll");
/// assert_eq!((task.trait_polls, task.inherent_polls), (1, 0));
/// assert_eq!(task.poll(), Poll::Ready(()), "a direct call still reaches the inherent one");
/// ```
///
/// ```
/// use std::task::Poll;
/// use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};
///
/// struct SmartPinTask { polls: u32 }
///
/// unsafe impl UnsafeWakeableTask for SmartPinTask {
///     unsafe fn poll(&mut self) -> Poll<()> {
///         self.polls += 1;
///         if self.polls == 2 { Poll::Ready(()) } else { Poll::Pending }
///     }
///     unsafe fn wake(&self) {}
///     unsafe fn drop_in_place(&mut self) {}
/// }
///
/// derive_unsafe_wakeable_vtable!(SmartPinTask, SMARTPIN_TASK_VTABLE);
///
/// let key = VTableKey { segment: "scheduler", kind: "smartpin", version: 1 };
/// GLOBAL_VTABLE_REGISTRY.register(key, &SMARTPIN_TASK_VTABLE);
///
/// let mut task = RegisteredTask::new(Box::new(SmartPinTask { polls: 0 }), key);
/// assert_eq!(unsafe { task.poll() }, Poll::Pending);
/// assert_eq!(unsafe { task.poll() }, Poll::Ready(()));
/// // dropping `task` frees the SmartPinTask
/// ```
#[macro_export]
macro_rules! derive_unsafe_wakeable_vtable {
    ($t:ty, $vtable:ident) => {
        pub static $vtable: $crate::UnsafeWakeableVTable = $crate::UnsafeWakeableVTable::of::<$t>();
    };
}

/* ================================
   3. Registry keyed by segment/kind/version
   ================================ */

/// Key for identifying which vtable to use for a given task kind.
///
/// `segment`  → subsystem (e.g., "search", "blob", "scheduler")
/// `kind`     → specific job family (e.g., "gpu_stage", "index_merge")
/// `version`  → protocol evolution / rollout
#[derive(Clone, Copy, Eq, Ord, PartialOrd)]
pub struct VTableKey {
    /// The subsystem.
    pub segment: &'static str,
    /// The job family.
    pub kind: &'static str,
    /// The protocol version.
    pub version: u16,
}

impl PartialEq for VTableKey {
    fn eq(&self, other: &Self) -> bool {
        self.segment == other.segment
            && self.kind == other.kind
            && self.version == other.version
    }
}

impl Hash for VTableKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.segment.hash(state);
        self.kind.hash(state);
        self.version.hash(state);
    }
}

impl fmt::Debug for VTableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VTableKey")
            .field("segment", &self.segment)
            .field("kind", &self.kind)
            .field("version", &self.version)
            .finish()
    }
}

/// Global-ish registry of task vtables.
///
/// You can also embed one registry per cluster/segment if you prefer.
pub struct VTableRegistry {
    inner: RwLock<BTreeMap<VTableKey, &'static UnsafeWakeableVTable>>,
}

impl Default for VTableRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VTableRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register `vtable` under `key`, replacing what was there. Tasks already created keep
    /// the vtable they were created with.
    pub fn register(
        &self,
        key: VTableKey,
        vtable: &'static UnsafeWakeableVTable,
    ) {
        let mut guard = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        guard.insert(key, vtable);
    }

    /// The vtable registered under `key`.
    pub fn get(&self, key: &VTableKey) -> Option<&'static UnsafeWakeableVTable> {
        let guard = self.inner.read().unwrap_or_else(PoisonError::into_inner);
        guard.get(key).copied()
    }
}

/// One global registry (optional; you can ignore it and build your own).
pub static GLOBAL_VTABLE_REGISTRY: LazyLock<VTableRegistry> = LazyLock::new(VTableRegistry::new);


/* ================================
   4. Safety wrapper: RegisteredTask
   ================================ */

/// Owns a boxed task together with the key its vtable was resolved from.
///
/// [`new`](Self::new) checks the vtable against the task's type, and the task is freed when
/// this is dropped. [`from_raw_parts`](Self::from_raw_parts) (`unsafe`) wraps a task over memory
/// the caller owns, which is never freed here; [`owns_its_task`](Self::owns_its_task) says which
/// of the two a task is. [`into_raw_parts`](Self::into_raw_parts) hands the pointer back. The
/// fields are private so the pointer, vtable and free fn stay together:
///
/// ```compile_fail,E0616
/// # use waker_tables::RegisteredTask;
/// fn swap(a: &mut RegisteredTask, b: &mut RegisteredTask) {
///     std::mem::swap(&mut a.ptr, &mut b.ptr); // private: a's Drop must never free b's task
/// }
/// ```
pub struct RegisteredTask {
    key: VTableKey,
    ptr: UnsafeWakeablePtr,
    // Frees the box `new` took: `T` is known only there, so its free fn is kept here. `None` for a
    // task from `from_raw_parts`, whose memory the caller owns.
    free: Option<unsafe fn(*mut ())>,
}

unsafe fn free_box<T>(data: *mut ()) {
    // SAFETY: `data` came from `Box::<T>::into_raw` in `RegisteredTask::new` and is freed once,
    // from `Drop`.
    drop(unsafe { Box::from_raw(data as *mut T) });
}

impl Drop for RegisteredTask {
    fn drop(&mut self) {
        if let Some(free) = self.free {
            // SAFETY: `free` is set only by `new`, together with `ptr.data`, and the fields are
            // private.
            unsafe { free(self.ptr.data) }
        }
    }
}

impl RegisteredTask {
    /// Creates a new registered task.
    ///
    /// # Panics
    /// Panics if `key` is not registered in `GLOBAL_VTABLE_REGISTRY`, or if the vtable
    /// registered under it was built for a type other than `T`. The task is dropped first.
    pub fn new<T: UnsafeWakeableTask + 'static>(
        task: Box<T>,
        key: VTableKey,
    ) -> Self {
        let vtable = GLOBAL_VTABLE_REGISTRY
            .get(&key)
            .expect("vtable for key not registered");
        assert!(
            vtable.is_for::<T>(),
            "vtable for key {:?} is for {}, not {}",
            key,
            (vtable.type_name)(),
            std::any::type_name::<T>(),
        );

        Self {
            key,
            ptr: UnsafeWakeablePtr {
                data: Box::into_raw(task) as *mut (),
                vtable,
            },
            free: Some(free_box::<T>),
        }
    }

    /// A task over memory the caller owns (0.1 wrote this as the struct literal
    /// `RegisteredTask { key, ptr }`). `key` is recorded as given: it is not looked up or checked.
    /// `RegisteredTask`'s `Drop` never frees this task ([`owns_its_task`](Self::owns_its_task) is
    /// `false`); the caller frees it, after this is dropped or after
    /// [`into_raw_parts`](Self::into_raw_parts) hands the pointer back. It is a `const fn`, since
    /// the struct literal worked in a `const`: a `const` task can fill an array as `[TASK; N]`.
    ///
    /// ```
    /// use std::task::Poll;
    /// use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeablePtr, UnsafeWakeableTask, VTableKey};
    ///
    /// struct Steps { polls: u32 }
    /// // SAFETY: the functions only touch `self`.
    /// unsafe impl UnsafeWakeableTask for Steps {
    ///     unsafe fn poll(&mut self) -> Poll<()> { self.polls += 1; Poll::Pending }
    ///     unsafe fn wake(&self) {}
    ///     unsafe fn drop_in_place(&mut self) {}
    /// }
    /// derive_unsafe_wakeable_vtable!(Steps, STEPS_VTABLE);
    ///
    /// let mut steps = Steps { polls: 0 }; // on the stack: the caller owns it
    /// let key = VTableKey { segment: "docs", kind: "steps", version: 1 };
    /// let ptr = UnsafeWakeablePtr { data: &mut steps as *mut Steps as *mut (), vtable: &STEPS_VTABLE };
    /// // SAFETY: `steps` is a live Steps, outlives `task`, and is used only through `task` meanwhile.
    /// let mut task = unsafe { RegisteredTask::from_raw_parts(key, ptr) };
    /// // SAFETY: Steps' functions assume nothing beyond exclusive access.
    /// unsafe { task.poll() };
    /// drop(task); // does not free `steps`
    /// assert_eq!(steps.polls, 1);
    /// ```
    ///
    /// # Safety
    ///
    /// `ptr.data` is valid for reads and writes as the type `ptr.vtable`'s functions read it as,
    /// for as long as the `RegisteredTask` exists, and is never derived from a shared reference
    /// `&x`. [The data pointer](UnsafeWakeablePtr#the-data-pointer) lists the memory that meets
    /// this and the memory that does not. While a `poll`, `wake` or `drop_in_place` call on the
    /// task runs, nothing else uses it.
    ///
    /// Safe code still cannot pair a task with another type's vtable: calling this outside
    /// `unsafe` is refused, and [`new`](Self::new) panics on a key registered for another type.
    ///
    /// ```compile_fail,E0133
    /// # use waker_tables::{RegisteredTask, UnsafeWakeablePtr, VTableKey, GLOBAL_VTABLE_REGISTRY};
    /// let key = VTableKey { segment: "docs", kind: "b", version: 1 };
    /// let a = Box::into_raw(Box::new(0u8)) as *mut ();
    /// let task = RegisteredTask::from_raw_parts(key, UnsafeWakeablePtr { data: a, vtable: GLOBAL_VTABLE_REGISTRY.get(&key).unwrap() });
    /// ```
    ///
    /// ```should_panic
    /// # use std::task::Poll;
    /// # use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};
    /// struct A;
    /// struct B(u64);
    /// # unsafe impl UnsafeWakeableTask for A { unsafe fn poll(&mut self) -> Poll<()> { Poll::Ready(()) } unsafe fn wake(&self) {} unsafe fn drop_in_place(&mut self) {} }
    /// # unsafe impl UnsafeWakeableTask for B { unsafe fn poll(&mut self) -> Poll<()> { self.0 += 1; Poll::Ready(()) } unsafe fn wake(&self) {} unsafe fn drop_in_place(&mut self) {} }
    /// derive_unsafe_wakeable_vtable!(B, B_VTABLE);
    /// let key = VTableKey { segment: "docs", kind: "b", version: 1 };
    /// GLOBAL_VTABLE_REGISTRY.register(key, &B_VTABLE);
    /// // An A under B's key: panics ("vtable for key ... is for B, not A").
    /// let _ = RegisteredTask::new(Box::new(A), key);
    /// ```
    pub const unsafe fn from_raw_parts(key: VTableKey, ptr: UnsafeWakeablePtr) -> Self {
        Self { key, ptr, free: None }
    }

    /// Whether this `RegisteredTask` owns its task and frees it when dropped: `true` for one made
    /// by [`new`](Self::new), which boxed the task, and `false` for one made by
    /// [`from_raw_parts`](Self::from_raw_parts), whose memory the caller owns.
    ///
    /// Check it before [`into_raw_parts`](Self::into_raw_parts), whose result does not say: code
    /// that takes a `RegisteredTask` it did not build learns here whether the pointer it gets back
    /// is a `Box<T>` it must free or memory it must leave alone.
    pub fn owns_its_task(&self) -> bool {
        self.free.is_some()
    }

    /// Hand back the key and the task pointer without freeing the task.
    ///
    /// Check [`owns_its_task`](Self::owns_its_task) first: the result does not say who owns the
    /// memory. If it was `true` (a task made by [`new`](Self::new)), the caller now owns the
    /// `Box<T>` and must free it, with `drop(unsafe { Box::from_raw(ptr.data as *mut T) })`, or it
    /// leaks. If it was `false` (a task made by [`from_raw_parts`](Self::from_raw_parts)), the
    /// memory belongs to whoever called `from_raw_parts`, and only they free it: freeing it here
    /// as a `Box<T>` could free stack memory, or free it a second time.
    ///
    /// ```
    /// use std::task::Poll;
    /// use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};
    ///
    /// struct Steps { polls: u32 }
    /// // SAFETY: the functions only touch `self`.
    /// unsafe impl UnsafeWakeableTask for Steps {
    ///     unsafe fn poll(&mut self) -> Poll<()> { self.polls += 1; Poll::Pending }
    ///     unsafe fn wake(&self) {}
    ///     unsafe fn drop_in_place(&mut self) {}
    /// }
    /// derive_unsafe_wakeable_vtable!(Steps, STEPS_VTABLE);
    /// let key = VTableKey { segment: "docs", kind: "into-raw-parts", version: 1 };
    /// GLOBAL_VTABLE_REGISTRY.register(key, &STEPS_VTABLE);
    ///
    /// let task = RegisteredTask::new(Box::new(Steps { polls: 0 }), key);
    /// let owned = task.owns_its_task(); // first: the parts do not say
    /// let (_, ptr) = task.into_raw_parts();
    /// if owned {
    ///     // SAFETY: `new` boxed a Steps, and the caller now owns that Box; it is freed once, here.
    ///     drop(unsafe { Box::from_raw(ptr.data as *mut Steps) });
    /// }
    /// # assert!(owned);
    /// ```
    ///
    /// Throwing the result away is warned about (`#[must_use]`), since a task made by `new` would
    /// leak:
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # use std::task::Poll;
    /// # use waker_tables::{derive_unsafe_wakeable_vtable, RegisteredTask, UnsafeWakeableTask, VTableKey, GLOBAL_VTABLE_REGISTRY};
    /// # struct Steps { polls: u32 }
    /// # unsafe impl UnsafeWakeableTask for Steps {
    /// #     unsafe fn poll(&mut self) -> Poll<()> { self.polls += 1; Poll::Pending }
    /// #     unsafe fn wake(&self) {}
    /// #     unsafe fn drop_in_place(&mut self) {}
    /// # }
    /// # derive_unsafe_wakeable_vtable!(Steps, STEPS_VTABLE);
    /// # let key = VTableKey { segment: "docs", kind: "into-raw-parts", version: 1 };
    /// # GLOBAL_VTABLE_REGISTRY.register(key, &STEPS_VTABLE);
    /// let task = RegisteredTask::new(Box::new(Steps { polls: 0 }), key);
    /// task.into_raw_parts(); // unused_must_use: the Box leaks
    /// ```
    #[must_use = "check owns_its_task() before this call: if it was true, `ptr.data` must be freed as Box<T> or the task leaks"]
    pub fn into_raw_parts(self) -> (VTableKey, UnsafeWakeablePtr) {
        let this = ManuallyDrop::new(self);
        // SAFETY: `this` is never dropped, so the `ptr` read out here is the only copy left in use,
        // and `UnsafeWakeablePtr` has no `Drop` of its own.
        let ptr = unsafe { std::ptr::read(&this.ptr) };
        (this.key, ptr)
    }

    /// The key the vtable was resolved from.
    pub fn key(&self) -> VTableKey {
        self.key
    }

    /// The task pointer and its vtable.
    pub fn ptr(&self) -> &UnsafeWakeablePtr {
        &self.ptr
    }

    /// Runs the vtable's `poll` function on the task.
    ///
    /// # Safety
    /// The contract of the vtable's `poll` function ([`UnsafeWakeableVTable::poll_fn`]): for a
    /// vtable from [`UnsafeWakeableVTable::of`] or [`derive_unsafe_wakeable_vtable!`], that is `T`'s
    /// [`UnsafeWakeableTask::poll`]; for one from [`UnsafeWakeableVTable::from_raw_parts`], it is the
    /// contract of the function passed as `poll_fn`.
    pub unsafe fn poll(&mut self) -> Poll<()> {
        // SAFETY: the vtable is `T`'s (checked in `new`) and the task is a live `Box<T>` until
        // `Drop`, valid for reads and writes, or the caller of `from_raw_parts` promised a pointer
        // valid for reads and writes as the vtable's type; `&mut self` makes the access exclusive
        // (or `from_raw_parts`' caller promised nothing else uses it); the vtable function's own
        // contract is the caller's.
        unsafe { self.ptr.poll() }
    }

    /// Runs the vtable's `wake` function on the task.
    ///
    /// # Safety
    /// The contract of the vtable's `wake` function ([`UnsafeWakeableVTable::wake_fn`]): for a
    /// vtable from [`UnsafeWakeableVTable::of`] or [`derive_unsafe_wakeable_vtable!`], that is `T`'s
    /// [`UnsafeWakeableTask::wake`]; for one from [`UnsafeWakeableVTable::from_raw_parts`], it is the
    /// contract of the function passed as `wake_fn`.
    pub unsafe fn wake(&self) {
        // SAFETY: as `poll`.
        unsafe { self.ptr.wake() }
    }

    /// Runs the vtable's `drop` function, the task's cleanup hook. The task is still freed only
    /// when this is dropped.
    ///
    /// # Safety
    /// The contract of the vtable's `drop` function ([`UnsafeWakeableVTable::drop_fn`]): for a
    /// vtable from [`UnsafeWakeableVTable::of`] or [`derive_unsafe_wakeable_vtable!`], that is `T`'s
    /// [`UnsafeWakeableTask::drop_in_place`]; for one from [`UnsafeWakeableVTable::from_raw_parts`],
    /// it is the contract of the function passed as `drop_fn`.
    pub unsafe fn drop_in_place(&mut self) {
        // SAFETY: as `poll`.
        unsafe { self.ptr.drop_in_place() }
    }
}

impl fmt::Debug for RegisteredTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisteredTask")
            .field("key", &self.key)
            .field("ptr", &self.ptr)
            .finish()
    }
}

/// The README's code blocks, run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../readme.md")]
pub struct ReadmeDoctests;

/* ================================
   5. (Optional) tests skeleton
   ================================ */

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTask {
        #[allow(dead_code)]
        pub woken: bool,
        pub polled: usize,
    }

    // SAFETY: the functions only touch `self`.
    unsafe impl UnsafeWakeableTask for DummyTask {
        unsafe fn poll(&mut self) -> Poll<()> {
            self.polled += 1;
            Poll::Pending
        }

        unsafe fn wake(&self) {
            // no-op here; just checking callability
        }

        unsafe fn drop_in_place(&mut self) {
            // nothing special
        }
    }

    derive_unsafe_wakeable_vtable!(DummyTask, DUMMY_TASK_VTABLE);

    #[test]
    fn registry_and_registered_task_basic_flow() {
        let key = VTableKey {
            segment: "test",
            kind: "dummy",
            version: 1,
        };

        GLOBAL_VTABLE_REGISTRY.register(key, &DUMMY_TASK_VTABLE);

        let task = Box::new(DummyTask {
            woken: false,
            polled: 0,
        });

        let mut registered = RegisteredTask::new(task, key);

        // SAFETY: DummyTask's functions assume nothing beyond exclusive access.
        unsafe {
            let _ = registered.poll();
            registered.wake();
            registered.drop_in_place();
        }
    }
}
