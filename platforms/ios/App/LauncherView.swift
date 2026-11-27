import SwiftUI
import UniformTypeIdentifiers

/// Nova is the thin iOS host for the ORIGINAL Horizon game.
/// The gameplay runtime is linked in the integrated build but cannot yet
/// own an iOS event loop; do not invoke its desktop-style entry from SwiftUI.
struct LauncherView: View {
    @State private var showArchivePicker = false
    @State private var showMetalDiagnostic = false
    @State private var showDiagnostics = false
    @State private var showTouchEditor = false
    @State private var showSettings = false
    @State private var displayedDiagnostics = ""
    @State private var archiveName: String? = NovaGameFiles.stagedArchiveName()
    @State private var archiveMessage: String?
    @State private var importInProgress = false
    @State private var prepareInProgress = false
    @State private var resourcesInstalled = NovaGameFiles.resourcesPresent()
    @StateObject private var linkDownloader = NovaResourceLinkDownloader()
    @State private var resourceURL = "https://www.dropbox.com/scl/fi/ehz2qwar0ek8rvtq9spx2/NovaResources.zip?rlkey=0dksjly4cqsgb12w1iielar76&st=pjv4pckp&dl=1"

    @State private var nextBootMessage: String?

    // First resource import uses SwiftUI; subsequent game-ready launches show
    // Nova's UI inside the same winit-owned UIKit window.
    private var canTestHorizonBoot: Bool {
        resourcesInstalled && horizon_ios_host_api_version() >= 2
    }

    var body: some View {
        ZStack {
            Color(red: 0.06, green: 0.09, blue: 0.13)
                .ignoresSafeArea()

            ScrollView {
                VStack(spacing: 18) {
                    Text("Nova")
                        .font(.system(size: 46, weight: .bold, design: .rounded))
                        .foregroundStyle(.white)
                        .accessibilityAddTraits(.isHeader)

                    Text("Original Horizon for iOS / iPadOS")
                        .font(.headline)
                        .foregroundStyle(.white.opacity(0.75))

                    Button("Finish setup — launch on next opening") {
                        // A second UIApplicationMain inside SwiftUI is illegal.
                        // Save a one-shot request and show explicit restart steps.
                        NovaDiagnostics.record("launcher.game-boot-armed")
                        UserDefaults.standard.set(true, forKey: "nova.launchHorizonOnce")
                        nextBootMessage = "Close Nova using the app switcher and reopen it once after setup. On future launches, Play opens the already running game without another restart."
                    }
                    .font(.headline)
                    .frame(minWidth: 180, minHeight: 48)
                    .buttonStyle(.borderedProminent)
                    .disabled(!canTestHorizonBoot)
                    .accessibilityHint("Start Horizon's winit lifecycle on the next app launch, after closing Nova")

                    if let nextBootMessage {
                        Text(nextBootMessage)
                            .foregroundStyle(.orange)
                            .multilineTextAlignment(.center)
                    } else {
                        Text(canTestHorizonBoot
                             ? "After the first setup, reopen Nova once to start Horizon. Future Play presses use the in-game Nova launcher without restarting."
                             : "Prepare your game files to enable the experimental Horizon startup test. The full game build is required.")
                            .foregroundStyle(.white.opacity(0.75))
                            .multilineTextAlignment(.center)
                    }

                    Button("Settings") {
                        showSettings = true
                    }
                    .buttonStyle(.bordered)

                    Button("Touch Layout") {
                        showTouchEditor = true
                    }
                    .buttonStyle(.bordered)

                    Text("Current session: Diagnostics/lastlog.text. Earlier sessions have dated filenames in Files → Nova → Diagnostics.")
                        .font(.footnote)
                        .foregroundStyle(.white.opacity(0.6))
                        .multilineTextAlignment(.center)

                    Button("View Logs") {
                        displayedDiagnostics = NovaDiagnostics.tail()
                        showDiagnostics = true
                    }
                    .buttonStyle(.bordered)

                    if let logURL = NovaDiagnostics.logURL() {
                        ShareLink(item: logURL, preview: SharePreview("Nova diagnostics")) {
                            Label("Export Logs", systemImage: "square.and.arrow.up")
                        }
                        .buttonStyle(.bordered)
                    }

                    Button("Test Metal display") {
                        showMetalDiagnostic = true
                    }
                    .buttonStyle(.bordered)
                    .accessibilityHint("Opens a GPU display test, not Horizon gameplay")

                    Divider().overlay(.white.opacity(0.2))

                    Text("Game files")
                        .font(.title3.bold())
                        .foregroundStyle(.white)

                    Text(resourcesInstalled ? "Prepared resources installed locally." : "No validated game resources installed.")
                        .foregroundStyle(resourcesInstalled ? .green : .white.opacity(0.75))
                        .multilineTextAlignment(.center)

                    if let archiveName {
                        Text("Imported ZIP: \(archiveName)")
                            .font(.footnote)
                            .foregroundStyle(.white.opacity(0.7))
                    }

                    if !resourcesInstalled {
                        Text("Download prepared game files")
                            .font(.headline)
                            .foregroundStyle(.white)

                        TextField("HTTPS URL of a prepared resources ZIP", text: $resourceURL)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.URL)
                            .textContentType(.URL)
                            .textFieldStyle(.roundedBorder)
                            .accessibilityLabel("Resources ZIP download URL")

                        Button(linkDownloader.isDownloading ? "Downloading…" : "Download and install from link") {
                            downloadAndInstallResources()
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(linkDownloader.isDownloading || importInProgress || prepareInProgress)

                        if linkDownloader.isDownloading {
                            if let fraction = linkDownloader.progress {
                                ProgressView(value: fraction)
                                    .tint(.white)
                            } else {
                                ProgressView().tint(.white)
                            }
                            Text(downloadProgressText)
                                .font(.footnote)
                                .foregroundStyle(.white.opacity(0.75))
                            Button("Cancel download") { linkDownloader.cancel() }
                                .buttonStyle(.bordered)
                        }
                    }

                    Button(importInProgress ? "Importing…" : "Import game files ZIP") {
                        showArchivePicker = true
                    }
                    .buttonStyle(.bordered)
                    .disabled(importInProgress || prepareInProgress || linkDownloader.isDownloading)

                    Button(prepareInProgress ? "Preparing resources…" : "Extract and verify game files") {
                        extractAndVerifyResources()
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(archiveName == nil || importInProgress || prepareInProgress || linkDownloader.isDownloading)

                    if prepareInProgress {
                        ProgressView().tint(.white)
                    }

                    Text("Nova does not bundle game files. Link downloads come from the URL you enter. Use a source you trust and install only resources you have rights to use.")
                        .font(.footnote)
                        .foregroundStyle(.white.opacity(0.55))
                        .multilineTextAlignment(.center)

                    if let archiveMessage {
                        Text(archiveMessage)
                            .font(.footnote)
                            .foregroundStyle(.orange)
                            .multilineTextAlignment(.center)
                    }

                    Text("Host bridge API v\(horizon_ios_host_api_version())")
                        .font(.footnote.monospaced())
                        .foregroundStyle(.white.opacity(0.5))
                }
                .frame(maxWidth: 540)
                .padding(28)
                .frame(maxWidth: .infinity)
            }
        }
        .fullScreenCover(isPresented: $showTouchEditor) {
            NovaTouchLayoutEditor(onDone: {
                showTouchEditor = false
            }, onSave: { layout in
                NovaDiagnostics.record("touch.layout.saved; buttons=\(layout.controls.count)")
            })
        }
        .fullScreenCover(isPresented: $showSettings) {
            NovaTouchSettingsView(onDone: { showSettings = false })
        }
        .sheet(isPresented: $showDiagnostics) {
            VStack(spacing: 12) {
                Text("Nova startup diagnostics")
                    .font(.headline)
                Text("Records the launcher, game startup, window creation and rendering stages. Export only if you're comfortable sharing the log.")
                    .font(.footnote)
                    .multilineTextAlignment(.center)
                ScrollView {
                    Text(displayedDiagnostics)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                        .padding(12)
                }
                Button("Close") { showDiagnostics = false }
                    .buttonStyle(.borderedProminent)
            }
            .padding()
        }
        .sheet(isPresented: $showMetalDiagnostic) {
            VStack(spacing: 12) {
                Text("Nova Metal surface test")
                    .font(.title3.bold())
                Text("This is a GPU display diagnostic, not the Horizon game.")
                    .font(.footnote)
                GameSurfaceView()
                    .frame(minHeight: 180)
                    .clipShape(RoundedRectangle(cornerRadius: 12))
                Button("Close") { showMetalDiagnostic = false }
                    .buttonStyle(.borderedProminent)
            }
            .padding()
        }
        .fileImporter(
            isPresented: $showArchivePicker,
            allowedContentTypes: [.zip],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case .success(let urls):
                guard let url = urls.first else { return }
                importInProgress = true
                DispatchQueue.global(qos: .userInitiated).async {
                    let result = Result { try NovaGameFiles.stageZip(from: url) }
                    DispatchQueue.main.async {
                        importInProgress = false
                        switch result {
                        case .success(let name):
                            NovaDiagnostics.record("resources.archive-imported")
                            archiveName = name
                            archiveMessage = "ZIP imported locally. Tap Extract and verify game files."
                        case .failure(let error):
                            NovaDiagnostics.record("resources.archive-import-failed")
                            archiveMessage = "Could not import ZIP: \(error.localizedDescription)"
                        }
                    }
                }
            case .failure(let error):
                archiveMessage = "File selection failed: \(error.localizedDescription)"
            }
        }
    }

    private var downloadProgressText: String {
        let done = ByteCountFormatter.string(fromByteCount: linkDownloader.downloadedBytes,
                                             countStyle: .file)
        if linkDownloader.totalBytes > 0 {
            let total = ByteCountFormatter.string(fromByteCount: linkDownloader.totalBytes,
                                                  countStyle: .file)
            return "Downloaded \(done) of \(total). Keep Nova open."
        }
        return "Downloaded \(done). Keep Nova open."
    }

    private func downloadAndInstallResources() {
        NovaDiagnostics.record("resources.download.started")
        archiveMessage = "Downloading game resources. Keep Nova open."
        linkDownloader.start(resourceURL) { result in
            switch result {
            case .success(let name):
                NovaDiagnostics.record("resources.download.succeeded")
                archiveName = name
                archiveMessage = "Download complete. Extracting and verifying resources…"
                extractAndVerifyResources()
            case .failure(let error):
                if (error as? URLError)?.code == .cancelled {
                    NovaDiagnostics.record("resources.download.cancelled")
                    archiveMessage = "Download cancelled. You can try again."
                } else {
                    NovaDiagnostics.record("resources.download.failed")
                    archiveMessage = "Download failed: \(error.localizedDescription)"
                }
            }
        }
    }

    private func extractAndVerifyResources() {
        guard !prepareInProgress else { return }
        NovaDiagnostics.record("resources.extraction.started")
        prepareInProgress = true
        archiveMessage = "Extracting and verifying the ZIP. Keep Nova open."
        DispatchQueue.global(qos: .userInitiated).async {
            let result = Result { try NovaGameFiles.prepareStoredZip() }
            DispatchQueue.main.async {
                prepareInProgress = false
                resourcesInstalled = NovaGameFiles.resourcesPresent()
                archiveName = NovaGameFiles.stagedArchiveName()
                switch result {
                case .success:
                    NovaDiagnostics.record("resources.extraction.succeeded")
                    archiveMessage = "Resources installed. Close and reopen Nova to start the game."
                case .failure(let error):
                    NovaDiagnostics.record("resources.extraction.failed")
                    archiveMessage = "Installation failed: \(error.localizedDescription)"
                }
            }
        }
    }

}

// Downloads a ZIP into a URLSession temporary file. Only the existing
// NovaGameFiles.stageDownloadedZip/prepareStoredZip path may install its contents.
// This type is private to the first-run launcher; the in-game UI is unchanged.
private final class NovaResourceLinkDownloader: NSObject, ObservableObject, URLSessionDownloadDelegate {
    @Published private(set) var isDownloading = false
    @Published private(set) var progress: Double?
    @Published private(set) var downloadedBytes: Int64 = 0
    @Published private(set) var totalBytes: Int64 = 0

    private static let maxArchiveBytes: Int64 = 3 * 1024 * 1024 * 1024
    private var session: URLSession?
    private var task: URLSessionDownloadTask?
    private var completion: ((Result<String, Error>) -> Void)?
    // The URLSession delegate queue serializes these two values.
    private var stagedName: String?
    private var downloadError: Error?

    enum Failure: LocalizedError {
        case invalidURL
        case insecureRedirect
        case httpStatus(Int)
        case archiveTooLarge
        case notAnArchive

        var errorDescription: String? {
            switch self {
            case .invalidURL: return "Enter a valid HTTPS download link."
            case .insecureRedirect: return "The download redirected to an unsafe HTTP address."
            case .httpStatus(let code): return "The download server returned HTTP \(code)."
            case .archiveTooLarge: return "The ZIP is larger than Nova's 3 GiB download limit."
            case .notAnArchive: return "The link did not return a valid resources ZIP."
            }
        }
    }

    func start(_ link: String, completion: @escaping (Result<String, Error>) -> Void) {
        guard !isDownloading else { return }
        let text = link.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let components = URLComponents(string: text),
              components.scheme?.lowercased() == "https",
              let host = components.host, !host.isEmpty,
              components.user == nil, components.password == nil,
              let url = components.url else {
            completion(.failure(Failure.invalidURL))
            return
        }
        self.completion = completion
        stagedName = nil
        downloadError = nil
        downloadedBytes = 0
        totalBytes = 0
        progress = nil
        isDownloading = true

        // An ephemeral session sends no Microsoft account credentials or
        // persistent cookies to the user-supplied download server.
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 60
        configuration.timeoutIntervalForResource = 4 * 60 * 60
        configuration.urlCache = nil
        let session = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
        self.session = session
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        request.cachePolicy = .reloadIgnoringLocalCacheData
        let task = session.downloadTask(with: request)
        self.task = task
        task.resume()
    }

    func cancel() {
        task?.cancel()
    }

    func urlSession(_ session: URLSession, task: URLSessionTask,
                    willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest,
                    completionHandler: @escaping (URLRequest?) -> Void) {
        guard request.url?.scheme?.lowercased() == "https" else {
            downloadError = Failure.insecureRedirect
            completionHandler(nil)
            return
        }
        completionHandler(request)
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                    didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64,
                    totalBytesExpectedToWrite: Int64) {
        if totalBytesWritten > Self.maxArchiveBytes ||
           totalBytesExpectedToWrite > Self.maxArchiveBytes {
            downloadError = Failure.archiveTooLarge
            downloadTask.cancel()
            return
        }
        DispatchQueue.main.async {
            self.downloadedBytes = totalBytesWritten
            self.totalBytes = max(0, totalBytesExpectedToWrite)
            self.progress = totalBytesExpectedToWrite > 0
                ? min(1, Double(totalBytesWritten) / Double(totalBytesExpectedToWrite))
                : nil
        }
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                    didFinishDownloadingTo location: URL) {
        if let response = downloadTask.response as? HTTPURLResponse,
           !(200...299).contains(response.statusCode) {
            downloadError = Failure.httpStatus(response.statusCode)
            return
        }
        do {
            // Move the temporary download into the existing installer cache.
            // An invalid ZIP never replaces the installed resources.
            stagedName = try NovaGameFiles.stageDownloadedZip(from: location)
        } catch NovaGameFiles.ArchiveError.invalidZip {
            downloadError = Failure.notAnArchive
            NovaDiagnostics.record("resources.download.invalid-archive")
        } catch {
            downloadError = error
            NovaDiagnostics.record("resources.download.staging-failed")
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask,
                    didCompleteWithError error: Error?) {
        let result: Result<String, Error>
        if let downloadError {
            result = .failure(downloadError)
        } else if let error {
            result = .failure(error)
        } else if let stagedName {
            result = .success(stagedName)
        } else {
            result = .failure(Failure.notAnArchive)
        }
        DispatchQueue.main.async {
            self.isDownloading = false
            self.task = nil
            self.session?.finishTasksAndInvalidate()
            self.session = nil
            let callback = self.completion
            self.completion = nil
            callback?(result)
        }
    }
}

