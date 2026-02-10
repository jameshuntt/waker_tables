// waker_tables/src/lib.rs

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::task::{Poll, RawWaker, RawWakerVTable, Waker};

use once_cell::sync::Lazy;

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
///
/// This is **purely about refcount and memory ownership**, not scheduling.
/// Scheduling logic (enqueueing, routing, etc.) lives in your own types.
pub unsafe trait UnsafeWakeable: Send + Sync + 'static {
    unsafe fn wake(ptr: *const Self);
    unsafe fn wake_by_ref(ptr: *const Self);
    unsafe fn clone_ptr(ptr: *const Self) -> *const Self;
    unsafe fn drop_ptr(ptr: *const Self);
}

/// Helper macro for the common `Arc<T>`-backed case.
///
/// You implement your *behavior* on `Arc<T>` and this macro wires
/// the raw pointer plumbing correctly.
///
/// ```ignore
/// impl SmartPinTask {
///     pub fn on_wake(self: Arc<Self>) {
///         self.scheduler.wake_stage(self.plan_id, self.id);
///     }
/// }
///
/// derive_arc_unsafe_wakeable!(SmartPinTask, on_wake);
/// ```
#[macro_export]
macro_rules! derive_arc_unsafe_wakeable {
    ($t:ty, $wake_method:ident) => {
        unsafe impl $crate::UnsafeWakeable for $t {
            unsafe fn wake(ptr: *const Self) {
                let arc = std::sync::Arc::from_raw(ptr);
                arc.$wake_method();
                // drop arc
            }

            unsafe fn wake_by_ref(ptr: *const Self) {
                let arc = std::sync::Arc::from_raw(ptr);
                let cloned = arc.clone();
                std::mem::forget(arc); // keep original alive
                cloned.$wake_method();
                // drop cloned
            }

            unsafe fn clone_ptr(ptr: *const Self) -> *const Self {
                let arc = std::sync::Arc::from_raw(ptr);
                let cloned = arc.clone();
                std::mem::forget(arc);
                std::sync::Arc::into_raw(cloned)
            }

            unsafe fn drop_ptr(ptr: *const Self) {
                // balances one strong ref
                let _ = std::sync::Arc::from_raw(ptr);
            }
        }
    };
}

/// Declare a `RawWakerVTable` for a specific `UnsafeWakeable` type.
///
/// This gives you a `'static` vtable constant that you can reuse everywhere.
///
/// ```ignore
/// use waker_tables::{UnsafeWakeable, declare_raw_waker_vtable, waker_from_arc};
///
/// struct SmartPinTask { /* ... */ }
/// unsafe impl UnsafeWakeable for SmartPinTask { /* ... */ }
///
/// declare_raw_waker_vtable!(SmartPinTask, SMARTPIN_TASK_RAW_VTABLE);
///
/// fn make_waker(task: Arc<SmartPinTask>) -> Waker {
///     waker_from_arc(task, &SMARTPIN_TASK_RAW_VTABLE)
/// }
/// ```
#[macro_export]
macro_rules! declare_raw_waker_vtable {
    ($t:ty, $name:ident) => {
        unsafe fn __waker_clone(data: *const ()) -> std::task::RawWaker {
            let ptr = data as *const $t;
            let new_ptr = <$t as $crate::UnsafeWakeable>::clone_ptr(ptr);
            std::task::RawWaker::new(new_ptr as *const (), &$name)
        }

        unsafe fn __waker_wake(data: *const ()) {
            let ptr = data as *const $t;
            <$t as $crate::UnsafeWakeable>::wake(ptr);
        }

        unsafe fn __waker_wake_by_ref(data: *const ()) {
            let ptr = data as *const $t;
            <$t as $crate::UnsafeWakeable>::wake_by_ref(ptr);
        }

        unsafe fn __waker_drop(data: *const ()) {
            let ptr = data as *const $t;
            <$t as $crate::UnsafeWakeable>::drop_ptr(ptr);
        }

        pub static $name: std::task::RawWakerVTable = std::task::RawWakerVTable::new(
            __waker_clone,
            __waker_wake,
            __waker_wake_by_ref,
            __waker_drop,
        );
    };
}

/// Cheap wrapper for `Arc<T>` + a `RawWakerVTable`.
#[repr(transparent)]
pub struct SmartWakerPtr<T: UnsafeWakeable>(*const T);

impl<T: UnsafeWakeable> SmartWakerPtr<T> {
    pub fn from_arc(arc: Arc<T>) -> Self {
        Self(Arc::into_raw(arc))
    }

    /// # Safety
    /// The pointer must have been created by `SmartWakerPtr::from_arc`.
    pub unsafe fn to_arc(self) -> Arc<T> {
        Arc::from_raw(self.0)
    }

    /// Clone the underlying `Arc` without dropping the original.
    pub unsafe fn to_cloned_arc(&self) -> Arc<T> {
        let arc = Arc::from_raw(self.0);
        let cloned = arc.clone();
        std::mem::forget(arc);
        cloned
    }
}

/// Build a `RawWaker` from a raw pointer and pre-declared vtable.
pub fn raw_waker_from_ptr<T: UnsafeWakeable>(
    ptr: *const T,
    vtable: &'static RawWakerVTable,
) -> RawWaker {
    RawWaker::new(ptr as *const (), vtable)
}

/// Build a `Waker` from an `Arc<T>` + vtable.
pub fn waker_from_arc<T: UnsafeWakeable>(
    arc: Arc<T>,
    vtable: &'static RawWakerVTable,
) -> Waker {
    let ptr = Arc::into_raw(arc);
    unsafe { Waker::from_raw(raw_waker_from_ptr(ptr, vtable)) }
}

/* ================================
   2. Task vtable (poll/wake/drop)
   ================================ */

/// Trait describing an object that can be polled + woken + dropped
/// via a manual vtable.
///
/// This is your **abstract task**, not necessarily a `Future`.
pub unsafe trait UnsafeWakeableTask {
    unsafe fn poll(&mut self) -> Poll<()>;
    unsafe fn wake(&self);
    unsafe fn drop_in_place(&mut self);
}

/// Function-pointer vtable for `UnsafeWakeableTask`.
pub struct UnsafeWakeableVTable {
    pub poll_fn: unsafe fn(*mut ()) -> Poll<()>,
    pub wake_fn: unsafe fn(*const ()),
    pub drop_fn: unsafe fn(*mut ()),
}

impl fmt::Debug for UnsafeWakeableVTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnsafeWakeableVTable").finish_non_exhaustive()
    }
}

/// Thin pointer + vtable pair.
///
/// This is the "fat pointer" equivalent you can pass around cheaply.
#[repr(C)]
pub struct UnsafeWakeablePtr {
    pub data: *mut (),
    pub vtable: &'static UnsafeWakeableVTable,
}

impl UnsafeWakeablePtr {
    /// # Safety
    /// `self.data` must be a valid `UnsafeWakeableTask` for this vtable.
    pub unsafe fn poll(&mut self) -> Poll<()> {
        (self.vtable.poll_fn)(self.data)
    }

    pub unsafe fn wake(&self) {
        (self.vtable.wake_fn)(self.data as *const ())
    }

    pub unsafe fn drop_in_place(&mut self) {
        (self.vtable.drop_fn)(self.data)
    }
}

impl fmt::Debug for UnsafeWakeablePtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We can’t see inside the vtable safely here, but pointer-level
        // introspection is still very useful when debugging schedulers.
        f.debug_struct("UnsafeWakeablePtr")
            .field("data", &self.data)
            .finish()
    }
}

/// Derive an `UnsafeWakeableVTable` for a concrete type that implements
/// `UnsafeWakeableTask`.
///
/// ```ignore
/// struct SmartPinTask { /* ... */ }
///
/// unsafe impl UnsafeWakeableTask for SmartPinTask {
///     unsafe fn poll(&mut self) -> Poll<()> { /* ... */ }
///     unsafe fn wake(&self) { /* ... */ }
///     unsafe fn drop_in_place(&mut self) { /* ... */ }
/// }
///
/// derive_unsafe_wakeable_vtable!(SmartPinTask, SMARTPIN_TASK_VTABLE);
/// ```
#[macro_export]
macro_rules! derive_unsafe_wakeable_vtable {
    ($t:ty, $vtable:ident) => {
        pub static $vtable: $crate::UnsafeWakeableVTable = $crate::UnsafeWakeableVTable {
            poll_fn: |ptr| unsafe { (*(ptr as *mut $t)).poll() },
            wake_fn: |ptr| unsafe { (*(ptr as *const $t)).wake() },
            drop_fn: |ptr| unsafe { (*(ptr as *mut $t)).drop_in_place() },
        };
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
    pub segment: &'static str,
    pub kind: &'static str,
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

impl VTableRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn register(
        &self,
        key: VTableKey,
        vtable: &'static UnsafeWakeableVTable,
    ) {
        let mut guard = self.inner.write().unwrap();
        guard.insert(key, vtable);
    }

    pub fn get(&self, key: &VTableKey) -> Option<&'static UnsafeWakeableVTable> {
        let guard = self.inner.read().unwrap();
        guard.get(key).copied()
    }
}

/// One global registry (optional; you can ignore it and build your own).
pub static GLOBAL_VTABLE_REGISTRY: Lazy<VTableRegistry> =
    Lazy::new(VTableRegistry::new);

/* ================================
   4. Safety wrapper: RegisteredTask
   ================================ */

/// Safer wrapper that couples a task pointer with the key used to
/// resolve its vtable.
///
/// This helps avoid mixing the wrong pointer with the wrong vtable.
pub struct RegisteredTask {
    pub key: VTableKey,
    pub ptr: UnsafeWakeablePtr,
}

impl RegisteredTask {
    /// Creates a new registered task.
    ///
    /// # Panics
    /// Panics if `key` is not registered in `GLOBAL_VTABLE_REGISTRY`.
    pub fn new<T: UnsafeWakeableTask + 'static>(
        task: Box<T>,
        key: VTableKey,
    ) -> Self {
        let vtable = GLOBAL_VTABLE_REGISTRY
            .get(&key)
            .expect("vtable for key not registered");

        Self {
            key,
            ptr: UnsafeWakeablePtr {
                data: Box::into_raw(task) as *mut (),
                vtable,
            },
        }
    }

    /// # Safety
    /// Caller must ensure exclusive access to the underlying task while
    /// it is being polled.
    pub unsafe fn poll(&mut self) -> Poll<()> {
        self.ptr.poll()
    }

    pub unsafe fn wake(&self) {
        self.ptr.wake()
    }

    /// Drop the underlying task in-place via vtable.
    pub unsafe fn drop_in_place(&mut self) {
        self.ptr.drop_in_place()
    }
}

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

        unsafe {
            let _ = registered.poll();
            registered.wake();
            registered.drop_in_place();
        }
    }
}