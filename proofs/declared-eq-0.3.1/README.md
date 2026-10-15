# declared-eq-0.3.1

`waker_tables = "=0.3.1"`. Compares the static `declare_raw_waker_vtable!` declares with the type's own vtable, as 0.3.1's test `declared_static_is_the_types_own_vtable` did.

`cargo run` gives `*waker_from_arc(..).vtable() == SIGNAL_VTABLE: true, *Signal::raw_waker_vtable() == SIGNAL_VTABLE: true`.
