//! Which device, driver, CUDA, cuDF and host a benchmark run measured on, and the directory
//! its results go to.

#[cfg(test)]
mod tests;

use std::path::Path;

use super::Provenance;

/// The vendor dropped, ASCII lowercased, every run of other characters one `-`: the same name
/// always gives the same segment, and a slug is its own slug. `scripts/calibration/record.py`
/// has the copy that files a record under it.
pub(crate) fn device_slug(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let model = lower.strip_prefix("nvidia ").unwrap_or(&lower);
    model
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A CUDA version in the runtime's `1000 * major + 10 * minor`, as `major.minor`.
pub(crate) fn cuda_version(encoded: u32) -> String {
    format!("{}.{}", encoded / 1000, encoded % 1000 / 10)
}

/// Where the kernel module states its release. The NVIDIA container toolkit puts it in a
/// container's `/proc` as well, so the run image reads the host's driver here too.
pub(crate) const DRIVER_VERSION_PATH: &str = "/proc/driver/nvidia/version";

/// The release on the file's one `NVRM version:` line: its one word of digits and dots,
/// `580.95.05`. `None` for no such line, two of them, or a line naming no release or two.
pub(crate) fn driver_release_of(text: &str) -> Option<String> {
    let mut modules = text
        .lines()
        .filter_map(|line| line.strip_prefix("NVRM version:"));
    let module = modules.next()?;
    if modules.next().is_some() {
        return None;
    }
    let is_release = |word: &&str| {
        word.contains('.')
            && word
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    };
    let mut releases = module.split_whitespace().filter(is_release);
    let release = releases.next()?;
    releases.next().is_none().then(|| release.to_string())
}

/// The driver's release from the module's version file, or why there is none.
pub(crate) fn driver_release(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "cannot read the driver's release from {}: {e}",
            path.display()
        )
    })?;
    driver_release_of(&text).ok_or_else(|| {
        format!(
            "{} has no one `NVRM version:` line naming one release: {text:?}",
            path.display()
        )
    })
}

/// The five as `(key, value)`, in the one order the trailer and the heading both write.
pub(crate) fn fields(provenance: &Provenance) -> [(&'static str, &str); 5] {
    [
        ("device", &provenance.device),
        ("driver", &provenance.driver),
        ("cuda", &provenance.cuda),
        ("cudf", &provenance.cudf),
        ("host", &provenance.host),
    ]
}

/// This process's provenance: the device and CUDA runtime from the runtime, the driver's
/// release from its kernel module, the cuDF release this binary was built against, and the
/// kernel's host name — inside a container, the container's own name unless it was started
/// with `--hostname`.
#[cfg(not(feature = "rust-only"))]
pub(crate) fn of_this_run(what: &str) -> Provenance {
    let device = crate::executor::device_info()
        .unwrap_or_else(|| panic!("{what}: the CUDA runtime describes no device"));
    let driver = driver_release(Path::new(DRIVER_VERSION_PATH))
        .unwrap_or_else(|refused| panic!("{what}: {refused}"));
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_else(|e| panic!("{what}: cannot read the host name: {e}"));
    Provenance {
        device: device.name,
        driver,
        cuda: cuda_version(device.runtime),
        cudf: super::device_answer::cudf_release(),
        host: host.trim().to_string(),
    }
}
