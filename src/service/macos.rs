use std::path::PathBuf;
use std::process::Command;

use color_eyre::eyre::{Result, bail, eyre};

const LABEL: &str = "com.pmanager.daemon";

fn plist_path(home: &str) -> PathBuf {
    PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn home_dir() -> Result<String> {
    std::env::var("HOME").map_err(|_| eyre!("HOME environment variable must be set"))
}

pub fn install(log_level: Option<&str>) -> Result<()> {
    let home = home_dir()?;
    let exe = std::env::current_exe()?;
    let exe = exe
        .to_str()
        .ok_or_else(|| eyre!("executable path isn't valid UTF-8"))?;
    let log_path = PathBuf::from(&home)
        .join(".config")
        .join("pmanager")
        .join("daemon.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let log_level_args = match log_level {
        Some(level) => {
            format!("\n        <string>--log-level</string>\n        <string>{level}</string>")
        }
        None => String::new(),
    };

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>daemon</string>{log_level_args}
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{home}</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        log = log_path.display(),
    );

    let path = plist_path(&home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, plist)?;

    let uid = unsafe { libc::getuid() };
    let path_str = path
        .to_str()
        .ok_or_else(|| eyre!("plist path isn't valid UTF-8"))?;
    let status = Command::new("launchctl")
        .args(["bootstrap", &format!("gui/{uid}"), path_str])
        .status()?;
    if !status.success() {
        bail!(
            "launchctl bootstrap failed (exit status {status}); see launchctl's own error output above"
        );
    }

    println!(
        "Installed and started {LABEL} — logs at {}",
        log_path.display()
    );
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let home = home_dir()?;
    let uid = unsafe { libc::getuid() };
    // Best-effort: bootout fails if it's already not loaded, which is fine.
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("gui/{uid}/{LABEL}")])
        .status();

    let path = plist_path(&home);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    println!("Uninstalled {LABEL}");
    Ok(())
}
