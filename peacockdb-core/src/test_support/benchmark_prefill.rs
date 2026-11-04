//! The benchmark's pinned host cache: which cases this process runs, and what the prefill and
//! the rounds must show for a case's numbers to have read pinned buffers only.
//!
//! Pure, so it compiles and is tested in every shape; `corpus_benchmark` is its caller.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::executor::{PinnedCache, PrefillError, PrefillStats};

use super::{BenchmarkCase, NOT_TIMED};

#[cfg(test)]
mod tests;

/// libtest's options that take their value as the next argument. Their `--flag=value` form is
/// one argument and consumes nothing more.
const VALUED_FLAGS: [&str; 7] = [
    "--test-threads",
    "--format",
    "--color",
    "--logfile",
    "-Z",
    "--skip",
    "--shuffle-seed",
];

/// What one process prefilled: the read it made, and the union of its cases' files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessPrefill {
    pub(crate) stats: PrefillStats,
    pub(crate) files: BTreeSet<PathBuf>,
}

/// The query file a case plans: the macro's own spelling, so `aggregate_groupby` plans
/// `aggregate-groupby.sql`.
pub(crate) fn case_query_file(case: &BenchmarkCase) -> String {
    case.query.replace('_', "-")
}

/// The cases this libtest process will run, by libtest's rules over the name the macro gives a
/// case, `bench_<dataset>_sf<sf>_<query>_<mode>`: a positional argument is a filter and a name
/// matching any one runs (none = all); `--skip` drops a match; `--exact` makes both equalities.
pub(crate) fn selected_benchmark_cases<'a>(
    args: &[String],
    cases: impl IntoIterator<Item = &'a BenchmarkCase>,
) -> Vec<&'a BenchmarkCase> {
    let mut filters: Vec<&str> = Vec::new();
    let mut skips: Vec<&str> = Vec::new();
    let mut exact = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exact" => exact = true,
            "--skip" => skips.extend(args.next().map(String::as_str)),
            flag if VALUED_FLAGS.contains(&flag) => {
                args.next();
            }
            flag if flag.starts_with("--skip=") => skips.push(&flag["--skip=".len()..]),
            flag if flag.starts_with('-') => {}
            filter => filters.push(filter),
        }
    }
    let matches = |name: &str, pattern: &str| match exact {
        true => name == pattern,
        false => name.contains(pattern),
    };
    cases
        .into_iter()
        .filter(|case| case.mode != NOT_TIMED)
        .filter(|case| {
            let name = format!(
                "bench_{}_sf{}_{}_{}",
                case.dataset, case.sf, case.query, case.mode
            );
            (filters.is_empty() || filters.iter().any(|filter| matches(&name, filter)))
                && !skips.iter().any(|skip| matches(&name, skip))
        })
        .collect()
}

/// A case's rounds read pinned buffers only if the cache neither filled, fell back, reloaded
/// nor evicted between the snapshot before its warm-up and the one after its last run.
pub(crate) fn rounds_were_pinned(before: &PinnedCache, after: &PinnedCache) -> Result<(), String> {
    let moved = |count: fn(&PinnedCache) -> u64| count(after) - count(before);
    let (fills, fallbacks, reloads, evictions) = (
        moved(|cache| cache.fills),
        moved(|cache| cache.fallbacks),
        moved(|cache| cache.reloads),
        moved(|cache| cache.evictions),
    );
    match fills + fallbacks + reloads + evictions {
        0 => Ok(()),
        _ => Err(format!(
            "pinned cache: {fills} fills, {fallbacks} fallbacks, {reloads} reloads, \
             {evictions} evictions during the rounds"
        )),
    }
}

/// The failed section's reason when the process's prefill refused. An off cache is none: a
/// configuration with the cache off measures from the path on purpose.
pub(crate) fn prefill_reason(prefill: &Result<ProcessPrefill, PrefillError>) -> Option<String> {
    match prefill {
        Err(error @ (PrefillError::DoesNotFit { .. } | PrefillError::Io { .. })) => {
            Some(format!("prefill: {error}"))
        }
        Err(PrefillError::Off) | Ok(_) => None,
    }
}

/// The first of a case's files the prefill did not make resident: its loads would fill from
/// disk inside the rounds.
pub(crate) fn unprefilled(
    case_files: &BTreeSet<PathBuf>,
    prefilled: &BTreeSet<PathBuf>,
) -> Option<PathBuf> {
    case_files.difference(prefilled).next().cloned()
}
