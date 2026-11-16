import SwiftUI
import UIKit
import UniformTypeIdentifiers
import SafariServices

/// Nova's game-ready home screen. Keep all launcher presentation on the iOS
/// side; this view never touches the upstream Horizon engine or its controls.
struct NovaMenuView: View {
    let onPlay: () -> Void
    let onEdit: () -> Void
    let onSettings: () -> Void
    let onLogs: () -> Void
    let onClose: () -> Void

    @State private var confirmClose = false
    @State private var showDiscord = false
    @State private var showGameFolder = false
    @State private var gameFolder: URL?
    @State private var folderError: String?

    // Exact colors requested for the redesign.
    private enum Palette {
        static let header = Color(red: 0x38 / 255.0, green: 0x38 / 255.0, blue: 0x38 / 255.0)
        static let background = Color(red: 0x75 / 255.0, green: 0x75 / 255.0, blue: 0x75 / 255.0)
        static let forceClose = Color(red: 1.0, green: 0x42 / 255.0, blue: 0x42 / 255.0)
        static let button = Color(red: 0xA4 / 255.0, green: 0x66 / 255.0, blue: 1.0)
    }

    var body: some View {
        GeometryReader { geometry in
            let width = geometry.size.width
            let height = geometry.size.height
            let columnWidth = max(150, width * 0.25)
            let tileHeight = max(44, height * 0.095)
            let gap = max(7, height * 0.016)
            let labelSize = max(16, min(29, height * 0.033))

            VStack(spacing: 0) {
                header(width: width, height: height * 0.20)
                    .frame(height: height * 0.20)
                    .background(Palette.header)

                ZStack(alignment: .bottom) {
                    Palette.background

                    HStack(alignment: .bottom, spacing: 0) {
                        VStack(spacing: gap) {
                            tile("custom controls", size: labelSize, action: onEdit)
                            tile("mouse settings", size: labelSize, action: onSettings)
                            tile("logs", size: labelSize, action: onLogs)
                        }
                        .frame(width: columnWidth)
                        .frame(height: tileHeight * 3 + gap * 2)

                        Spacer(minLength: 0)

                        VStack(spacing: gap) {
                            tile("discord", size: labelSize) { showDiscord = true }
                            // These entries are deliberately visual-only until the
                            // download features are implemented.
                            tile("download shaders", size: labelSize, enabled: false) {}
                            tile("download resources packs", size: labelSize, enabled: false) {}
                            tile("open game directory", size: labelSize) { openGameFolder() }
                        }
                        .frame(width: columnWidth)
                        .frame(height: tileHeight * 4 + gap * 3)
                    }
                    .padding(.horizontal, max(8, width * 0.01))
                    .padding(.bottom, max(6, height * 0.008))

                    tile("PLAY", size: max(28, min(56, height * 0.06)), action: onPlay)
                        .frame(width: width * 0.275, height: max(60, height * 0.138))
                        .padding(.bottom, height * 0.085)
                }
            }
            .frame(width: width, height: height)
        }
        .ignoresSafeArea()
        .sheet(isPresented: $showDiscord) {
            NovaDiscordBrowser()
        }
        .sheet(isPresented: $showGameFolder) {
            if let gameFolder {
                NovaGameFolderPicker(startingDirectory: gameFolder)
            }
        }
        .alert("Unable to open game directory",
               isPresented: Binding(get: { folderError != nil },
                                    set: { if !$0 { folderError = nil } })) {
            Button("OK", role: .cancel) { folderError = nil }
        } message: {
            Text(folderError ?? "The game folder is unavailable.")
        }
        .confirmationDialog("Force close Nova?", isPresented: $confirmClose) {
            Button("Force Close App", role: .destructive, action: onClose)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The app exits immediately. Unsaved world changes can be lost.")
        }
    }

    private func header(width: CGFloat, height: CGFloat) -> some View {
        let headSize = min(height * 0.78, 140)
        return HStack(spacing: max(14, width * 0.035)) {
            // No account system is installed yet. Use the default Steve head;
            // account switching and authenticated IGN-based skins come later.
            AsyncImage(url: URL(string: "https://mc-heads.net/head/MHF_Steve/128")) { image in
                image.resizable()
                    .interpolation(.none)
                    .scaledToFit()
            } placeholder: {
                Image(systemName: "person.crop.square.fill")
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.white.opacity(0.75))
            }
            .frame(width: headSize, height: headSize)
            .accessibilityLabel("Default Steve skin")

            Text("Steve")
                .font(.system(size: max(24, min(50, height * 0.31)), weight: .regular))
                .foregroundStyle(.white)
                .lineLimit(1)

            Spacer(minLength: 8)

            tile("force close", tint: Palette.forceClose,
                 size: max(17, min(32, height * 0.18))) {
                confirmClose = true
            }
            .frame(width: max(135, width * 0.19),
                   height: max(44, height * 0.55))
        }
        .padding(.horizontal, max(12, width * 0.013))
    }

    private func tile(_ title: String,
                      tint: Color = Palette.button,
                      size: CGFloat,
                      enabled: Bool = true,
                      action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: size, weight: .regular))
                .foregroundStyle(.black)
                .lineLimit(1)
                .minimumScaleFactor(0.52)
                .padding(.horizontal, 5)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(tint)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityHint(enabled ? "" : "Coming in a future update")
    }

    private func openGameFolder() {
        do {
            gameFolder = try NovaGameFiles.gameDirectory()
            showGameFolder = true
        } catch {
            folderError = error.localizedDescription
        }
    }
}

/// Safari's in-app browser avoids requiring the Discord app to be installed.
private struct NovaDiscordBrowser: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> SFSafariViewController {
        SFSafariViewController(url: URL(string: "https://discord.gg/aH9mtNDc24")!)
    }

    func updateUIViewController(_ controller: SFSafariViewController, context: Context) {}
}

/// Present the system Files picker at the real Documents/Game folder.
/// Do not copy, export, or move large worlds/resources just to browse them.
private struct NovaGameFolderPicker: UIViewControllerRepresentable {
    let startingDirectory: URL

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: false)
        picker.directoryURL = startingDirectory
        picker.shouldShowFileExtensions = true
        return picker
    }

    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}
}
