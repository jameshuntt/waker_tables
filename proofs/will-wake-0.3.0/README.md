# will-wake-0.3.0

`waker_tables = "=0.3.0"`. A waker from `waker_from_arc` and five clones of it.

`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256 cargo run --release` gives `will_wake true for 0 of 5 clones`. `cargo run` and `cargo run --release` print `5 of 5`; this small program needs the extra codegen units to split the constant.
