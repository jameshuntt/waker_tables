# will-wake-0.1.4

`waker_tables = "=0.1.4"`. A waker on the static `declare_raw_waker_vtable!` declares, and five clones of it.

`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256 cargo run --release` gives `will_wake true for 5 of 5 clones`. `cargo run` and `cargo run --release` print the same.
