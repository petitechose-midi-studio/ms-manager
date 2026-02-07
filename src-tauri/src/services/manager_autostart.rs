#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;

/// Per-user autostart management for ms-manager itself.
///
/// Design goal: the end-user should never need to manage oc-bridge directly.
/// ms-manager is the *only* app that autostarts by default, and it in turn
/// supervises oc-bridge.

#[cfg(target_os = "windows")]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

// Keep the identifier stable across releases.
#[cfg(target_os = "windows")]
const AUTOSTART_ID: &str = "MidiStudioManager";

pub fn is_installed() -> bool {
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("reg")
            .args(["query", RUN_KEY, "/v", AUTOSTART_ID])
            .output();
        return out.is_ok_and(|o| o.status.success());
    }

    #[cfg(target_os = "macos")]
    {
        return plist_path().exists();
    }

    #[cfg(target_os = "linux")]
    {
        return desktop_path().exists();
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

pub fn install() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;

    #[cfg(target_os = "windows")]
    {
        let data = format!("{} --background", quote_exec(&exe));
        let out = std::process::Command::new("reg")
            .args([
                "add",
                RUN_KEY,
                "/v",
                AUTOSTART_ID,
                "/t",
                "REG_SZ",
                "/d",
                &data,
                "/f",
            ])
            .output()?;

        if out.status.success() {
            return Ok(());
        }

        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).to_string(),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let path = plist_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let plist = launch_agent_plist(&exe);
        std::fs::write(&path, plist)?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let path = desktop_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let desktop = xdg_desktop_entry(&exe);
        std::fs::write(&path, desktop)?;
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = exe;
        Err(std::io::Error::other("unsupported platform"))
    }
}

pub fn uninstall() -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        if !is_installed() {
            return Ok(());
        }
        let out = std::process::Command::new("reg")
            .args(["delete", RUN_KEY, "/v", AUTOSTART_ID, "/f"])
            .output()?;
        if out.status.success() {
            return Ok(());
        }
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).to_string(),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let path = plist_path();
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        let path = desktop_path();
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        return Ok(());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(std::io::Error::other("unsupported platform"))
    }
}

#[cfg(target_os = "windows")]
fn quote_exec(path: &std::path::Path) -> String {
    let s = path.to_string_lossy().to_string();
    if s.contains(' ') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s
    }
}

#[cfg(target_os = "macos")]
fn plist_path() -> PathBuf {
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("io.petitechose.midistudio.manager.plist")
}

#[cfg(target_os = "linux")]
fn desktop_path() -> PathBuf {
    if let Some(v) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(v)
            .join("autostart")
            .join("io.petitechose.midistudio.manager.desktop");
    }
    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".config")
        .join("autostart")
        .join("io.petitechose.midistudio.manager.desktop")
}

#[cfg(target_os = "macos")]
fn launch_agent_plist(exe: &std::path::Path) -> String {
    let exe = exe.to_string_lossy();
    format!(
        r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
  <key>Label</key>
  <string>io.petitechose.midistudio.manager</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>--background</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "linux")]
fn xdg_desktop_entry(exe: &std::path::Path) -> String {
    let exe = exe.to_string_lossy();
    format!(
        "[Desktop Entry]\nType=Application\nName=ms-manager\nExec={exe} --background\nX-GNOME-Autostart-enabled=true\n"
    )
}
