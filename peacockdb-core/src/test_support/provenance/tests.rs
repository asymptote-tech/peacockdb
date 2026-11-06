use std::path::Path;

use crate::test_support::testdata_root;

use super::{cuda_version, device_slug, driver_release, driver_release_of};

/// The directory a device's results go to is its name made a path segment, the same one on
/// every run. The examples are `testdata/device_slugs.tsv`, which `record.py`'s copy of
/// the rule is held to as well, so the two cannot drift apart unnoticed.
#[test]
fn device_slug_is_stable() {
    let path = testdata_root().join("device_slugs.tsv");
    let table =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rows: Vec<(&str, &str)> = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_once('\t')
                .unwrap_or_else(|| panic!("{}: {line:?} is not name<TAB>slug", path.display()))
        })
        .collect();
    assert!(
        rows.iter()
            .any(|&row| row == ("NVIDIA H100 80GB HBM3", "h100-80gb-hbm3"))
            && rows.iter().any(|&row| row == ("NVIDIA H200", "h200")),
        "{} lost the two names the plan pins",
        path.display()
    );
    for (name, want) in rows {
        let slug = device_slug(name);
        assert_eq!(slug, want, "{name:?}");
        assert!(
            !slug.is_empty() && !slug.contains('/') && !slug.contains(char::is_whitespace),
            "{name:?} slugs to {slug:?}, which is not one path segment"
        );
        assert_eq!(device_slug(&slug), slug, "{name:?}: a slug is its own slug");
    }
}

/// The runtime's `1000 * major + 10 * minor`, as the toolkits name themselves.
#[test]
fn cuda_versions_read_as_major_minor() {
    assert_eq!(cuda_version(12080), "12.8");
    assert_eq!(cuda_version(12090), "12.9");
    assert_eq!(cuda_version(13000), "13.0");
}

/// The kernel module's release out of `/proc/driver/nvidia/version`, in both module flavours'
/// wording, and nothing out of a text that is not one.
#[test]
fn the_driver_release_is_read_from_the_module_version() {
    let proprietary = "NVRM version: NVIDIA UNIX x86_64 Kernel Module  535.247.01  Tue Jun 17 \
                       22:46:15 UTC 2025\nGCC version:  gcc version 12.3.0 (Ubuntu \
                       12.3.0-1ubuntu1~22.04) \n";
    assert_eq!(
        driver_release_of(proprietary).as_deref(),
        Some("535.247.01")
    );
    let open = "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  580.95.05  Release Build  \
                (dvs-builder@U22-I3-AF03-09-1)  Tue Sep 23 09:53:58 UTC 2025\nGCC version:  gcc \
                version 13.3.0 (Ubuntu 13.3.0-6ubuntu2~24.04) \n";
    assert_eq!(driver_release_of(open).as_deref(), Some("580.95.05"));

    assert_eq!(driver_release_of(""), None);
    assert_eq!(
        driver_release_of("GCC version:  gcc version 12.3.0\n"),
        None
    );
    assert_eq!(
        driver_release_of("NVRM version: NVIDIA UNIX x86_64 Kernel Module  Tue Jun 17\n"),
        None,
        "a module line naming no release"
    );
    assert_eq!(
        driver_release_of(
            "NVRM version: NVIDIA UNIX x86_64 Kernel Module  535.247.01  580.95.05\n"
        ),
        None,
        "a module line naming two releases"
    );
    assert_eq!(
        driver_release_of(&format!("{proprietary}{open}")),
        None,
        "two module lines name two drivers"
    );
}

/// A file that cannot be read is refused naming it, as one that reads as no release is.
#[test]
fn an_unreadable_or_unknown_driver_version_is_refused() {
    let missing = Path::new("/nonexistent/peacock/driver/version");
    let refused = driver_release(missing).expect_err("there is no file");
    assert!(
        refused.contains("/nonexistent/peacock/driver/version"),
        "{refused}"
    );

    let dir = std::env::temp_dir().join(format!("peacock-driver-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let unknown = dir.join("version");
    std::fs::write(&unknown, "not a module version\n").unwrap();
    let refused = driver_release(&unknown).expect_err("no release in it");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.contains("NVRM version"), "{refused}");
}
