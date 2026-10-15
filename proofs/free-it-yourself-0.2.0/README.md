# free-it-yourself-0.2.0

`waker_tables = "=0.2.0"`. The program from `free-it-yourself-0.1.4` with the one rename the 0.2.0 changelog gives, `ptr` to `ptr()`.

`cargo run` gives the process aborts (exit status 134).

`MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri run` gives `error: Undefined Behavior: constructing invalid value of type std::boxed::Box<Job>: encountered a dangling box (use-after-free)`, in `waker_tables::free_box::<Job>` called from `<waker_tables::RegisteredTask as std::ops::Drop>::drop`.
