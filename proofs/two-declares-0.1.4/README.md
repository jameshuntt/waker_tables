# two-declares-0.1.4

`waker_tables = "=0.1.4"`. Two `declare_raw_waker_vtable!` in one module.

`cargo build` gives four errors: ``error[E0428]: the name `__waker_clone` is defined multiple times``, and the same for `__waker_wake`, `__waker_wake_by_ref` and `__waker_drop`.
