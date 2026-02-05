use std::path::Path;

use sysinfo::{ProcessRefreshKind, System, UpdateKind};

fn norm_path(p: &Path) -> String {
    // Best-effort normalization for comparisons.
    let s = p.to_string_lossy().to_string();
    if cfg!(windows) {
        s.replace('/', "\\").to_ascii_lowercase()
    } else {
        s
    }
}

pub fn find_oc_bridge_daemon_pids(exe_path: &Path) -> Vec<u32> {
    let exe_norm = norm_path(exe_path);

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );

    let mut pids = Vec::new();
    for (pid, proc_) in sys.processes() {
        let Some(exe) = proc_.exe() else {
            continue;
        };

        if norm_path(exe) != exe_norm {
            continue;
        }

        if !proc_.cmd().iter().any(|a| a == "--daemon") {
            continue;
        }

        pids.push(pid.as_u32());
    }
    pids
}

pub fn kill_oc_bridge_daemons(exe_path: &Path) -> usize {
    let pids = find_oc_bridge_daemon_pids(exe_path);
    if pids.is_empty() {
        return 0;
    }

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );

    let mut killed = 0;
    for pid_u32 in pids {
        let pid = sysinfo::Pid::from_u32(pid_u32);
        if let Some(p) = sys.process(pid) {
            if p.kill() {
                killed += 1;
            }
        }
    }
    killed
}

pub fn kill_all_oc_bridge_daemons() -> usize {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );

    let mut killed = 0;
    for (_pid, proc_) in sys.processes() {
        let name = proc_.name().to_ascii_lowercase();
        if name != "oc-bridge" && name != "oc-bridge.exe" {
            continue;
        }
        if !proc_.cmd().iter().any(|a| a == "--daemon") {
            continue;
        }
        if proc_.kill() {
            killed += 1;
        }
    }
    killed
}
