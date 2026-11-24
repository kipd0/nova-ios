import Foundation
import Darwin

/// Per-process diagnostics visible in Files → Nova → Diagnostics.
///
/// The live session is lastlog.text. At the next cold start, that complete
/// file is renamed to its creation-time timestamp, even if the previous
/// session crashed or was force-closed. Do not clear the file while running.
///
/// These files can contain player/server information from Horizon's own logs.
/// Nothing is uploaded automatically.
enum NovaDiagnostics {
    private static let lock = NSLock()
    private static var currentURL: URL?

    /// Initialize before recording the very first process event. Calling this
    /// again in the same process must not rotate the current session.
    static func startSession() {
        _ = logURL()
    }

    static func logURL() -> URL? {
        lock.lock()
        defer { lock.unlock() }
        return logURLLocked()
    }

    private static func logURLLocked() -> URL? {
        if let currentURL { return currentURL }
        do {
            let folder = try NovaVisibleStorage.directory(named: "Diagnostics")
            let url = folder.appendingPathComponent("lastlog.text")
            if FileManager.default.fileExists(atPath: url.path) {
                // The archived name describes when the previous session began,
                // not when the new session was started.
                let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
                let started = (attrs[.creationDate] as? Date)
                    ?? (attrs[.modificationDate] as? Date) ?? Date()
                let format = DateFormatter()
                format.locale = Locale(identifier: "en_US_POSIX")
                format.dateFormat = "yyyy-MM-dd_HH-mm-ss"
                let name = format.string(from: started)
                var archive = folder.appendingPathComponent("\(name).text")
                var suffix = 2
                while FileManager.default.fileExists(atPath: archive.path) {
                    archive = folder.appendingPathComponent("\(name)-\(suffix).text")
                    suffix += 1
                }
                try FileManager.default.moveItem(at: url, to: archive)
            }
            guard FileManager.default.createFile(atPath: url.path, contents: Data()) else {
                return nil
            }
            currentURL = url
            return url
        } catch {
            NSLog("Nova: diagnostic session unavailable: %@", error.localizedDescription)
            return nil
        }
    }

    /// Do not mirror unfiltered stdout/stderr into the Files-visible
    /// Diagnostics folder. Native frameworks and unforeseen failures can print
    /// credentials even when our own account code never does.
    static func captureConsole() {
        record("diagnostics.raw-console-capture-disabled; privacy-protection")
    }

    private static func sanitized(_ message: String) -> String {
        let flattened = message.replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
        let lower = flattened.lowercased()
        let privateMarkers = [
            "access_token", "refresh_token", "id_token", "device_code",
            "user_code", "access token", "refresh token", "bearer ",
            "authorization:", "password", "client_secret", "cookie:",
            "xbl3.0", "session-file", "session_file", "login.microsoftonline.com",
            "api.minecraftservices.com/authentication"
        ]
        if privateMarkers.contains(where: { lower.contains($0) }) {
            return "[redacted credential-related diagnostic]"
        }
        let patterns = [
            #"[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}"#,
            #"\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*\b"#,
            #"[A-Za-z0-9_+/-]{100,}={0,2}"#
        ]
        return patterns.reduce(flattened) { text, pattern in
            guard let regex = try? NSRegularExpression(pattern: pattern, options: .caseInsensitive) else {
                return text
            }
            return regex.stringByReplacingMatches(in: text, range: NSRange(text.startIndex..., in: text),
                                                  withTemplate: "[redacted]")
        }
    }

    static func record(_ stage: String) {
        lock.lock()
        defer { lock.unlock() }
        guard let url = logURLLocked() else { return }
        let stamp = ISO8601DateFormatter().string(from: Date())
        let safeStage = sanitized(stage)
        let bytes = Array("[\(stamp)] [swift] \(safeStage)\n".utf8)
        // O_APPEND prevents concurrent Rust/Swift writes from seeking to an
        // outdated end-of-file offset and overwriting one another.
        let descriptor = url.path.withCString { Darwin.open($0, O_WRONLY | O_APPEND) }
        guard descriptor >= 0 else {
            NSLog("Nova: could not open diagnostic session for writing")
            return
        }
        defer { _ = Darwin.close(descriptor) }
        let complete = bytes.withUnsafeBytes { buffer -> Bool in
            guard let base = buffer.baseAddress else { return false }
            var offset = 0
            while offset < buffer.count {
                let result = Darwin.write(descriptor, base.advanced(by: offset),
                                          buffer.count - offset)
                if result > 0 {
                    offset += result
                } else if result == -1 && errno == EINTR {
                    continue
                } else {
                    return false
                }
            }
            return true
        }
        if !complete {
            NSLog("Nova: could not complete diagnostic session write")
        }
    }

    /// Read only the last 32 KB, even when a long gameplay session generated
    /// a large log. The full file remains available in Files.
    static func tail() -> String {
        guard let url = logURL() else {
            return "No diagnostics have been recorded yet."
        }
        do {
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            let end = try handle.seekToEnd()
            try handle.seek(toOffset: end > 32_000 ? end - 32_000 : 0)
            let data = try handle.readToEnd() ?? Data()
            return String(decoding: data, as: UTF8.self)
        } catch {
            return "Could not read diagnostics: \(error.localizedDescription)"
        }
    }
}
