# PManagerMenuBar

A native macOS menu bar client for `pmanager` — shows live tunnel status next to
the system WiFi/Bluetooth icons and lets you start/stop tunnels with a click,
instead of opening the TUI. It's a second thin client of the same daemon,
speaking the same Unix-socket JSON protocol as the TUI (`src/client/tui/`);
the daemon and TUI are unchanged.

v1 scope: view live tunnel status, start/stop tunnels. Adding/editing/deleting
tunnels or profiles, and "launch at login", stay TUI-only for now.

## Build & run

```sh
cd macos/PManagerMenuBar
swift run PManagerMenuBar
```

This is a plain Swift Package Manager app (no Xcode project) — it only needs
the Swift toolchain that ships with Xcode Command Line Tools, no full Xcode
install required. `swift build` produces a runnable binary at
`.build/debug/PManagerMenuBar`; there's no `.app` bundle, so `LSUIElement` is
set programmatically instead (`NSApp.setActivationPolicy(.accessory)` in
`PManagerMenuBarApp.swift`) to keep it out of the Dock. If you do have Xcode,
you can open this folder's `Package.swift` directly in it.

## How it finds the daemon

On launch it tries to connect to `~/.config/pmanager/pmanager.sock`. If
nothing's listening, it probes a short list of common install locations for
the `pmanager` binary (`~/.cargo/bin/pmanager`, `/usr/local/bin/pmanager`,
`/opt/homebrew/bin/pmanager` — see `DaemonProcess.swift`) and spawns
`pmanager daemon` if found. `cargo install --path .` (i.e. `make install`)
already places the binary at `~/.cargo/bin/pmanager`, so no extra setup is
needed after a normal install. If no daemon is found and none can be spawned,
the dropdown shows a "Daemon not running" state with a Retry button — start
one yourself (`pmanager` or `pmanager service install`) and retry.

## Layout

- `Sources/PManagerCore/Wire/` — Codable mirrors of `src/ipc/protocol.rs`,
  `src/model.rs`, `src/config/schema.rs`. Several types need custom
  `Codable` implementations because serde's default externally-tagged enum
  shape has no built-in Swift `Codable` equivalent — see the doc comments on
  `TunnelState`, `TunnelEvent`, `ResponsePayload`, `DaemonMessage`.
- `Sources/PManagerCore/Networking/` — `UnixSocketConnection` (newline-framed
  `NWConnection`), `DaemonClient` (request/response), `DaemonEventSubscriber`
  (a **second**, dedicated connection for `Subscribe` — the daemon
  permanently converts a subscribed connection to event-only mode, so it can
  never be reused for requests), `DaemonProcess`/`PManagerPaths` (auto-spawn).
- `Sources/PManagerCore/ViewModel/MenuBarViewModel.swift` — connection
  lifecycle, tunnel state, start/stop intents.
- `Sources/PManagerMenuBar/Views/` and `PManagerMenuBarApp.swift` — the
  `MenuBarExtra` UI.

## Status

Builds clean (`swift build`) and has been verified end-to-end against a real
daemon: auto-spawn, `ListTunnels`/`ListProfiles`, `Subscribe`, and a live
`StartTunnel`/`StopTunnel` round trip observed through the event stream
(`Connecting` → `Connected` → `Stopping` → `Stopped`). The GUI itself
(`swift run PManagerMenuBar`) launches and stays connected without crashing;
visually confirming the menu bar icon/dropdown still needs a manual check on
a machine with normal display access.
