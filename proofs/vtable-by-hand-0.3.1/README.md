# vtable-by-hand-0.3.1

`waker_tables = "=0.3.1"`. The same source as `vtable-by-hand-0.1.4`.

`cargo build --bin build_by_hand` gives ``error: cannot construct `UnsafeWakeableVTable` with struct literal syntax due to private fields``.

`cargo build --bin read_poll_fn` gives ``error[E0616]: field `poll_fn` of struct `UnsafeWakeableVTable` is private``.
