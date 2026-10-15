# unsafe-wake-method-0.3.1

`waker_tables = "=0.3.1"`. The same two programs as `unsafe-wake-method-0.3.0`.

`cargo build --bin deref_unwrap_unchecked` gives ``error[E0133]: call to unsafe function `Option::<T>::unwrap_unchecked` is unsafe and requires unsafe block``.

`cargo build --bin own_unsafe_fn` gives ``error[E0133]: call to unsafe function `OnExecutorThread::on_wake` is unsafe and requires unsafe block``.
