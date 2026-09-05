//! Rust-native host identity for `std.system.Platform`.

/// Portable host fields exposed to Terlan tooling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPlatform {
    /// Rust target operating-system identifier.
    pub operating_system: String,
    /// Rust target architecture identifier.
    pub architecture: String,
    /// Host search-path separator.
    pub path_separator: String,
    /// Host executable suffix.
    pub executable_suffix: String,
}

/// Dynamic host measurements recorded as informational performance metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct HostMetrics {
    /// Whether every required measurement was available.
    pub available: bool,
    /// Stable diagnostic explaining an unavailable snapshot.
    pub message: String,
    /// Host kernel release.
    pub kernel: String,
    /// Human-readable operating-system identity.
    pub operating_system: String,
    /// Human-readable processor model.
    pub cpu_model: String,
    /// Total physical memory in bytes.
    pub memory_bytes: i64,
    /// Currently available memory in bytes.
    pub available_memory_bytes: i64,
    /// Logical CPUs on which the current process may execute.
    pub cpu_affinity: Vec<i64>,
    /// Active frequency-scaling governor for the first logical CPU.
    pub cpu_governor: String,
    /// One-minute host load average.
    pub load_1m: f64,
    /// Five-minute host load average.
    pub load_5m: f64,
    /// Fifteen-minute host load average.
    pub load_15m: f64,
}

/// Returns the compile-target host identity used by the current VM binary.
pub fn current() -> HostPlatform {
    HostPlatform {
        operating_system: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        path_separator: if cfg!(windows) { ";" } else { ":" }.to_string(),
        executable_suffix: if cfg!(windows) { ".exe" } else { "" }.to_string(),
    }
}

/// Captures one typed snapshot of dynamic host measurements.
pub fn current_metrics() -> HostMetrics {
    #[cfg(target_os = "linux")]
    {
        linux_metrics().unwrap_or_else(unavailable_metrics)
    }

    #[cfg(not(target_os = "linux"))]
    {
        unavailable_metrics("host metrics are currently available only on Linux")
    }
}

#[cfg(target_os = "linux")]
fn linux_metrics() -> Result<HostMetrics, &'static str> {
    let status = read_host_file("/proc/self/status")?;
    let load = read_host_file("/proc/loadavg")?;
    let kernel = read_host_file("/proc/sys/kernel/osrelease")?;
    let os_release = read_host_file("/etc/os-release")?;
    let cpu_info = read_host_file("/proc/cpuinfo")?;
    let memory_info = read_host_file("/proc/meminfo")?;
    let governor = read_host_file("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")?;

    let affinity = keyed_line(&status, "Cpus_allowed_list:")
        .and_then(parse_cpu_list)
        .ok_or("could not decode process CPU affinity")?;
    let operating_system = quoted_assignment(&os_release, "PRETTY_NAME=")
        .ok_or("could not decode operating-system identity")?;
    let cpu_model = keyed_line(&cpu_info, "model name")
        .and_then(|value| value.strip_prefix(':'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("could not decode CPU model")?;
    let memory_bytes = memory_kib(&memory_info, "MemTotal:")?;
    let available_memory_bytes = memory_kib(&memory_info, "MemAvailable:")?;
    let mut loads = load.split_whitespace();
    let load_1m = parse_load(loads.next())?;
    let load_5m = parse_load(loads.next())?;
    let load_15m = parse_load(loads.next())?;

    Ok(HostMetrics {
        available: true,
        message: String::new(),
        kernel: kernel.trim().to_string(),
        operating_system,
        cpu_model: cpu_model.to_string(),
        memory_bytes,
        available_memory_bytes,
        cpu_affinity: affinity,
        cpu_governor: governor.trim().to_string(),
        load_1m,
        load_5m,
        load_15m,
    })
}

#[cfg(target_os = "linux")]
fn read_host_file(path: &str) -> Result<String, &'static str> {
    std::fs::read_to_string(path).map_err(|_| "required Linux host metric is unavailable")
}

#[cfg(target_os = "linux")]
fn keyed_line<'a>(source: &'a str, key: &str) -> Option<&'a str> {
    source
        .lines()
        .find_map(|line| line.strip_prefix(key).map(str::trim))
}

#[cfg(target_os = "linux")]
fn quoted_assignment(source: &str, key: &str) -> Option<String> {
    keyed_line(source, key).map(|value| value.trim_matches('"').to_string())
}

#[cfg(target_os = "linux")]
fn parse_cpu_list(source: &str) -> Option<Vec<i64>> {
    let mut cpus = Vec::new();
    for component in source.split(',') {
        let mut bounds = component.split('-');
        let first = bounds.next()?.parse::<i64>().ok()?;
        match bounds.next() {
            Some(last) => {
                let last = last.parse::<i64>().ok()?;
                if bounds.next().is_some() || last < first {
                    return None;
                }
                cpus.extend(first..=last);
            }
            None => cpus.push(first),
        }
    }
    (!cpus.is_empty()).then_some(cpus)
}

#[cfg(target_os = "linux")]
fn memory_kib(source: &str, key: &str) -> Result<i64, &'static str> {
    let kib = keyed_line(source, key)
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or("could not decode host memory")?;
    kib.checked_mul(1024)
        .ok_or("host memory exceeds the portable integer range")
}

#[cfg(target_os = "linux")]
fn parse_load(value: Option<&str>) -> Result<f64, &'static str> {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or("could not decode host load average")
}

fn unavailable_metrics(message: &'static str) -> HostMetrics {
    HostMetrics {
        available: false,
        message: message.to_string(),
        kernel: String::new(),
        operating_system: std::env::consts::OS.to_string(),
        cpu_model: String::new(),
        memory_bytes: 0,
        available_memory_bytes: 0,
        cpu_affinity: Vec::new(),
        cpu_governor: String::new(),
        load_1m: 0.0,
        load_5m: 0.0,
        load_15m: 0.0,
    }
}
