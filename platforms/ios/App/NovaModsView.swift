import SwiftUI

private struct ModInventory: Decodable {
    struct Mod: Identifiable, Decodable {
        let id: String
        let name: String
        var enabled: Bool
    }

    var mods: [Mod]
    let problems: [String]

    static func scan(in home: URL) async throws -> Self {
        var error = [CChar](repeating: 0, count: 1024)
        let capacity = UInt(error.count)
        guard let json = nova_ios_scan_mods(home.path, &error, capacity) else {
            throw ModFileError(message: String(cString: error))
        }
        defer { nova_ios_free_mod_list(json) }
        return try JSONDecoder().decode(Self.self, from: Data(bytes: json, count: strlen(json)))
    }
}

private struct ModFileError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

struct NovaModsView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var gameHome: URL?
    @State private var inventory: ModInventory?
    @State private var loadError: String?
    @State private var saveError: String?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if let loadError {
                        Text(loadError)
                        Button("Try again") { Task { await load() } }
                    } else if let inventory {
                        if inventory.mods.isEmpty {
                            Text("No mods found.")
                        }
                        ForEach(inventory.mods) { mod in
                            Toggle(isOn: Binding(get: { mod.enabled }, set: { save($0, for: mod.id) })) {
                                VStack(alignment: .leading) {
                                    Text(mod.name)
                                    if mod.name != mod.id {
                                        Text(mod.id).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                            }
                        }
                    } else {
                        ProgressView("Scanning mods…")
                    }
                } footer: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("restart to take effect")
                        Text("Add mods in Files → Nova → Game → horizon-mods.")
                    }
                    .font(.footnote)
                }
                if let problems = inventory?.problems, !problems.isEmpty {
                    Section("Package issues") {
                        ForEach(problems, id: \.self) { Text($0) }
                    }
                }
            }
            .navigationTitle("Manage mods")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await load() }
            .alert("Couldn’t save mod setting", isPresented: Binding(
                get: { saveError != nil }, set: { if !$0 { saveError = nil } }
            )) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(saveError ?? "")
            }
        }
    }

    @MainActor private func load() async {
        loadError = nil
        do {
            let home = try NovaGameFiles.gameDirectory()
            gameHome = home
            inventory = try await ModInventory.scan(in: home)
        } catch {
            loadError = error.localizedDescription
        }
    }

    private func save(_ enabled: Bool, for id: String) {
        guard let gameHome, let index = inventory?.mods.firstIndex(where: { $0.id == id }) else { return }
        var error = [CChar](repeating: 0, count: 1024)
        let capacity = UInt(error.count)
        if nova_ios_set_mod_enabled(gameHome.path, id, enabled, &error, capacity) {
            inventory?.mods[index].enabled = enabled
        } else {
            saveError = String(cString: error)
        }
    }
}
