import Foundation

/// Locates and spawns the `pmanager` daemon, mirroring `spawn_daemon()` in
/// src/client/mod.rs. Unlike the Rust CLI, this GUI app has no "current
/// binary" to re-invoke as `daemon` — it wasn't launched as `pmanager`, and
/// GUI apps don't inherit the interactive shell PATH — so it probes a short
/// list of common install locations instead. If none are found, it simply
/// doesn't spawn; the caller falls back to a "daemon not running" UI state
/// rather than failing silently.
enum DaemonProcess {
    static let candidatePaths: [String] = [
        "\(NSHomeDirectory())/.cargo/bin/pmanager",
        "/usr/local/bin/pmanager",
        "/opt/homebrew/bin/pmanager",
    ]

    static func locateBinary() -> String? {
        let fileManager = FileManager.default
        return candidatePaths.first { fileManager.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func spawnDaemonIfPossible() -> Bool {
        guard let binaryPath = locateBinary() else { return false }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: binaryPath)
        process.arguments = ["daemon"]
        process.standardInput = FileHandle.nullDevice

        if let logHandle = try? PManagerPaths.openDaemonLogForAppending() {
            process.standardOutput = logHandle
            process.standardError = logHandle
        }

        do {
            try process.run()
            return true
        } catch {
            return false
        }
    }
}
