//! What a device's list and section decide for one row of a mode's file: timed, with the list's
//! `wrong`, or untimed with its reason, the list's `skipped` over a failed section. The page's cell
//! and the CSV's row are both rendered from it.

use super::super::{CaseStatus, FAILED, MODES, SKIPPED, parse_benchmark_section};
use super::{Device, Found, Place, first_line};

/// What a device's list and section decide for one row, before the page or the CSV renders it,
/// so a status is decided once: the list's `skipped` and `wrong` over the section.
pub(crate) enum Decided<'f> {
    Timed {
        found: &'f Found,
        load_us: u64,
        execute_us: u64,
        total_us: u64,
        peak_bytes: Option<u64>,
        pinned: bool,
        /// The list's note where it marks this mode `wrong`.
        wrong: Option<String>,
    },
    /// Not timed: `text` is the reason as the page reads it.
    Untimed {
        outcome: Untimed,
        text: String,
        found: Option<&'f Found>,
    },
    /// No section and no list row.
    Missing,
}

#[derive(Clone, Copy)]
pub(crate) enum Untimed {
    Failed,
    Skipped,
}

impl Untimed {
    pub(crate) fn class(self) -> &'static str {
        match self {
            Untimed::Failed => "off",
            Untimed::Skipped => "skip",
        }
    }

    pub(crate) fn status(self) -> &'static str {
        match self {
            Untimed::Failed => "failed",
            Untimed::Skipped => "skipped",
        }
    }
}

/// One device's decision for a row at `place`'s mode.
pub(crate) fn decided<'f>(place: &Place, device: &Device, found: Option<&'f Found>) -> Decided<'f> {
    let mode = MODES[place.mode].ident();
    // The first is the only one: `parse_device_cases` refuses a case listed twice.
    let case = device.cases.iter().find(|case| {
        case.dataset == place.dataset.1
            && case.sf == place.sf.1
            && case.query.replace('_', "-") == place.query.1
    });
    let timed = found.and_then(|found| parse_benchmark_section(&found.body).map(|s| (found, s)));
    if let Some((found, section)) = timed {
        let total_us = section
            .count("device_us")
            .unwrap_or_else(|| panic!("{}:{}: no device_us", found.rel, found.line));
        let load_us = section
            .count("load_us")
            .unwrap_or_else(|| section.tree_load_us());
        let wrong = case
            .filter(|case| {
                case.status == CaseStatus::Wrong && case.modes.iter().any(|m| *m == mode)
            })
            .map(|case| case.note.clone());
        return Decided::Timed {
            found,
            load_us,
            execute_us: section.count("execute_us").unwrap_or(total_us - load_us),
            total_us,
            peak_bytes: section.count("peak_bytes"),
            pinned: section
                .field("pinned_cache")
                .is_some_and(|cache| cache.split(' ').next() == Some("on")),
            wrong,
        };
    }
    let skipped = case.filter(|case| case.status == CaseStatus::Skipped);
    let (outcome, text) = match (skipped, found) {
        (Some(case), _) => (Untimed::Skipped, format!("skipped: {}", case.note)),
        (None, Some(found)) => match found.body.strip_prefix(FAILED) {
            Some(rest) => (Untimed::Failed, format!("failed: {}", first_line(rest))),
            None => (
                Untimed::Skipped,
                format!(
                    "skipped: {}",
                    first_line(found.body.trim_start_matches(SKIPPED))
                ),
            ),
        },
        (None, None) => return Decided::Missing,
    };
    Decided::Untimed {
        outcome,
        text,
        found,
    }
}
