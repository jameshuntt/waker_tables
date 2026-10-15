# vtable-size-0.2.0

`waker_tables = "=0.2.0"`. The same program as `vtable-size-0.1.4`.

`cargo run` gives `size_of UnsafeWakeableVTable = 40, UnsafeWakeablePtr = 16`, then `UnsafeWakeableVTable { type_name: "proof_vtable_size_0_2_0::Inner", .. }` and `UnsafeWakeablePtr { data: 0x0, type_name: "proof_vtable_size_0_2_0::Inner" }`.
