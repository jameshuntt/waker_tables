# header-key-0.1.4

`waker_tables = "=0.1.4"`. `transparent_wrapper`: a `#[repr(transparent)]` wrapper given to `RegisteredTask::new` under its inner type's key. `repr_c_header`: two `#[repr(C)]` task types that start with a `Header`, both under the header's key.

`cargo run --bin transparent_wrapper` gives `Wrapped under Inner's key: accepted, poll = Pending`.

`cargo run --bin repr_c_header` gives `Fetch and Parse under Header's key: accepted, polls = (Pending, Pending)`. Under Miri with `MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-ignore-leaks"` both run clean; without `-Zmiri-ignore-leaks` the only error is the leak every 0.1.4 `RegisteredTask` has.
