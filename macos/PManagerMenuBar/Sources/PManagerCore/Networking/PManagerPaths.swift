import Foundation

/// Mirrors src/ipc/mod.rs's `config_dir`/`socket_path` — same layout the
/// daemon and TUI already use.
enum PManagerPaths {
    static var configDir: String {
        "\(NSHomeDirectory())/.config/pmanager"
    }

    static var socketPath: String {
        "\(configDir)/pmanager.sock"
    }

    static var daemonLogPath: String {
        "\(configDir)/daemon.log"
    }

    /// Opens (creating if needed) the daemon log file for appending, matching
    /// spawn_daemon()'s `OpenOptions::new().create(true).append(true)`.
    static func openDaemonLogForAppending() throws -> FileHandle {
        let fileManager = FileManager.default
        try fileManager.createDirectory(atPath: configDir, withIntermediateDirectories: true)
        if !fileManager.fileExists(atPath: daemonLogPath) {
            fileManager.createFile(atPath: daemonLogPath, contents: nil)
        }
        guard let handle = FileHandle(forWritingAtPath: daemonLogPath) else {
            throw DaemonError(message: "Could not open \(daemonLogPath) for writing")
        }
        handle.seekToEndOfFile()
        return handle
    }
}
