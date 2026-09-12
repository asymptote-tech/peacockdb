//! flatc's output, included verbatim: 7,336 lines, and the whole reason `wire/` is drawn
//! where it is.
//!
//! The `allow` is not cosmetic here, which it was while this module was `pub` in `lib.rs`:
//! everything was externally reachable then, so `dead_code` could not fire. Private to one
//! component, every generated type the crate does not name is dead code — hundreds of them
//! — and `unused_imports` and the clippy set are the same story for code nobody wrote.
//! So is `unreachable_pub`: flatc spells every item `pub`, and this module is `mod`.
#![allow(unused_imports, dead_code, unreachable_pub, clippy::all)]

include!(concat!(env!("OUT_DIR"), "/gpu_plan_generated.rs"));
