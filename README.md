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

`~/.config/pmanager/config.toml` is created automatically and is safe to hand-edit. The daemon notices saved edits within a couple of seconds and applies only what changed: tunnels you didn't touch keep running with their open connections, an edited tunnel is stopped (start it again to pick up the change), and a removed one is torn down. `r` in the TUI does the same on demand and refreshes the list. Profiles are named tables, and tunnels are one-line entries under the profile they use:

```toml
[profiles.nixserver]
host = "192.168.1.50"
username = "germano"

[profiles.nixserver.auth]
type = "private_key"
path = "/Users/me/.ssh/id_ed25519"

[profiles.worker]
host = "10.233.1.2"
username = "germano"
jump = "nixserver"

[profiles.worker.auth]
type = "private_key"
path = "/Users/me/.ssh/id_ed25519"

[profiles.worker.local]
web = "1337 -> localhost:1337"
postgres = "15432 -> db.internal:5432"

[profiles.worker.remote]
preview = "8080 -> localhost:3000"

[profiles.worker.dynamic]
socks = 1080
```

Adding, editing, or removing a normal tunnel means changing one line. The left side is its globally unique name. `listen -> target` is used consistently: `local` listens on this machine, while `remote` listens on the SSH server. Dynamic entries only need their local SOCKS port. Listeners default to `127.0.0.1`.

Use an inline table for non-default settings or an explicit bind address:

```toml
[profiles.worker.local]
web = { listen = "0.0.0.0:1337", target = "localhost:3000", autostart = true }
disabled-db = { listen = 15432, target = "db.internal:5432", enabled = false }
```

SSH port defaults to `22`, and authentication defaults to `agent`. Explicit auth types are `agent`, `private_key` (`path` and optional `passphrase`), and `password` (optional `password`). Passwords and key passphrases are stored as plaintext, so prefer the agent where possible. Jump profiles may themselves use `jump`, but cycles and unknown profile references are rejected.

Profiles can also be managed in the TUI: press `p` to switch panels, then `a`/`e`/`d`. Deleting a profile still referenced by a tunnel or jump is rejected.

## Host key verification

A host presenting a plain public key is checked against your normal `~/.ssh/known_hosts` — the same file and format the system `ssh` client uses, so hosts you've already connected to with `ssh` are already trusted here. An unrecorded host is trusted on first connection and then recorded (`accept-new` semantics, like `ssh -o StrictHostKeyChecking=accept-new`) since the daemon has no terminal to interactively prompt on. A host presenting a **different** key than the one on record is always rejected — that's the actual attack this check exists to catch.

A host presenting a **certificate** instead is verified against `@cert-authority` lines in the same `~/.ssh/known_hosts` file (the marker OpenSSH itself uses for CA trust) — no separate pmanager config needed. A certificate is accepted only if: a `@cert-authority` line matches the host, the certificate validates against that CA's key, the current time is within the certificate's validity window, and (if the certificate restricts principals) this host is one of them. Any failure is rejected and logged loudly, the same as a known_hosts key mismatch — certificates don't fall back to trust-on-first-use, since a CA signature is what makes them trustworthy in the first place. The host pattern matching on `@cert-authority` lines only supports an exact `host[:port]` match or a bare `*`, not full glob syntax.

