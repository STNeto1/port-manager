use std::path::PathBuf;
use std::process::Command;

use color_eyre::eyre::{Result, bail, eyre};

const UNIT_NAME: &str = "pmanager.service";

fn unit_path(home: &str) -> PathBuf {
    PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user")
        .join(UNIT_NAME)
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

    let exec_start = match log_level {
        Some(level) => format!("{exe} daemon --log-level {level}"),
        None => format!("{exe} daemon"),
    };

    let unit = format!(
        r#"[Unit]
Description=pmanager SSH tunnel daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
Environment=HOME=%h
ExecStart={exec_start}
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
"#
    );

    let path = unit_path(&home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, unit)?;

    run_systemctl(&["--user", "daemon-reload"])?;
    run_systemctl(&["--user", "enable", "--now", UNIT_NAME])?;

    println!(
        "Installed and started {UNIT_NAME}. To keep it running without an active login \
         session, also run: loginctl enable-linger $USER"
    );
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let home = home_dir()?;
    // Best-effort: fails harmlessly if it's already not loaded.
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", UNIT_NAME])
        .status();

    let path = unit_path(&home);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();

    println!("Uninstalled {UNIT_NAME}");
    Ok(())
}

fn run_systemctl(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl").args(args).status()?;
    if !status.success() {
        bail!(
            "`systemctl {}` failed (exit status {status})",
            args.join(" ")
        );
    }
    Ok(())
}
