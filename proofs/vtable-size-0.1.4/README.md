# vtable-size-0.1.4

`waker_tables = "=0.1.4"`. The size of `UnsafeWakeableVTable` and `UnsafeWakeablePtr`, and their `Debug` output.

`cargo run` gives `size_of UnsafeWakeableVTable = 24, UnsafeWakeablePtr = 16`, then `UnsafeWakeableVTable { .. }` and `UnsafeWakeablePtr { data: 0x0 }`.
