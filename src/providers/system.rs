use std::{
    fs,
    path::{Path, PathBuf},
};

use super::Provider;

const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";
const NETWORK_PATH: &str = "/sys/class/net";

pub struct BatteryProvider {
    root: PathBuf,
}

pub struct NetworkProvider {
    root: PathBuf,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BatteryItem {
    name: String,
    percentage: u8,
    status: String,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkCounts {
    up: u32,
    down: u32,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkInterface {
    name: String,
    kind: String,
    state: String,
    is_up: bool,
}

impl BatteryProvider {
    pub fn new_default() -> Self {
        Self::new(POWER_SUPPLY_PATH)
    }

    fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl NetworkProvider {
    pub fn new_default() -> Self {
        Self::new(NETWORK_PATH)
    }

    fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Provider for BatteryProvider {
    fn name(&self) -> &'static str {
        "battery"
    }

    fn snapshot(&self) -> serde_json::Value {
        battery_snapshot(&self.root).unwrap_or_else(unavailable)
    }
}

impl Provider for NetworkProvider {
    fn name(&self) -> &'static str {
        "network"
    }

    fn snapshot(&self) -> serde_json::Value {
        network_snapshot(&self.root).unwrap_or_else(unavailable)
    }
}

fn battery_snapshot(root: &Path) -> Result<serde_json::Value, String> {
    let mut batteries = Vec::new();
    let entries = fs::read_dir(root)
        .map_err(|error| format!("failed to read power supply directory: {error}"))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read power supply entry: {error}"))?;
        let path = entry.path();
        if !path.is_dir() || read_trim(path.join("type")).as_deref() != Some("Battery") {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let capacity = read_trim(path.join("capacity"))
            .ok_or_else(|| format!("battery {name} missing capacity"))?;
        let percentage = capacity
            .parse::<u16>()
            .map_err(|error| format!("invalid battery capacity for {name}: {error}"))?
            .min(100) as u8;
        let status = read_trim(path.join("status"))
            .map(|status| normalize_status(status.as_str()))
            .unwrap_or_else(|| "unknown".to_owned());

        batteries.push(BatteryItem {
            name,
            percentage,
            status,
        });
    }

    if batteries.is_empty() {
        return Ok(serde_json::json!({
            "available": false,
            "reason": "no battery",
        }));
    }

    let percentage = average_battery_percentage(&batteries);
    let status = aggregate_battery_status(&batteries);
    Ok(serde_json::json!({
        "available": true,
        "percentage": percentage,
        "status": status,
        "batteries": batteries,
    }))
}

fn average_battery_percentage(batteries: &[BatteryItem]) -> u8 {
    let sum = batteries
        .iter()
        .map(|battery| u32::from(battery.percentage))
        .sum::<u32>();
    ((sum + batteries.len() as u32 / 2) / batteries.len() as u32) as u8
}

fn aggregate_battery_status(batteries: &[BatteryItem]) -> &'static str {
    if batteries
        .iter()
        .any(|battery| battery.status.as_str() == "charging")
    {
        "charging"
    } else if batteries
        .iter()
        .any(|battery| battery.status.as_str() == "discharging")
    {
        "discharging"
    } else if batteries.iter().all(|battery| battery.status == "full") {
        "full"
    } else if batteries
        .iter()
        .any(|battery| battery.status.as_str() == "not-charging")
    {
        "not-charging"
    } else {
        "unknown"
    }
}

fn network_snapshot(root: &Path) -> Result<serde_json::Value, String> {
    let mut wired = NetworkCounts { up: 0, down: 0 };
    let mut wireless = NetworkCounts { up: 0, down: 0 };
    let mut interfaces = Vec::new();
    let entries = fs::read_dir(root)
        .map_err(|error| format!("failed to read network interface directory: {error}"))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read network entry: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "lo" {
            continue;
        }

        let kind = network_kind(&path);
        let state = read_trim(path.join("operstate"))
            .map(|state| state.to_ascii_lowercase())
            .unwrap_or_else(|| "unknown".to_owned());
        let is_up = state == "up";

        match kind {
            "wired" => count_network_state(&mut wired, is_up),
            "wireless" => count_network_state(&mut wireless, is_up),
            _ => {}
        }

        interfaces.push(NetworkInterface {
            name,
            kind: kind.to_owned(),
            state,
            is_up,
        });
    }

    if interfaces.is_empty() {
        return Ok(serde_json::json!({
            "available": false,
            "reason": "no network interfaces",
        }));
    }

    interfaces.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    Ok(serde_json::json!({
        "available": true,
        "wired": wired,
        "wireless": wireless,
        "interfaces": interfaces,
    }))
}

fn network_kind(path: &Path) -> &'static str {
    if path.join("wireless").is_dir() {
        return "wireless";
    }

    match read_trim(path.join("type")).as_deref() {
        Some("1") => "wired",
        _ => "other",
    }
}

fn count_network_state(counts: &mut NetworkCounts, is_up: bool) {
    if is_up {
        counts.up += 1;
    } else {
        counts.down += 1;
    }
}

fn normalize_status(status: &str) -> String {
    status.trim().to_ascii_lowercase().replace(' ', "-")
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn unavailable(reason: String) -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "reason": reason,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static NEXT_TEST_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "html-desktop-shell-system-test-{}-{}",
                std::process::id(),
                NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("test dir should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: impl AsRef<Path>, contents: &str) {
        let path = path.as_ref();
        fs::create_dir_all(path.parent().expect("fixture file should have a parent"))
            .expect("fixture parent should be created");
        fs::write(path, contents).expect("fixture file should be written");
    }

    #[test]
    fn battery_snapshot_reads_capacity_and_status() {
        let dir = TestDir::new();
        write_file(dir.path().join("BAT0/type"), "Battery\n");
        write_file(dir.path().join("BAT0/capacity"), "87\n");
        write_file(dir.path().join("BAT0/status"), "Discharging\n");
        write_file(dir.path().join("AC/type"), "Mains\n");

        let snapshot = battery_snapshot(dir.path()).expect("battery fixture should parse");

        assert_eq!(snapshot["available"], true);
        assert_eq!(snapshot["percentage"], 87);
        assert_eq!(snapshot["status"], "discharging");
        assert_eq!(snapshot["batteries"][0]["name"], "BAT0");
    }

    #[test]
    fn battery_snapshot_reports_no_battery() {
        let dir = TestDir::new();
        write_file(dir.path().join("AC/type"), "Mains\n");

        let snapshot = battery_snapshot(dir.path()).expect("empty battery fixture should parse");

        assert_eq!(snapshot["available"], false);
        assert_eq!(snapshot["reason"], "no battery");
    }

    #[test]
    fn battery_provider_reports_malformed_capacity_as_unavailable() {
        let dir = TestDir::new();
        write_file(dir.path().join("BAT0/type"), "Battery\n");
        write_file(dir.path().join("BAT0/capacity"), "bad\n");

        let snapshot = BatteryProvider::new(dir.path()).snapshot();

        assert_eq!(snapshot["available"], false);
        assert!(
            snapshot["reason"]
                .as_str()
                .expect("reason should be a string")
                .contains("invalid battery capacity for BAT0")
        );
    }

    #[test]
    fn network_snapshot_counts_wired_and_wireless_states() {
        let dir = TestDir::new();
        write_file(dir.path().join("lo/type"), "772\n");
        write_file(dir.path().join("enp1s0/type"), "1\n");
        write_file(dir.path().join("enp1s0/operstate"), "up\n");
        write_file(dir.path().join("wlan0/type"), "1\n");
        fs::create_dir_all(dir.path().join("wlan0/wireless"))
            .expect("wireless fixture dir should be created");
        write_file(dir.path().join("wlan0/operstate"), "down\n");

        let snapshot = network_snapshot(dir.path()).expect("network fixture should parse");

        assert_eq!(snapshot["available"], true);
        assert_eq!(snapshot["wired"]["up"], 1);
        assert_eq!(snapshot["wired"]["down"], 0);
        assert_eq!(snapshot["wireless"]["up"], 0);
        assert_eq!(snapshot["wireless"]["down"], 1);
        assert_eq!(snapshot["interfaces"].as_array().expect("array").len(), 2);
    }

    #[test]
    fn network_snapshot_reports_no_interfaces() {
        let dir = TestDir::new();
        write_file(dir.path().join("lo/type"), "772\n");

        let snapshot = network_snapshot(dir.path()).expect("loopback-only fixture should parse");

        assert_eq!(snapshot["available"], false);
        assert_eq!(snapshot["reason"], "no network interfaces");
    }
}
