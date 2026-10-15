# task-literal-0.3.1

`waker_tables = "=0.3.1"`. The same source as `task-literal-0.1.4`.

`cargo build --bin slab_slot` gives ``error: cannot construct `RegisteredTask` with struct literal syntax due to private fields``, and twice ``error[E0616]: field `ptr` of struct `RegisteredTask` is private``.

`cargo build --bin const_table` gives ``error: cannot construct `RegisteredTask` with struct literal syntax due to private fields``.
