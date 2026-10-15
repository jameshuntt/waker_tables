# proofs

Small programs that `CHANGELOG.md` cites and that are not part of the crate. Each folder is its own
Cargo project, pinned to one published version (`waker_tables = "=0.3.1"`) or one git commit, and
named for what it checks and the version it checks. Its README gives the command and what it
printed. Run the command inside the folder.

The outputs were taken with rustc 1.97.1 and Miri from nightly (`miri d3cd04068e`) on
aarch64-apple-darwin. Miri addresses, allocation ids and tag numbers change from run to run.

Each folder has its own `Cargo.toml`, so `cargo test` and `cargo package` for `waker_tables` skip
the programs.
