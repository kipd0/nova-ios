import CryptoKit
import Foundation

enum NovaPackCategory: String, Identifiable {
    case resourcePack = "resourcepack"
    case shader

    var id: Self { self }
    var title: String { self == .shader ? "Shaders" : "Resource packs" }
    var folder: String { self == .shader ? "shaderpack" : "resourcepack" }
}

enum NovaModrinth {
    struct Project: Decodable, Identifiable {
        let projectId: String
        let title: String
        let author: String
        var id: String { projectId }
    }

    struct Version: Decodable, Identifiable {
        let id: String
        let name: String
        let gameVersions: [String]
        let files: [PackFile]

        var archive: PackFile? {
            let archives = files.filter { $0.filename.lowercased().hasSuffix(".zip") }
            return archives.first(where: { $0.primary }) ?? archives.first
        }
    }

    struct PackFile: Decodable {
        let url: URL
        let filename: String
        let primary: Bool
        let hashes: [String: String]
    }

    private struct SearchResults: Decodable {
        let hits: [Project]
    }

    private static let api = URL(string: "https://api.modrinth.com/v2")!

    static func search(_ category: NovaPackCategory, query: String) async throws -> [Project] {
        var url = URLComponents(url: api.appendingPathComponent("search"), resolvingAgainstBaseURL: false)!
        url.queryItems = [
            URLQueryItem(name: "facets", value: "[[\"project_type:\(category.rawValue)\"]]"),
            URLQueryItem(name: "query", value: query),
            URLQueryItem(name: "limit", value: "50")
        ]
        guard let endpoint = url.url else { throw URLError(.badURL) }
        let results: SearchResults = try await fetch(endpoint)
        return results.hits
    }

    static func versions(for project: Project) async throws -> [Version] {
        let endpoint = api.appendingPathComponent("project")
            .appendingPathComponent(project.id).appendingPathComponent("version")
        let versions: [Version] = try await fetch(endpoint)
        return versions.filter { $0.archive != nil }
    }

    private static func request(_ url: URL) -> URLRequest {
        var request = URLRequest(url: url)
        request.setValue("kipd0/nova-ios (https://github.com/kipd0/nova-ios)", forHTTPHeaderField: "User-Agent")
        return request
    }

    private static func checkResponse(_ response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse else { throw URLError(.badServerResponse) }
        guard (200..<300).contains(http.statusCode) else {
            throw PackError.message("Modrinth returned HTTP \(http.statusCode). Try again later.")
        }
    }

    private static func fetch<T: Decodable>(_ url: URL) async throws -> T {
        let (data, response) = try await URLSession.shared.data(for: request(url))
        try checkResponse(response)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(T.self, from: data)
    }

    static func install(_ file: PackFile, in category: NovaPackCategory) async throws {
        let name = file.filename
        guard file.url.scheme == "https", name.lowercased().hasSuffix(".zip"),
              !name.contains("/"), !name.contains("\\"), !name.contains("\0") else {
            throw PackError.message("Modrinth returned an invalid archive.")
        }
        let manager = FileManager.default
        let folder = try NovaGameFiles.gameDirectory().appendingPathComponent(category.folder, isDirectory: true)
        try manager.createDirectory(at: folder, withIntermediateDirectories: true)
        let destination = folder.appendingPathComponent(name)
        guard !manager.fileExists(atPath: destination.path) else {
            throw PackError.message("\(name) already exists in Game/\(category.folder).")
        }

        let (temporary, response) = try await URLSession.shared.download(for: request(file.url))
        defer { try? manager.removeItem(at: temporary) }
        try checkResponse(response)
        let handle = try FileHandle(forReadingFrom: temporary)
        defer { try? handle.close() }
        var hash = SHA512()
        while let chunk = try handle.read(upToCount: 1024 * 1024), !chunk.isEmpty {
            try Task.checkCancellation()
            hash.update(data: chunk)
        }
        let digest = hash.finalize().map { String(format: "%02x", $0) }.joined()
        guard digest == file.hashes["sha512"]?.lowercased() else {
            throw PackError.message("The downloaded archive failed verification. Please try again.")
        }
        try Task.checkCancellation()
        try manager.moveItem(at: temporary, to: destination)
    }

    private enum PackError: LocalizedError {
        case message(String)
        var errorDescription: String? {
            switch self { case .message(let text): return text }
        }
    }
}
