# declared-clone-0.1.4

`waker_tables = "=0.1.4"`. A waker built with std's `Waker::from_raw` on the static `declare_raw_waker_vtable!` declares, and one clone.

`cargo run` gives `clone carries DECLARED: true, clone.will_wake(&waker): true`.
