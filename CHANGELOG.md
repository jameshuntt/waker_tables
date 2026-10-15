# Changelog

`waker_tables` from 0.1.4 to 0.3.1, newest first. The links go to exact lines at exact commits: the code as it was, the change, and the test or run that shows it.

Runs quoted here were done with rustc 1.97.1 and Miri from nightly (`miri d3cd04068e`) on aarch64-apple-darwin, Miri with `-Zmiri-strict-provenance`. Where a check needed a small program that isn't in the repo, the lines that matter are shown inline.

## 0.3.1 (2026-10-15)

Published 07:28 UTC from [`55f7fae`](https://github.com/jameshuntt/waker_tables/commit/55f7faecf77710a8ec94fd0bd355c1106da26e1a) by the CI publish job ([run 38698850135](https://github.com/jameshuntt/waker_tables/actions/runs/38698850135)). The code is [`c907695`](https://github.com/jameshuntt/waker_tables/commit/c907695fd476e2b7132e146a69edde5c3b1cd28b), which we first wrote up as 0.4.0. [`55f7fae`](https://github.com/jameshuntt/waker_tables/commit/55f7faecf77710a8ec94fd0bd355c1106da26e1a) renamed it to 0.3.1 before anything went out, so there is no 0.4.0 on crates.io. A soundness fix plus one new provided method fits a patch, and as a patch it reaches every crate on `waker_tables = "0.3"` with a plain `cargo update`.

### Security

**`derive_arc_unsafe_wakeable!` would call an `unsafe fn` for you.** The macro called your wake method inside the bodies of the `unsafe fn wake` and `unsafe fn wake_by_ref` it generated. The body of an `unsafe fn` is an unsafe context, so if the name you passed was an `unsafe fn` method, it ran without anyone writing `unsafe`. That covers your own `unsafe fn on_wake`, and anything reachable from `Arc<T>` through `Deref`, like `Option::unwrap_unchecked`. Now the method is called from a plain nested `fn call(arc: Arc<$t>)`, so naming an `unsafe fn` is a compile error (E0133). `wake_by_ref` now goes through `wake(clone_ptr(ptr))`, so it hits the same check. A safe method works the same as before.

- Before: [src/lib.rs L119-129 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/src/lib.rs#L119-L129). The calls are on L122 and L128.
- Fix: [src/lib.rs L171-184 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L171-L184), commit [`c907695`](https://github.com/jameshuntt/waker_tables/commit/c907695fd476e2b7132e146a69edde5c3b1cd28b).
- Proof: the two `compile_fail,E0133` doctests at [src/lib.rs L136-163 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L136-L163). We ran the second one (`Empty` derefs to `Option<u8>`, then `derive_arc_unsafe_wakeable!(Empty, unwrap_unchecked)` and `waker_from_arc(Arc::new(Empty)).wake()`) as a program under `#![forbid(unsafe_code)]`. Against 0.3.0 it builds with no warnings, and Miri stops at `<Empty as waker_tables::UnsafeWakeable>::wake at src/lib.rs:122:17` with `Undefined Behavior: entering unreachable code`. Against 0.3.1 it doesn't build: `error[E0133]: call to unsafe function Option::<T>::unwrap_unchecked is unsafe and requires unsafe function or block`. The first one, your own `unsafe fn on_wake`, ran on 0.3.0 with no `unsafe` at any call site, and gives the same E0133 on 0.3.1. `cargo test --doc` on 0.3.1: 9 passed, and the 4 compile_fail doctests pass.
- Since: the first commit, [src/lib.rs L51-63 @ 5891e22](https://github.com/jameshuntt/waker_tables/blob/5891e225403f786f08e7d70d790c4c4e3881a5f9/src/lib.rs#L51-L63). 0.2.0 rewrote the macro ([src/lib.rs L114-143 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L114-L143)) and left the call where it was. The same program written for 0.1.4's `waker_from_arc(arc, &VTABLE)` gets the same Miri error at `src/lib.rs:53:17`.

### Fixed

**A waker and its own clone could fail `will_wake` in release builds.** std's `Waker::will_wake` compares the data pointer and the vtable's address. Since 0.2.0, `waker_from_arc`, `raw_waker_from_ptr` and every clone took the vtable as `&RawWakerVTableOf::<T>::VTABLE`, a reference to an associated const. A const doesn't have one address, and an optimized build can hold two copies of it. So in release, `clone.will_wake(&waker)` came back `false`. No UB and no lost wake, since `will_wake` is best-effort. But anything that keeps a waker and swaps it only when `!will_wake` ends up re-cloning on every poll, and std's own `Waker::clone_from` does exactly that. Now `UnsafeWakeable` has a provided method, `raw_waker_vtable()`. `derive_arc_unsafe_wakeable!` overrides it with one `static` per type, and all three places that build a `RawWaker` take the vtable from it. A hand-written impl keeps the default, which is the old const, so for those `will_wake` stays best-effort. The trait docs and the README say so.

- Before: [src/lib.rs L155-165 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/src/lib.rs#L155-L165) (`raw_waker_vtable` and `raw_clone`) and [src/lib.rs L245-247 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/src/lib.rs#L245-L247) (`raw_waker_from_ptr`).
- Fix: [src/lib.rs L91-104 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L91-L104) (the provided method), [src/lib.rs L197-201 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L197-L201) (the static), [src/lib.rs L219-223 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L219-L223) and [src/lib.rs L304-306 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L304-L306) (the callers), commit [`c907695`](https://github.com/jameshuntt/waker_tables/commit/c907695fd476e2b7132e146a69edde5c3b1cd28b).
- Proof: `clone_adds_one_ref_and_drop_takes_it_back` in [tests/raw_waker.rs L60-76 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/tests/raw_waker.rs#L60-L76) asserts `clone.will_wake(&waker)` on L68. `cargo test --release --test raw_waker` on 0.3.0: `10 passed; 1 failed`, `panicked at tests/raw_waker.rs:68:9: assertion failed: clone.will_wake(&waker)`. On 0.3.1: `11 passed`. The new test `a_waker_and_its_clones_share_one_vtable` in [tests/will_wake.rs L25-46 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/tests/will_wake.rs#L25-L46) compares the addresses directly. It passes in release, and again with `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256`. Debug builds always passed (`11 passed` on 0.2.0), and CI never ran the tests in release ([.github/workflows/ci.yml L23 @ c7379a9](https://github.com/jameshuntt/waker_tables/blob/c7379a9c6d129f1f03baeb91b0e7d0feca5ee961/.github/workflows/ci.yml#L23)), so it never showed up there. CI now also runs `cargo test --release` ([.github/workflows/ci.yml L24 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/.github/workflows/ci.yml#L24)).
- Introduced: we broke this in 0.2.0, commit [`c9e46ab`](https://github.com/jameshuntt/waker_tables/commit/c9e46abee3680578209a8e0333f233ea775832cf), at [src/lib.rs L155-157 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L155-L157). In 0.1.4, `declare_raw_waker_vtable!` declared a real `static` and its clone fn handed out `&$name` ([src/lib.rs L99-103 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L99-L103), [src/lib.rs L120-125 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L120-L125)), so every clone had the same address. The same release test at [`88525d1`](https://github.com/jameshuntt/waker_tables/commit/88525d162e3bdafc716d059437d6037a1f91e83c), the commit right before, passes (`8 passed`). At [`c9e46ab`](https://github.com/jameshuntt/waker_tables/commit/c9e46abee3680578209a8e0333f233ea775832cf) it fails on L68.

**`examples/block_on.rs`: `Ticks` ignored its tick count.** `Ticks::start(ticks, ..)` ran the timer `ticks` times, but `poll` checked `state.fired == 3`. So `Ticks::start(2, ..)` hung forever and `Ticks::start(4, ..)` returned 3. This was only in the example; the library wasn't involved. Now `TickState` keeps `ticks` and `poll` checks `state.fired == state.ticks`.

- Before: [examples/block_on.rs L50-85 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/examples/block_on.rs#L50-L85). The check is on L78.
- Fix: [examples/block_on.rs L50-86 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/examples/block_on.rs#L50-L86).
- Proof: `ticks_returns_its_own_count` in [examples/block_on.rs L95-122 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/examples/block_on.rs#L95-L122) runs 0, 1, 2, 3, 4 and 7 ticks with a 5 second deadline. With the old `== 3` check put back it fails: `left: [None, None, None, Some(3), Some(3), Some(3)]`, `right: [Some(0), Some(1), Some(2), Some(3), Some(4), Some(7)]`. On 0.3.1: `1 passed`.
- Introduced: 0.2.0, commit [`c9e46ab`](https://github.com/jameshuntt/waker_tables/commit/c9e46abee3680578209a8e0333f233ea775832cf), when the example was added.

**The README said `UnsafeWakeableVTable` is three function pointers.** It's five: `poll_fn`, `wake_fn`, `drop_fn`, `type_id`, `type_name`. On 0.3.0, `size_of::<UnsafeWakeableVTable>()` is 40 bytes (5 words) and `size_of::<UnsafeWakeablePtr>()` is 16 (2 words). Now the README says five, and a doctest asserts both sizes.

- Before: [readme.md L324-325 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/readme.md?plain=1#L324-L325).
- Fix: [readme.md L328-339 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/readme.md?plain=1#L328-L339). The code block runs as a doctest through `ReadmeDoctests`.
- Introduced: 0.2.0 added `type_id` and `type_name` ([src/lib.rs L295-306 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L295-L306)) and didn't update that line.

### Added

- `UnsafeWakeable::raw_waker_vtable()`, a provided method ([src/lib.rs L91-104 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L91-L104)). The trait's `# Safety` list has one more line: an override has to return a `'static` copy of the default ([src/lib.rs L61-62 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L61-L62)).
- `tests/will_wake.rs`. Besides the address test, `a_hand_written_impl_keeps_the_default_vtable` ([tests/will_wake.rs L48-95 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/tests/will_wake.rs#L48-L95)) checks wakes and refcounts for a manual impl and doesn't assert `will_wake`.
- README: a `will_wake` entry under Common pitfalls ([readme.md L362-366 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/readme.md?plain=1#L362-L366)), the rule that the wake method has to be safe, and notes for this version.

### Changed

- `derive_arc_unsafe_wakeable!` needs a sized type. `struct Dst([u8])` with the macro built on 0.3.0 and now fails with E0277. No waker could be made from an unsized type before either, since `waker_from_arc<T>` needs `T: Sized`.
- `declare_raw_waker_vtable!` docs: the static it declares has the same four functions as the vtable wakers carry, at its own address ([src/lib.rs L240-244 @ c907695](https://github.com/jameshuntt/waker_tables/blob/c907695fd476e2b7132e146a69edde5c3b1cd28b/src/lib.rs#L240-L244)).

## 0.3.0 (2026-10-15)

Published 01:09 UTC from [`c0b70f3`](https://github.com/jameshuntt/waker_tables/commit/c0b70f39cf3c2221b3d67684a1f73e957ecf863e) by the CI publish job ([run 38663996243](https://github.com/jameshuntt/waker_tables/actions/runs/38663996243)).

### Security

**A hand-built `UnsafeWakeableVTable` could lie about its type.** 0.2.0 gave the task vtable a `type_id` so `RegisteredTask::new` could refuse a key registered for another type. But every field was `pub`, so safe code could build a vtable with one type's functions and another type's `type_id`:

```rust
static LIAR: UnsafeWakeableVTable = UnsafeWakeableVTable {
    type_id: small_type_id, // fn() -> TypeId { TypeId::of::<Small>() }
    type_name: small_name,
    ..UnsafeWakeableVTable::of::<Big>()
};
```

Register `LIAR` under a key, and `RegisteredTask::new(Box::new(Small(0)), key)` passes the check. The next `poll` runs `Big`'s `poll` on a `Small`. The task types need their `unsafe impl`, but the only `unsafe` call is `task.poll()`, and its `# Safety` section only asks for the task's own `poll` contract. `Small`'s `poll` has nothing to ask. So the hole was in the crate, not in the caller. Now the fields are private, and the only ways to build one are `UnsafeWakeableVTable::of::<T>()` and `derive_unsafe_wakeable_vtable!`. `type_name()` reads the name.

- Before: [src/lib.rs L291-306 @ 81d8b60](https://github.com/jameshuntt/waker_tables/blob/81d8b60829d2fde5413de32021a7e9475b8e21e1/src/lib.rs#L291-L306).
- Fix: [src/lib.rs L291-316 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/src/lib.rs#L291-L316) and [src/lib.rs L350-353 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/src/lib.rs#L350-L353) (`type_name()`), commit [`c0b70f3`](https://github.com/jameshuntt/waker_tables/commit/c0b70f39cf3c2221b3d67684a1f73e957ecf863e).
- Proof: the `compile_fail,E0451` doctest at [src/lib.rs L297-309 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/src/lib.rs#L297-L309) is that vtable. Against 0.2.3 under Miri, `LIAR` plus one `unsafe { task.poll() }` gives `Undefined Behavior: constructing invalid value of type &mut Big: encountered a dangling reference` in `waker_tables::task_poll::<Big>` at `src/lib.rs:310`. Against 0.3.0: `error[E0451]: fields poll_fn, wake_fn, drop_fn, type_id and type_name of struct UnsafeWakeableVTable are private`.
- Introduced: 0.2.0, commit [`c9e46ab`](https://github.com/jameshuntt/waker_tables/commit/c9e46abee3680578209a8e0333f233ea775832cf). It added the check and made the two new fields `pub`, next to the three function pointers that were already `pub` in 0.1.4 ([src/lib.rs L184-189 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L184-L189)).

### Changed

- `UnsafeWakeableVTable`'s fields are private. Code that read `vtable.poll_fn` or called `(vtable.type_name)()` won't build. Use `vtable.type_name()` and `vtable.is_for::<T>()`. That's a breaking change, so the version went to 0.3.0.
- `vtable_knows_its_type` uses `type_name()` ([tests/task_vtable.rs L255-260 @ c0b70f3](https://github.com/jameshuntt/waker_tables/blob/c0b70f39cf3c2221b3d67684a1f73e957ecf863e/tests/task_vtable.rs#L255-L260)).
- README: `waker_tables = "0.3"` and notes for this version.

## 0.2.3 (2026-10-15)

Published 00:48 UTC from [`81d8b60`](https://github.com/jameshuntt/waker_tables/commit/81d8b60829d2fde5413de32021a7e9475b8e21e1) by the CI publish job ([run 38662001241](https://github.com/jameshuntt/waker_tables/actions/runs/38662001241)). No code changes: `src/` and `tests/` are the same as 0.2.0.

### Fixed

**LICENSE-APACHE still had the template's copyright line.** 0.2.2 shipped `Copyright [yyyy] [name of copyright owner]`. Now it says `Copyright 2026 1o LLC`, same as LICENSE-MIT.

- Before: [LICENSE-APACHE L190 @ a2b4c40](https://github.com/jameshuntt/waker_tables/blob/a2b4c40721ff92d7e4220b199a4b1737e64057ca/LICENSE-APACHE#L190). Fix: [LICENSE-APACHE L190 @ 18c88e8](https://github.com/jameshuntt/waker_tables/blob/18c88e8031762d534a873e9ed94d94b2e72f05f1/LICENSE-APACHE#L190), commit [`18c88e8`](https://github.com/jameshuntt/waker_tables/commit/18c88e8031762d534a873e9ed94d94b2e72f05f1). LICENSE-MIT: [LICENSE-MIT L3 @ a2b4c40](https://github.com/jameshuntt/waker_tables/blob/a2b4c40721ff92d7e4220b199a4b1737e64057ca/LICENSE-MIT#L3).

**Cargo.lock was a version behind.** [`18c88e8`](https://github.com/jameshuntt/waker_tables/commit/18c88e8031762d534a873e9ed94d94b2e72f05f1) bumped `Cargo.toml` to 0.2.3 and left `Cargo.lock` on 0.2.2 ([Cargo.lock L226-228 @ 18c88e8](https://github.com/jameshuntt/waker_tables/blob/18c88e8031762d534a873e9ed94d94b2e72f05f1/Cargo.lock#L226-L228)). [`81d8b60`](https://github.com/jameshuntt/waker_tables/commit/81d8b60829d2fde5413de32021a7e9475b8e21e1) fixed the lock ([Cargo.lock L226-228 @ 81d8b60](https://github.com/jameshuntt/waker_tables/blob/81d8b60829d2fde5413de32021a7e9475b8e21e1/Cargo.lock#L226-L228)), and CI's test and MSRV jobs now run with `--locked` ([.github/workflows/ci.yml L23 @ 81d8b60](https://github.com/jameshuntt/waker_tables/blob/81d8b60829d2fde5413de32021a7e9475b8e21e1/.github/workflows/ci.yml#L23), [.github/workflows/ci.yml L38 @ 81d8b60](https://github.com/jameshuntt/waker_tables/blob/81d8b60829d2fde5413de32021a7e9475b8e21e1/.github/workflows/ci.yml#L38)), so a stale lock fails CI. The published 0.2.3 came from 81d8b60 and has the right lock.

## 0.2.2 (2026-10-15)

Published 00:43 UTC from [`a2b4c40`](https://github.com/jameshuntt/waker_tables/commit/a2b4c40721ff92d7e4220b199a4b1737e64057ca) by the CI publish job ([run 38661580152](https://github.com/jameshuntt/waker_tables/actions/runs/38661580152)). No code changes.

### Added

- LICENSE-APACHE and LICENSE-MIT, commit [`a2b4c40`](https://github.com/jameshuntt/waker_tables/commit/a2b4c40721ff92d7e4220b199a4b1737e64057ca). `Cargo.toml` has said `license = "MIT OR Apache-2.0"` since 0.1.0 ([Cargo.toml L6 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/Cargo.toml#L6)), but no package up to 0.2.1 had the license text in it.

### Fixed

- The README's License section said `MIT & Apache-2.0` ([readme.md L395-397 @ 2659d29](https://github.com/jameshuntt/waker_tables/blob/2659d29ddb0c9790765a913e62ecece5b56be9c3/readme.md?plain=1#L395-L397)). The crate is MIT or Apache-2.0, your pick. Now it says that and links both files ([readme.md L395-397 @ a2b4c40](https://github.com/jameshuntt/waker_tables/blob/a2b4c40721ff92d7e4220b199a4b1737e64057ca/readme.md?plain=1#L395-L397)).
- Left for 0.2.3: the Apache file's copyright line was still the template.

## 0.2.1 (2026-10-15)

Published 00:41 UTC from [`2659d29`](https://github.com/jameshuntt/waker_tables/commit/2659d29ddb0c9790765a913e62ecece5b56be9c3) by the CI publish job ([run 38661324622](https://github.com/jameshuntt/waker_tables/actions/runs/38661324622)). No library changes.

### Added

- `examples/admission_executor.rs` ([examples/admission_executor.rs L1-137 @ e016cc2](https://github.com/jameshuntt/waker_tables/blob/e016cc2922836bcf42c30ecac7f1873cf16cfa89/examples/admission_executor.rs#L1-L137)), commit [`e016cc2`](https://github.com/jameshuntt/waker_tables/commit/e016cc2922836bcf42c30ecac7f1873cf16cfa89). An executor that runs five jobs with at most two in flight. The limit is uh-tom-mick's `AtomicAdmission`: a job that finds the gate full parks its waker, and a finishing job wakes the next one. The run queue is fed by wakers from `waker_from_arc`, and at the end the gate has to report drained. Last line of `cargo run --example admission_executor`: `5 jobs done, at most 2 in flight, gate drained`.
- `uh-tom-mick = "0.1"` as a dev-dependency only ([Cargo.toml L16-17 @ e016cc2](https://github.com/jameshuntt/waker_tables/blob/e016cc2922836bcf42c30ecac7f1873cf16cfa89/Cargo.toml#L16-L17)). Nothing changes for crates that depend on `waker_tables`.
- CI runs the example normally and under Miri ([.github/workflows/ci.yml L31 @ e016cc2](https://github.com/jameshuntt/waker_tables/blob/e016cc2922836bcf42c30ecac7f1873cf16cfa89/.github/workflows/ci.yml#L31), [.github/workflows/ci.yml L57-59 @ e016cc2](https://github.com/jameshuntt/waker_tables/blob/e016cc2922836bcf42c30ecac7f1873cf16cfa89/.github/workflows/ci.yml#L57-L59)).

## 0.2.0 (2026-10-15)

Published 00:35 UTC from [`c7379a9`](https://github.com/jameshuntt/waker_tables/commit/c7379a9c6d129f1f03baeb91b0e7d0feca5ee961) by the CI publish job ([run 38660809074](https://github.com/jameshuntt/waker_tables/actions/runs/38660809074)), the first version published that way. The work is [`88525d1`](https://github.com/jameshuntt/waker_tables/commit/88525d162e3bdafc716d059437d6037a1f91e83c) (`RegisteredTask` frees its task, tests for every public piece) and [`c9e46ab`](https://github.com/jameshuntt/waker_tables/commit/c9e46abee3680578209a8e0333f233ea775832cf) (everything else), then [`d3cc3a4`](https://github.com/jameshuntt/waker_tables/commit/d3cc3a4d1c9dd1047acbb1c19f63319ced49b5a4) (`rust-version`) and [`c7379a9`](https://github.com/jameshuntt/waker_tables/commit/c7379a9c6d129f1f03baeb91b0e7d0feca5ee961) (CI).

### Security

**`waker_from_arc` took any vtable.** In 0.1.4, `waker_from_arc(arc, vtable)` was a safe fn that paired your `Arc<T>` with whatever `&'static RawWakerVTable` you gave it. Give it another type's vtable and the waker runs that type's functions on your data, with no `unsafe` in your code. Now it's `waker_from_arc(arc)`: the vtable is built from `T`'s own `UnsafeWakeable` functions (`RawWakerVTableOf<T>`), so a waker can only run its own type's code. `raw_waker_from_ptr` lost its vtable argument the same way.

- Before: [src/lib.rs L153-168 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L153-L168).
- Fix: [src/lib.rs L145-180 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L145-L180) and [src/lib.rs L241-254 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L241-L254), commit [`c9e46ab`](https://github.com/jameshuntt/waker_tables/commit/c9e46abee3680578209a8e0333f233ea775832cf).
- Proof: this program has `#![forbid(unsafe_code)]` and no `unsafe` of its own. `Small` and `Big` each get `derive_arc_unsafe_wakeable!`, and `Big` also gets `declare_raw_waker_vtable!`:

  ```rust
  struct Small(u8);
  struct Big([u64; 8]);
  waker_from_arc(Arc::new(Small(1)), &BIG_VTABLE).wake();
  ```

  Against 0.1.4 under Miri: `Undefined Behavior: constructing invalid value of type &alloc::sync::ArcInner<Big>: encountered a dangling reference`, inside `Big::on_wake`. Against 0.2.0 it doesn't build: `error[E0061]: this function takes 1 argument but 2 arguments were supplied`. `each_type_runs_its_own_functions` ([tests/raw_waker.rs L205-221 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/tests/raw_waker.rs#L205-L221)) checks that two types' wakers each run their own `on_wake`.

**`RegisteredTask::new` didn't check the key's type.** 0.1.4's README said to use `RegisteredTask` to keep pointer and vtable together ([readme.md L331-332 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/readme.md?plain=1#L331-L332)). But `new` took whatever vtable was registered under the key. Register `Big`'s vtable, hand `new` a `Box<Small>`, and the next `poll` runs `Big::poll` on a `Small`. The `unsafe` on `RegisteredTask::poll` only asked for exclusive access, which the caller had. And `key` and `ptr` were `pub`, so the pair could be changed after `new`. Now the vtable records `type_id` and `type_name`, `new` asserts `vtable.is_for::<T>()` and panics with both names, and the fields are private behind `key()` and `ptr()`.

- Before: [src/lib.rs L336-365 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L336-L365).
- Fix: [src/lib.rs L557-586 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L557-L586), [src/lib.rs L323-339 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L323-L339) (`of`, `is_for`), [src/lib.rs L526-542 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L526-L542) (private fields, with a `compile_fail,E0616` doctest).
- Proof: `key_for_another_type_panics` and `key_for_another_type_panics_without_leaking_the_task` in [tests/task_vtable.rs L221-239 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/tests/task_vtable.rs#L221-L239). The case above against 0.1.4 under Miri:

  ```rust
  GLOBAL_VTABLE_REGISTRY.register(key, &BIG_VTABLE);
  let mut task = RegisteredTask::new(Box::new(Small(0)), key);
  let _ = unsafe { task.poll() };
  ```

  gives `Undefined Behavior: constructing invalid value of type &mut Big: encountered a dangling reference`, from `RegisteredTask::poll` at `src/lib.rs:371`. Against 0.2.0 it panics in `new`: `vtable for key VTableKey { segment: "probe", kind: "big", version: 1 } is for key_mismatch::Big, not key_mismatch::Small`.
- Left open: the new `type_id` field was `pub`, so a hand-built vtable could still lie. Closed in 0.3.0.

### Fixed

**`RegisteredTask` never freed its task.** `new` did `Box::into_raw(task)` and nothing turned it back into a `Box`. There was no `Drop`, and `drop_in_place`, documented as dropping the task, only calls the task's own `drop_in_place` hook. Every `RegisteredTask` leaked its box. Now `new` keeps a `free_box::<T>` fn next to the pointer, and `Drop` calls it once. `drop_in_place` is documented as the cleanup hook, and freeing happens on drop.

- Before: [src/lib.rs L340-382 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L340-L382).
- Fix: [src/lib.rs L372-410 @ 88525d1](https://github.com/jameshuntt/waker_tables/blob/88525d162e3bdafc716d059437d6037a1f91e83c/src/lib.rs#L372-L410), commit [`88525d1`](https://github.com/jameshuntt/waker_tables/commit/88525d162e3bdafc716d059437d6037a1f91e83c). With SAFETY comments: [src/lib.rs L537-555 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L537-L555).
- Proof: 0.1.4's own test `registry_and_registered_task_basic_flow` ([src/lib.rs L415-437 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L415-L437)) under Miri: `error: memory leaked: alloc40322 (Rust heap, size: 16, align: 8)`, allocated at `src/lib.rs:425:20`, the `Box::new(DummyTask { .. })`. New tests count drops: `registered_task_runs_and_frees_once`, `drop_in_place_then_drop_frees_once` and `many_tasks_each_freed` (20 tasks, 20 drops) in [tests/task_vtable.rs L94-147 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/tests/task_vtable.rs#L94-L147). `cargo +nightly miri test` on 0.2.0: 1, 11 and 13 tests and 9 doctests pass, no leak.

**`SmartWakerPtr` leaked its `Arc` ref when dropped.** It had no `Drop`; the ref only came back through `to_arc`. Now `Drop` gives the ref back, and `to_arc` holds `self` in `ManuallyDrop` so it isn't given back twice. `to_arc` and `to_cloned_arc` are safe fns now: the field is private, so the pointer always came from `from_arc`.

- Before: [src/lib.rs L129-151 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L129-L151).
- Fix: [src/lib.rs L206-239 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L206-L239).
- Proof: `smart_waker_ptr_drop_gives_the_ref_back` ([tests/raw_waker.rs L170-177 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/tests/raw_waker.rs#L170-L177)). Dropping one `SmartWakerPtr::from_arc(signal.clone())` leaves `Arc::strong_count(&signal)` at 2 on 0.1.4 and 1 on 0.2.0.

**Two `declare_raw_waker_vtable!` in one module didn't compile.** The macro put four helper fns with fixed names (`__waker_clone`, `__waker_wake`, `__waker_wake_by_ref`, `__waker_drop`) into your module, so a second one in the same module failed with `error[E0428]: the name __waker_clone is defined multiple times`, and the same for the other three. Now the macro is one line, a `pub static` copied from `RawWakerVTableOf::<$t>::VTABLE`, and two in one module build fine.

- Before: [src/lib.rs L96-127 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/src/lib.rs#L96-L127).
- Fix: [src/lib.rs L199-204 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L199-L204).
- Proof: no test in the repo for this. We built two invocations in one module against both versions; the errors above are from 0.1.4, and 0.2.0 builds it clean.

**The README examples leaked their tasks.** Examples 2 and 4 put a `Box` behind an `UnsafeWakeablePtr`, called `drop_in_place`, and never freed it. Example 3 called `etl.ptr.vtable.poll_fn(etl.ptr.data)` through the `pub` fields. Now 2 and 4 free the box with `Box::from_raw`, 3 goes through `RegisteredTask` and shows a refused key, and the README's code blocks run as doctests (`ReadmeDoctests`, [src/lib.rs L632-635 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L632-L635)).

- Before: [readme.md L178-192 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/readme.md?plain=1#L178-L192), [readme.md L242-244 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/readme.md?plain=1#L242-L244), [readme.md L292-311 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/readme.md?plain=1#L292-L311).
- Fix: [readme.md L175-191 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/readme.md?plain=1#L175-L191), [readme.md L232-254 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/readme.md?plain=1#L232-L254), [readme.md L295-317 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/readme.md?plain=1#L295-L317).

### Changed

What breaks when you move up from 0.1.4:

- `waker_from_arc(arc)` and `raw_waker_from_ptr(ptr)` take no vtable. `declare_raw_waker_vtable!` still builds, for 0.1 code.
- `RegisteredTask`'s fields are private. Use `key()` and `ptr()`.
- `UnsafeWakeableVTable` has two more fields, so a struct literal with the old three won't build. Use `UnsafeWakeableVTable::of::<T>()` or `derive_unsafe_wakeable_vtable!`, which now expands to `of::<$t>()` ([src/lib.rs L427-432 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L427-L432)).
- `SmartWakerPtr::to_arc` and `to_cloned_arc` aren't `unsafe` anymore.

And the rest:

- `derive_arc_unsafe_wakeable!` ([src/lib.rs L114-143 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L114-L143)): `clone_ptr` and `drop_ptr` use `Arc::increment_strong_count` and `decrement_strong_count` instead of `from_raw`, `clone`, `forget`. `wake_by_ref` holds the borrowed `Arc` in `ManuallyDrop`. Paths are `::std::...`, and each unsafe op has its own `unsafe {}` and SAFETY comment. The refcounts are the same; `tests/raw_waker.rs` checks `strong_count` after every step.
- `VTableRegistry::register` and `get` take the map out of a poisoned lock instead of panicking ([src/lib.rs L507 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L507), [src/lib.rs L513 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L513)).
- The crate is `#![deny(missing_docs, unsafe_op_in_unsafe_fn)]` ([src/lib.rs L36 @ c9e46ab](https://github.com/jameshuntt/waker_tables/blob/c9e46abee3680578209a8e0333f233ea775832cf/src/lib.rs#L36)), and the `unsafe` fns and trait methods got `# Safety` sections.
- `rust-version = "1.85"` ([Cargo.toml L5 @ d3cc3a4](https://github.com/jameshuntt/waker_tables/blob/d3cc3a4d1c9dd1047acbb1c19f63319ced49b5a4/Cargo.toml#L5)), the first Rust with edition 2024.

### Added

- `UnsafeWakeableVTable::of::<T>()` and `is_for::<T>()`. `RegisteredTask::key()`, `ptr()` and `Debug`. `Default` for `VTableRegistry`. `Debug` for `UnsafeWakeableVTable` and `UnsafeWakeablePtr` prints the task type's name.
- `tests/raw_waker.rs` and `tests/task_vtable.rs` ([`88525d1`](https://github.com/jameshuntt/waker_tables/commit/88525d162e3bdafc716d059437d6037a1f91e83c)), covering every public piece. They check every `Arc` count and every task drop, and they run under Miri.
- `examples/block_on.rs`, a whole executor on `waker_from_arc` woken by a timer thread, and `examples/task_table.rs`, a round-robin table of `RegisteredTask`s of two types with a third spawn that gets refused.
- Crate-level docs with a doctest. README sections for Examples and Testing, and notes for this version.
- CI, [.github/workflows/ci.yml L1-75 @ c7379a9](https://github.com/jameshuntt/waker_tables/blob/c7379a9c6d129f1f03baeb91b0e7d0feca5ee961/.github/workflows/ci.yml#L1-L75): tests, doctests, clippy with `-D warnings -W clippy::undocumented_unsafe_blocks`, docs with `-D warnings`, both examples, tests on Rust 1.85, Miri with `-Zmiri-strict-provenance -Zmiri-many-seeds=0..8`, and on a `v*` tag, `cargo publish` through crates.io trusted publishing once all of that passes.

## 0.1.4 (2026-02-25)

Published 10:04 UTC by hand with `cargo publish` (crates.io lists jameshuntt as publisher; there was no CI yet). The `.cargo_vcs_info.json` inside the package names [`e5b534e`](https://github.com/jameshuntt/waker_tables/commit/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847), and the package's files match that commit except `Cargo.toml`, which cargo rewrites on publish. The `.crate`'s sha256, `e0ca1ceced840955d7a103b17e65a581410574d44df225c39990ba55b0b09d44`, matches the checksum crates.io lists.

This is the code everything above starts from. Problems later found in it are under 0.2.0, 0.3.0 and 0.3.1, with links back to its lines.

### Changed

- README License line went from `MIT / Apache-2.0 (pick whatever your repo uses)` to `MIT & Apache-2.0` ([readme.md L355-357 @ e5b534e](https://github.com/jameshuntt/waker_tables/blob/e5b534ed5b6cff4c7ab98e9e43b28d9a86b77847/readme.md?plain=1#L355-L357)). Apart from the version numbers in `Cargo.toml` and `Cargo.lock`, that's the only change since [`9f385a7`](https://github.com/jameshuntt/waker_tables/commit/9f385a7cb3c4e68c5eac357bcf6aead6f9c0944a), where `Cargo.toml` went to 0.1.3.
