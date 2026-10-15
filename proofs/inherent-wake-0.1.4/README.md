# inherent-wake-0.1.4

`waker_tables = "=0.1.4"`. A task type with an inherent `fn wake(&self)` next to `UnsafeWakeableTask::wake`, woken once through the vtable from `derive_unsafe_wakeable_vtable!`.

`cargo run` gives `one wake through the vtable: inherent = 1, trait = 0`.
