//! Asserts the layout rules that nothing in rustc checks.
//!
//! Three of the design's claims are the compiler's: an implementation module is private, so
//! naming one from outside its component is `E0603`; a subcomponent declared `mod` is
//! unreachable from another component the same way. This target is for the rest — the claims
//! that hold today because someone wrote the tree that way and nothing would notice if the
//! next change did not.
//!
//! `llm-wiki/coding-style.md` has the rules. The reason each one needs a test rather than a
//! reviewer is recorded at the test that carries it.

// A test target's child modules resolve against tests/ itself, and a file there would be
// another target. The path keeps them under a directory named for this one.
#[path = "test_module_layout/near_miss.rs"]
mod near_miss;
#[path = "test_module_layout/privacy.rs"]
mod privacy;
#[path = "test_module_layout/test_code.rs"]
mod test_code;
#[path = "test_module_layout/tree.rs"]
mod tree;
#[path = "test_module_layout/visibility.rs"]
mod visibility;
#[path = "test_module_layout/walls.rs"]
mod walls;

use std::path::PathBuf;

/// The checkout this test reads.
///
/// Named `repo_root` deliberately: `scripts/build-test.sh` decides which targets may be
/// staged to a remote host by grepping a test's source for `repo_root` or
/// `.github/workflows`, and a target that reads the tree cannot run anywhere the tree is not.
/// Without the name this target is shipped to a CPU host where `sources()` panics on a missing
/// `src/` and the probe cannot find its rlib. One rule rather than two: match the classifier
/// rather than widening it.
pub(crate) fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub(crate) fn src_root() -> PathBuf {
    repo_root().join("peacockdb-core/src")
}
