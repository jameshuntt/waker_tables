// waker_tables/src/lib.rs

//! Custom raw waker logic: hand-built vtables for wakers and for tasks.
//!
//! Two halves:
//!
//! * **Wakers.** Put [`UnsafeWakeable`] on an `Arc`-backed type (usually with
//!   [`derive_arc_unsafe_wakeable!`]) and [`waker_from_arc`] turns an `Arc<T>` into a
//!   [`Waker`]. The `RawWakerVTable` is `T`'s own ([`UnsafeWakeable::raw_waker_vtable`]),
//!   so a waker can only ever run its own type's functions.
//! * **Tasks.** Put [`UnsafeWakeableTask`] on a task type, declare its vtable with
//!   [`derive_unsafe_wakeable_vtable!`], register it under a [`VTableKey`], and
//!   [`RegisteredTask`] owns the boxed task, runs it through the vtable, and frees it
//!   when dropped. The vtable records its task type, and `RegisteredTask::new` refuses a
//!   key whose vtable belongs to another type.
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
///   whose functions read `data` as `*const Self` (what [`declare_raw_waker_vtable!`] declares).
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
/// Kept for code written against 0.1: [`waker_from_arc`] no longer takes a vtable, it uses
/// `T`'s own ([`UnsafeWakeable::raw_waker_vtable`]). The static this declares holds the same
/// four functions, at its own address.
///
/// ```
/// use std::sync::Arc;
/// use waker_tables::{declare_raw_waker_vtable, derive_arc_unsafe_wakeable};
///
/// struct SmartPinTask;
/// impl SmartPinTask {
///     fn on_wake(self: Arc<Self>) {}
/// }
/// derive_arc_unsafe_wakeable!(SmartPinTask, on_wake);
///
/// declare_raw_waker_vtable!(SmartPinTask, SMARTPIN_TASK_RAW_VTABLE);
/// ```
#[macro_export]
macro_rules! declare_raw_waker_vtable {
    ($t:ty, $name:ident) => {
        pub static $name: ::std::task::RawWakerVTable = $crate::RawWakerVTableOf::<$t>::VTABLE;
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
/// records the task type, and [`RegisteredTask::new`] checks it. The fields are private, so a
/// vtable cannot claim one type while holding another type's functions:
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
    /// The vtable of `T`.
    pub const fn of<T: UnsafeWakeableTask + 'static>() -> Self {
        Self {
            poll_fn: task_poll::<T>,
            wake_fn: task_wake::<T>,
            drop_fn: task_drop_in_place::<T>,
            type_id: TypeId::of::<T>,
            type_name: std::any::type_name::<T>,
        }
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
#[repr(C)]
pub struct UnsafeWakeablePtr {
    /// The task.
    pub data: *mut (),
    /// The task type's vtable.
    pub vtable: &'static UnsafeWakeableVTable,
}

impl UnsafeWakeablePtr {
    /// # Safety
    /// `self.data` is a live task of the type `self.vtable` was built for, not used
    /// elsewhere during the call; plus the task's own `poll` contract.
    pub unsafe fn poll(&mut self) -> Poll<()> {
        // SAFETY: the caller's contract above.
        unsafe { (self.vtable.poll_fn)(self.data) }
    }

    /// # Safety
    /// `self.data` is a live task of the type `self.vtable` was built for; plus the task's
    /// own `wake` contract.
    pub unsafe fn wake(&self) {
        // SAFETY: the caller's contract above.
        unsafe { (self.vtable.wake_fn)(self.data as *const ()) }
    }

    /// Runs the task's cleanup hook; it does not free the task.
    ///
    /// # Safety
    /// As [`UnsafeWakeablePtr::poll`], with the task's own `drop_in_place` contract.
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
/// The vtable is checked against the task's type when created, and the task is freed when
/// this is dropped. The fields are private so the pointer, vtable and free fn stay together:
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
    // Frees the box `new` took: `T` is known only there, so its free fn is kept here.
    free: unsafe fn(*mut ()),
}

unsafe fn free_box<T>(data: *mut ()) {
    // SAFETY: `data` came from `Box::<T>::into_raw` in `RegisteredTask::new` and is freed once,
    // from `Drop`.
    drop(unsafe { Box::from_raw(data as *mut T) });
}

impl Drop for RegisteredTask {
    fn drop(&mut self) {
        // SAFETY: `ptr.data` and `free` were set together in `new` and the fields are private.
        unsafe { (self.free)(self.ptr.data) }
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
            free: free_box::<T>,
        }
    }

    /// The key the vtable was resolved from.
    pub fn key(&self) -> VTableKey {
        self.key
    }

    /// The task pointer and its vtable.
    pub fn ptr(&self) -> &UnsafeWakeablePtr {
        &self.ptr
    }

    /// # Safety
    /// The task's own `poll` contract.
    pub unsafe fn poll(&mut self) -> Poll<()> {
        // SAFETY: the vtable is `T`'s (checked in `new`), the task is live until `Drop`, and
        // `&mut self` makes the access exclusive; the rest is the caller's.
        unsafe { self.ptr.poll() }
    }

    /// # Safety
    /// The task's own `wake` contract.
    pub unsafe fn wake(&self) {
        // SAFETY: as `poll`.
        unsafe { self.ptr.wake() }
    }

    /// Run the task's cleanup hook. The task is still freed only when this is dropped.
    ///
    /// # Safety
    /// The task's own `drop_in_place` contract.
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
