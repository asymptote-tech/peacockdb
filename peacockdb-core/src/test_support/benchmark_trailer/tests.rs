use crate::test_support::Provenance;

use super::run_section;

/// The five provenance keys close the trailer, in the order the record's heading has them,
/// each with the run's own value.
#[test]
fn trailer_names_its_provenance() {
    let provenance = Provenance {
        device: "NVIDIA H100 80GB HBM3".to_string(),
        driver: "580.95.05".to_string(),
        cuda: "12.9".to_string(),
        cudf: "26.02.01".to_string(),
        host: "gpu-host-1".to_string(),
    };
    let trailer = run_section(7, 5, &[7, 6, 9], "rmm-pool initial=69.0GiB", &provenance);

    let keys: Vec<&str> = trailer
        .lines()
        .filter_map(|line| line.split_once('=').map(|(key, _)| key))
        .collect();
    assert_eq!(
        keys,
        [
            "run_us",
            "device_us",
            "runs",
            "build",
            "allocator",
            "device",
            "driver",
            "cuda",
            "cudf",
            "host"
        ],
        "{trailer}"
    );
    for line in [
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
