#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use color_eyre::eyre::Result;

/// Installs and starts a per-user launchd LaunchAgent (macOS) or systemd
/// user unit (Linux) that runs `pmanager daemon` persistently, so tunnels
/// with `autostart = true` come up on login/reboot without any client
/// ever connecting.
pub fn install(log_level: Option<&str>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::install(log_level)
    }
    #[cfg(target_os = "linux")]
    {
        linux::install(log_level)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = log_level;
        color_eyre::eyre::bail!("`pmanager service` is only supported on macOS and Linux");
    }
}

/// Stops and removes the service installed by [`install`].
pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::uninstall()
    }
    #[cfg(target_os = "linux")]
    {
        linux::uninstall()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        color_eyre::eyre::bail!("`pmanager service` is only supported on macOS and Linux");
    }
}
