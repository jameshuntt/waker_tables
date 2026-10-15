# task-literal-0.1.4

`waker_tables = "=0.1.4"`. `slab_slot`: a `RegisteredTask` written as a struct literal over a slot of a fixed array (not a `Box`), polled, and its pointer moved back out. `const_table`: a `RegisteredTask` struct literal in a `const`, filling `[TICK; 4]`.

`cargo run --bin slab_slot` gives `slab[1] = 11, the pointer moved back out is slab[1]: true`. Miri (`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin slab_slot`) reports nothing.

`cargo run --bin const_table` gives `const table of 4 tasks, polls = 4`. Miri reports nothing.
