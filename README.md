# pmanager

A terminal UI for managing SSH port-forwarding tunnels — local (`-L`), remote (`-R`), and dynamic/SOCKS (`-D`) — including tunnels reached through a jump host (`ProxyJump`).

A background **daemon** owns every SSH session and keeps tunnels running independently of the UI; the **TUI** (and a few headless CLI commands) are thin clients that connect to it over a Unix domain socket. Closing the TUI does not stop your tunnels.

## Install

```sh
make install   # cargo install --path . --force
```

or just build it in place:

```sh
make build     # cargo build
make release   # cargo build --release
```

## Quick start

```sh
pmanager
```

Running `pmanager` with no arguments launches the TUI. If no daemon is running yet, it starts one automatically — there's nothing to set up by hand for local use. The first run creates an empty config at `~/.config/pmanager/config.toml`; press `a` in the TUI to add your first tunnel.

## CLI

| Command | What it does |
|---|---|
| `pmanager` / `pmanager tui` | Launch the TUI (default). Auto-spawns the daemon if needed. |
| `pmanager daemon` | Run the daemon in the foreground (for a supervisor, e.g. launchd/systemd, or invoked internally by auto-spawn). |
| `pmanager list` | Print configured tunnels and their status. |
| `pmanager start <name>` | Start a tunnel by name; it keeps running after the command exits. |
| `pmanager stop <name>` | Stop a running tunnel by name. |
| `pmanager shutdown` | Stop the daemon and every tunnel it's running. |
| `pmanager service install` | Install and start a persistent per-user launchd/systemd service (see below). |
| `pmanager service uninstall` | Stop and remove that service. |
| `pmanager daemon --log-level <level>` | Set the daemon's log level (e.g. `info`, `debug`, `pmanager=trace`). Overrides `RUST_LOG`. |

The daemon logs to stdout. When auto-spawned it's redirected to `~/.config/pmanager/daemon.log`; run `pmanager daemon` yourself in a terminal (or under a supervisor) to see or capture it directly.

## Persistent daemon (launchd / systemd)

`pmanager service install` sets up the daemon to start automatically on login and restart if it crashes, so tunnels with `autostart = true` come up without needing the TUI or any client to connect first:

- **macOS**: writes a per-user LaunchAgent to `~/Library/LaunchAgents/com.pmanager.daemon.plist` and loads it with `launchctl bootstrap`.
- **Linux**: writes a systemd **user** unit to `~/.config/systemd/user/pmanager.service` and starts it with `systemctl --user enable --now`. For the unit to keep running without an active login session, also run `loginctl enable-linger $USER` — `pmanager service install` doesn't do this for you.

Both write the currently-running `pmanager` binary's own path into the service definition — reinstall (`pmanager service uninstall` then `install` again) after upgrading the binary at a different path (e.g. after `cargo install --path . --force`) so the service points at the new one. `pmanager service uninstall` stops the service and removes its plist/unit file; it doesn't touch `~/.config/pmanager/config.toml`.

## TUI keybindings

| Key | Action |
|---|---|
| `↑`/`k`, `↓`/`j` | Move selection |
| `Enter` / `s` | Start or stop the selected tunnel |
| `p` | Switch between the Tunnels and Profiles panels |
| `a` | Add a tunnel/profile (whichever panel is active) |
| `e` | Edit the selected tunnel/profile (editing a running tunnel stops it — start it again to pick up changes) |
| `d` | Delete the selected tunnel/profile (asks to confirm) |
| `r` | Reload `config.toml` from disk |
| `q` / `Ctrl+C` | Quit the TUI (does **not** stop the daemon or its tunnels) |
| `Esc` | Cancel a form or modal |
| Add/edit form: `Tab`/`Shift+Tab` or `↑`/`↓` | Move between fields |
| Add/edit form: `←`/`→` | Change a multiple-choice field (direction, auth method, jump) |

## Config file

`~/.config/pmanager/config.toml`, created automatically on first run and safe to hand-edit (`r` in the TUI reloads it). It has two parts: a list of **profiles** (reusable SSH connections — host/port/username/auth, optionally through a jump host) and a list of **tunnels** that each reference one profile by name, the same way an ssh_config `Host` block covers every `LocalForward` line under it. Any number of tunnels can share a profile instead of repeating its connection details.

```toml
[[profiles]]
name = "nixserver"
host = "192.168.1.50"
port = 22
username = "germano"
[profiles.auth]
type = "private_key"            # "password" | "private_key" | "agent"
path = "/Users/me/.ssh/id_ed25519"
passphrase = ""                  # omit or leave blank if the key isn't encrypted

[[profiles]]
name = "worker"
host = "10.233.1.2"
port = 22
username = "germano"
jump = "nixserver"                # reach `worker` through the `nixserver` profile. That profile can itself have a `jump`, chaining any number of hops.
[profiles.auth]
type = "private_key"
path = "/Users/me/.ssh/id_ed25519"

[[tunnels]]
id = "9d2e7f10-...."          # UUID, assigned automatically — don't reuse across entries
name = "worker-dev-server"
direction = "local"            # "local" | "remote" | "dynamic"
profile = "worker"              # references a [[profiles]] entry by name
enabled = true
autostart = false               # start automatically when the daemon launches

[tunnels.local_bind]            # where pmanager listens locally
bind_addr = "127.0.0.1"
port = 1337

[tunnels.remote]                # meaning depends on direction — see below
host = "localhost"
port = 1337
```

`[tunnels.remote]`'s meaning depends on `direction`:
- **local**: the address `profile.host` connects to *on the far side* of the SSH session (classic `ssh -L local_bind:remote`).
- **remote**: the address the daemon connects to *locally* when the server forwards a connection back (classic `ssh -R remote_bind:this_field`, with `local_bind` acting as the server-side bind address/port).
- **dynamic**: not used — SOCKS clients supply their own target per-connection.

**Auth methods** (set per-profile): `agent` (recommended — talks to `ssh-agent` via `$SSH_AUTH_SOCK`, no secrets in the config file), `private_key` (path + optional passphrase — a plaintext passphrase in `config.toml` is a real risk if the key needs one), and `password` (plaintext password in `config.toml` — avoid unless you understand that tradeoff; the add/edit form doesn't warn on this yet).

Profiles can be created, edited, and deleted from the TUI itself — press `p` to switch to the Profiles panel, then `a`/`e`/`d` as usual. A profile's `name` is its identity and can't be changed from an edit form; delete and recreate it to rename. Deleting a profile that's still referenced by a tunnel, or used as another profile's `jump`, is rejected until that reference is removed.

## Host key verification

Server host keys are checked against your normal `~/.ssh/known_hosts` — the same file and format the system `ssh` client uses, so hosts you've already connected to with `ssh` are already trusted here. An unrecorded host is trusted on first connection and then recorded (`accept-new` semantics, like `ssh -o StrictHostKeyChecking=accept-new`) since the daemon has no terminal to interactively prompt on. A host presenting a **different** key than the one on record is always rejected — that's the actual attack this check exists to catch. Certificate-based host authentication isn't verified yet (falls back to trust-without-checking, logged loudly).

## Known limitations

- No SSH certificate-based host verification.
