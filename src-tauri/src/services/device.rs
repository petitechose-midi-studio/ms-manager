use crate::layout::PayloadLayout;
use crate::models::{DeviceStatus, DeviceTarget};
use crate::services::process;

pub async fn device_status(layout: &PayloadLayout) -> DeviceStatus {
    let loader = loader_path(layout);
    if !loader.exists() {
        return DeviceStatus {
            connected: false,
            count: 0,
            targets: Vec::new(),
        };
    }

    let mut cmd = tokio::process::Command::new(&loader);
    process::no_console_window(&mut cmd);
    let out = match cmd.args(["list", "--json"]).output().await {
        Ok(v) => v,
        Err(_) => {
            return DeviceStatus {
                connected: false,
                count: 0,
                targets: Vec::new(),
            }
        }
    };

    if !out.status.success() {
        return DeviceStatus {
            connected: false,
            count: 0,
            targets: Vec::new(),
        };
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| !l.trim().is_empty()).unwrap_or("");

    let (count, targets) = parse_list_event(line);
    DeviceStatus {
        connected: count > 0,
        count,
        targets,
    }
}

fn loader_path(layout: &PayloadLayout) -> std::path::PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("midi-studio-loader.exe")
    } else {
        bin.join("midi-studio-loader")
    }
}

fn parse_list_event(json_line: &str) -> (u32, Vec<DeviceTarget>) {
    #[derive(serde::Deserialize)]
    struct ListEvent {
        event: String,
        #[serde(default)]
        count: u32,
        #[serde(default)]
        targets: Vec<DeviceTarget>,
    }

    let Ok(ev) = serde_json::from_str::<ListEvent>(json_line) else {
        return (0, Vec::new());
    };
    if ev.event != "list" {
        return (0, Vec::new());
    }
    let count = if ev.count > 0 {
        ev.count
    } else {
        ev.targets.len() as u32
    };
    (count, ev.targets)
}
