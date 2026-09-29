
# Pre-production system hardening tasks

<a id="t13"></a>
### #13 — Hermetic builds: system-library whitelist + CI audit
`ld` silently prefers system libs over the conda env (seen as
`libarrow.so.2300: undefined reference to curl_easy_getinfo@CURL_OPENSSL_4`). Whitelist
glibc/libgcc_s/libcuda only; everything else from `$CUDF_ROOT`. Enforce via CMake
find-root pinning, build.rs link-search order, and a post-link `ldd` audit that fails CI.

<a id="t196"></a>
### #196 — the table registrar's non-parquet guard does nothing, so a stray file panics
`read_table` in `lib.rs` opens with `if path.extension() != Some("parquet") { () }` — the
condition is computed and discarded, so a non-parquet entry falls through to
`ListingTableUrl::parse` and four `unwrap`s. The caller's `let Ok(..) else { continue }` says
the intent was an `Err` there.

Nothing in the tree provokes it: every dataset dir holds parquet and nothing else, and
`.duckdb_cache/` is a sibling rather than a child. The CLI is what makes it reachable by a
user, since it registers whatever directory it is pointed at. The fix is the `return Err(())`
the shape already asks for, with a case putting a non-parquet file in the dir.

<a id="t169"></a>
### #169 — a recipe plan is a chain, so its depth is its length, and the verifier caps depth

fb children are nested, so the recipe plan for a query is one deep chain rather than a broad
tree: depth equals the number of addressed nodes plus its stubs. The C++ verifier caps depth at
1024, and the Rust reader had to have the same limit raised to parse what it had just written.

Deepest today is tpcds at `tp4-rowgroup`, seq 382, so nothing is near it. What makes it worth
recording is the failure mode: a plan of roughly a thousand addressed nodes fails at
`begin_plan` — the whole query refused before a call is made — rather than degrading at the call
that overruns.

The fix belongs here rather than in the verifier. Raising a limit to fit a shape that grows
without bound only moves the number; splitting one recipe plan into several, loaded in turn, ends
it. Not urgent at a factor of two and a half of headroom, and it wants measuring before it wants
designing: nothing yet says a thousand-node plan is a shape this mode should produce.

<a id="t128"></a>
### #128 — Doctests run nowhere, and the meta guard cannot see them
No step in `pipeline.yml` passes `--doc`, and `test_ci_coverage.rs` enumerates `--test`
targets plus `--lib`, so a doctest is invisible to the guard whose whole job is finding
targets CI does not run. The crate has none today: the one it had documented an entry point
that no longer exists.

There is now one pipeline to document, and it is three calls in a fixed order —
`planner::plan` for the tree, `wire::attach_recipes` where a device is involved, and
`executor::run` over a backend. `peacockdb/src/main.rs` is the only place that
sequence is written down, and a reader of the crate meets the three functions separately. A
doctest on the entry it documents is the natural fix and the reason to close both halves at
once: write it, run `cargo test --features rust-only -p peacockdb-core --doc` in the
dataset-matrix tier, and teach the guard that `--doc` is a target class it must see named
(the `--lib` check at `line_runs_lib_tests` is the pattern).

