use crate::executor::{PinnedCache, PinnedCacheSource, PinnedCacheState, PrefillStats};
use crate::test_support::Provenance;

use super::{RunTotals, TRAILER_KEYS, run_section};

fn provenance() -> Provenance {
    Provenance {
        device: "NVIDIA H100 80GB HBM3".to_string(),
        driver: "580.95.05".to_string(),
        cuda: "12.9".to_string(),
        cudf: "26.02.01".to_string(),
        host: "gpu-host-1".to_string(),
    }
}

fn cache_off() -> PinnedCache {
    PinnedCache {
        state: PinnedCacheState::Off,
        source: PinnedCacheSource::Env,
        ..Default::default()
    }
}

/// The five provenance keys close the trailer, in the order the record's heading has them,
/// each with the run's own value. A resumed run keeps a section only when its trailer ends
/// with them, so every other key goes before them.
#[test]
fn trailer_names_its_provenance() {
    let totals = RunTotals {
        run_us: 7,
        device_us: 5,
        load_us: 3,
        peak_bytes: 4096,
    };
    let trailer = run_section(
        &totals,
        &[7, 6, 9],
        "rmm-pool initial=69.0GiB",
        &cache_off(),
        &PrefillStats::default(),
        &provenance(),
    );

    let keys: Vec<&str> = trailer
        .lines()
        .filter_map(|line| line.split_once('=').map(|(key, _)| key))
        .collect();
    assert_eq!(
        keys,
        [
            "run_us",
            "device_us",
            "load_us",
            "execute_us",
            "peak_bytes",
            "runs",
            "build",
            "allocator",
            "pinned_cache",
            "prefill_files",
            "prefill_bytes",
            "prefill_us",
            "device",
            "driver",
            "cuda",
            "cudf",
            "host"
        ],
        "{trailer}"
    );
    assert_eq!(
        keys, TRAILER_KEYS,
        "the keys a resumed run holds a section to"
    );
    for line in [
        "pinned_cache=off source=env",
        "prefill_bytes=0",
        "device=NVIDIA H100 80GB HBM3",
        "driver=580.95.05",
        "cuda=12.9",
        "cudf=26.02.01",
        "host=gpu-host-1",
    ] {
        assert!(
            trailer.lines().any(|l| l == line),
            "no {line:?} in {trailer}"
        );
    }
    assert!(trailer.starts_with("--- run ---\n"), "{trailer}");
}

/// `execute_us` is the device time outside the loads: `device_us − load_us`.
#[test]
fn execute_is_the_device_time_outside_the_loads() {
    let totals = RunTotals {
        run_us: 9,
        device_us: 8,
        load_us: 5,
        peak_bytes: 1,
    };
    let trailer = run_section(
        &totals,
        &[9, 9],
        "x",
        &cache_off(),
        &PrefillStats::default(),
        &provenance(),
    );
    for line in ["load_us=5", "execute_us=3", "peak_bytes=1"] {
        assert!(
            trailer.lines().any(|l| l == line),
            "no {line:?} in {trailer}"
        );
    }
}

/// The prefill's host IO has keys of its own and stays out of `load_us`; `pinned_cache` is the
/// cache's own line with nothing appended.
#[test]
fn the_prefill_is_reported_apart_from_the_load() {
    let end = PinnedCache {
        state: PinnedCacheState::On,
        capacity_bytes: 147_000_000_000,
        fill_us: 900,
        ..Default::default()
    };
    let prefill = PrefillStats {
        files: 8,
        bytes: 13_207_024_435,
        us: 812_345,
        needed: 13_207_024_435,
        limit: 147_000_000_000,
    };
    let totals = RunTotals {
        run_us: 9,
        device_us: 8,
        load_us: 5,
        peak_bytes: 1,
    };
    let trailer = run_section(&totals, &[9, 9], "x", &end, &prefill, &provenance());
    let cache = trailer
        .lines()
        .find(|l| l.starts_with("pinned_cache="))
        .unwrap_or_else(|| panic!("no pinned_cache in {trailer}"));
    assert!(cache.starts_with("pinned_cache=on cap="), "{cache}");
    assert!(cache.ends_with(" source=default"), "{cache}");
    for line in [
        "prefill_files=8",
        "prefill_bytes=13207024435",
        "prefill_us=812345",
        "load_us=5",
    ] {
        assert!(
            trailer.lines().any(|l| l == line),
            "no {line:?} in {trailer}"
        );
    }
}
