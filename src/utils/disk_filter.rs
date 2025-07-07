use std::collections::HashSet;

pub struct DiskFilter {
    excluded_prefixes: HashSet<&'static str>,
    excluded_exact: HashSet<&'static str>,
}

impl DiskFilter {
    pub fn new() -> Self {
        let mut excluded_prefixes = HashSet::new();
        let mut excluded_exact = HashSet::new();

        // Platform-specific system directories
        Self::add_macos_exclusions(&mut excluded_prefixes, &mut excluded_exact);
        Self::add_linux_exclusions(&mut excluded_prefixes, &mut excluded_exact);
        Self::add_common_exclusions(&mut excluded_prefixes, &mut excluded_exact);

        Self {
            excluded_prefixes,
            excluded_exact,
        }
    }

    pub fn should_include(&self, mount_point: &str) -> bool {
        // Check exact matches first (faster)
        if self.excluded_exact.contains(mount_point) {
            return false;
        }

        // Check prefix matches
        for prefix in &self.excluded_prefixes {
            if mount_point.starts_with(prefix) {
                return false;
            }
        }

        true
    }

    fn add_macos_exclusions(
        prefixes: &mut HashSet<&'static str>,
        exact: &mut HashSet<&'static str>,
    ) {
        // macOS system volumes
        prefixes.insert("/System/Volumes/");
        prefixes.insert("/Library/");
        prefixes.insert("/Applications/");
        prefixes.insert("/System/");
        prefixes.insert("/private/");
        prefixes.insert("/Volumes/");
        prefixes.insert("/Network/");

        exact.insert("/Users/Shared");
        exact.insert("/cores");
    }

    fn add_linux_exclusions(
        prefixes: &mut HashSet<&'static str>,
        exact: &mut HashSet<&'static str>,
    ) {
        // Linux system directories
        prefixes.insert("/dev/");
        prefixes.insert("/proc/");
        prefixes.insert("/sys/");
        prefixes.insert("/run/");
        prefixes.insert("/snap/");
        prefixes.insert("/usr/");
        prefixes.insert("/var/log/");
        prefixes.insert("/var/cache/");
        prefixes.insert("/var/lib/");
        prefixes.insert("/var/tmp/");
        prefixes.insert("/var/spool/");

        exact.insert("/boot");
        exact.insert("/tmp");
        exact.insert("/bin");
        exact.insert("/sbin");
        exact.insert("/etc");
        exact.insert("/lib");
        exact.insert("/lib64");
        exact.insert("/opt");
        exact.insert("/media");
        exact.insert("/mnt");
        exact.insert("/root");
        exact.insert("/srv");
    }

    fn add_common_exclusions(
        prefixes: &mut HashSet<&'static str>,
        _exact: &mut HashSet<&'static str>,
    ) {
        // Common runtime and temporary directories
        prefixes.insert("/tmp/");
        prefixes.insert("/var/tmp/");
    }
}

impl Default for DiskFilter {
    fn default() -> Self {
        Self::new()
    }
}

// Thread-safe singleton for global use
use std::sync::OnceLock;

static DISK_FILTER: OnceLock<DiskFilter> = OnceLock::new();

pub fn should_include_disk(mount_point: &str) -> bool {
    let filter = DISK_FILTER.get_or_init(DiskFilter::new);
    filter.should_include(mount_point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_exclusions() {
        let filter = DiskFilter::new();

        // Should exclude macOS system paths
        assert!(!filter.should_include("/System/Volumes/Data"));
        assert!(!filter.should_include("/Library/Preferences"));
        assert!(!filter.should_include("/Applications/Safari.app"));
        assert!(!filter.should_include("/Users/Shared"));

        // Should include user and data paths
        assert!(filter.should_include("/Users/john"));
        assert!(filter.should_include("/"));
        assert!(filter.should_include("/Volumes/ExternalDrive"));
    }

    #[test]
    fn test_linux_exclusions() {
        let filter = DiskFilter::new();

        // Should exclude Linux system paths
        assert!(!filter.should_include("/dev/sda1"));
        assert!(!filter.should_include("/proc/meminfo"));
        assert!(!filter.should_include("/sys/class"));
        assert!(!filter.should_include("/run/user/1000"));
        assert!(!filter.should_include("/usr/bin"));
        assert!(!filter.should_include("/var/log/syslog"));

        // Should include user and data paths
        assert!(filter.should_include("/home/user"));
        assert!(filter.should_include("/"));
        assert!(filter.should_include("/data"));
    }

    #[test]
    fn test_performance() {
        let filter = DiskFilter::new();
        let mount_points = [
            "/",
            "/home",
            "/data",
            "/System/Volumes/Data",
            "/usr/bin",
            "/var/log/messages",
            "/Users/john",
            "/Applications/Safari.app",
        ];

        // Test that all lookups are fast
        for mount_point in &mount_points {
            filter.should_include(mount_point);
        }
    }
}
