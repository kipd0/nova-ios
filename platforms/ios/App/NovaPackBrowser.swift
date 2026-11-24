import SwiftUI

struct NovaPackBrowser: View {
    let category: NovaPackCategory
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    var body: some View {
        NavigationStack {
            PackList(emptyMessage: "No matching packs.", load: {
                if !query.isEmpty { try await Task.sleep(nanoseconds: 300_000_000) }
                return try await NovaModrinth.search(category, query: query)
            }) { project in
                NavigationLink {
                    PackVersions(project: project, category: category)
                } label: {
                    VStack(alignment: .leading) {
                        Text(project.title)
                        Text(project.author).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .id(query)
            .searchable(text: $query, prompt: "Search Modrinth")
            .navigationTitle(category.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}

private struct PackVersions: View {
    let project: NovaModrinth.Project
    let category: NovaPackCategory
    @State private var downloading: NovaModrinth.Version?
    @State private var notice: String?

    var body: some View {
        PackList(emptyMessage: "No downloadable ZIP versions.", load: {
            try await NovaModrinth.versions(for: project)
        }) { version in
            Button {
                downloading = version
            } label: {
                HStack {
                    VStack(alignment: .leading) {
                        Text(version.name)
                        Text(version.gameVersions.joined(separator: ", "))
                            .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                    }
                    Spacer()
                    if downloading?.id == version.id {
                        ProgressView()
                    } else {
                        Image(systemName: "arrow.down.circle")
                    }
                }
            }
            .disabled(downloading != nil)
            .accessibilityHint("Download this version")
        }
        .navigationTitle(project.title)
        .navigationBarTitleDisplayMode(.inline)
        .task(id: downloading?.id) {
            guard let file = downloading?.archive else { return }
            defer { downloading = nil }
            do {
                try await NovaModrinth.install(file, in: category)
                notice = "Saved \(file.filename) to Nova/Game/\(category.folder)."
            } catch {
                if !Task.isCancelled { notice = error.localizedDescription }
            }
        }
        .alert("Pack download", isPresented: Binding(get: { notice != nil }, set: {
            if !$0 { notice = nil }
        })) {
            Button("OK") { notice = nil }
        } message: {
            Text(notice ?? "")
        }
    }
}

private struct PackList<Item: Identifiable, Row: View>: View {
    let emptyMessage: String
    let load: () async throws -> [Item]
    @ViewBuilder let row: (Item) -> Row
    @State private var items: [Item]?
    @State private var errorMessage: String?
    @State private var attempt = 0

    var body: some View {
        Group {
            if let errorMessage {
                VStack {
                    Text(errorMessage)
                    Button("Retry") { attempt += 1 }
                }.padding()
            } else if let items {
                if items.isEmpty {
                    Text(emptyMessage)
                } else {
                    List(items, rowContent: row)
                }
            } else {
                ProgressView("Loading from Modrinth…")
            }
        }
        .task(id: attempt) {
            errorMessage = nil
            do {
                let result = try await load()
                try Task.checkCancellation()
                items = result
            } catch {
                if !Task.isCancelled { errorMessage = error.localizedDescription }
            }
        }
    }
}
