# data-pointer-0.4.0

git commit `956a08d` (0.4.0). Three tasks handed to `RegisteredTask::from_raw_parts` that break "The data pointer" rule on purpose, each polled once.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin same_size` gives `error: Undefined Behavior: constructing invalid value of type bool: encountered 0x02, but expected a boolean`. `A(u8)` and `B(bool)` have the same size and alignment; the program asserts it.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin no_repr_c` gives `offset_of!(Fetch, header) = 8`, then the same `encountered 0x02, but expected a boolean` error: without `#[repr(C)]` the compiler put `id` first, and the header's `poll` read its low byte.

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run --bin from_shared_ref` gives `error: Undefined Behavior: trying to retag from <..> for SharedReadWrite permission at alloc..[0x0], but that tag only grants SharedReadOnly permission for this location`, in `waker_tables::task_poll::<Counter>`. The tag and allocation numbers vary.
