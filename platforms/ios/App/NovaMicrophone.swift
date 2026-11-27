import AVFoundation
import Foundation
import SwiftUI
import UIKit

/// Foreground-only microphone capture. No samples are saved or transmitted.
/// An optional native consumer can receive the input PCM on the audio callback
/// thread. It must not block, retain the borrowed buffer, or touch SwiftUI.
final class NovaMicrophone {
    private let engine = AVAudioEngine()
    private var installed = false

    func start(onPCM: ((AVAudioPCMBuffer) -> Void)? = nil,
               onLevel: @escaping (Double) -> Void) throws {
        guard NovaVoiceTests.microphoneAllowed else {
            throw NSError(domain: "NovaMicrophone", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "Microphone is disabled."])
        }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0,
              format.commonFormat == .pcmFormatFloat32 else {
            throw NSError(domain: "NovaMicrophone", code: 2,
                          userInfo: [NSLocalizedDescriptionKey: "No usable microphone input route."])
        }
        input.installTap(onBus: 0, bufferSize: 2048, format: nil) { buffer, _ in
            onPCM?(buffer)
            guard let channels = buffer.floatChannelData, buffer.frameLength > 0 else { return }
            let frames = Int(buffer.frameLength)
            let channelCount = Int(buffer.format.channelCount)
            let stride = buffer.stride
            var sum = 0.0
            for channel in 0..<channelCount {
                for frame in 0..<frames {
                    let sample = Double(channels[channel][frame * stride])
                    sum += sample * sample
                }
            }
            let rms = sqrt(sum / (Double(frames) * Double(channelCount)))
            let db = 20 * log10(max(rms, 0.000001))
            // Map -60 dBFS...0 dBFS into a normalized meter.
            onLevel(min(1, max(0, (db + 60) / 60)))
        }
        installed = true
        do {
            engine.prepare()
            try engine.start()
        } catch {
            stop()
            throw error
        }
    }

    func stop() {
        engine.stop()
        if installed {
            engine.inputNode.removeTap(onBus: 0)
            installed = false
        }
        engine.reset()
    }

    deinit { stop() }
}

/// Shared launcher preference and diagnostics. State changes are on the main
/// thread; generation tokens discard callbacks from cancelled permission/tests.
final class NovaVoiceTests: ObservableObject {
    static let shared = NovaVoiceTests()
    private static let preference = "nova.voicechat.microphoneEnabled"

    static var microphoneAllowed: Bool {
        UserDefaults.standard.bool(forKey: preference)
    }

    @Published private(set) var microphoneEnabled = NovaVoiceTests.microphoneAllowed
    @Published private(set) var isTesting = false
    @Published private(set) var isRequesting = false
    @Published private(set) var level = 0.0
    @Published private(set) var inputMessage = ""
    @Published private(set) var outputMessage = ""

    private var microphone: NovaMicrophone?
    private var player: AVAudioPlayer?
    private var generation = 0

    private init() {}

    func setMicrophoneEnabled(_ enabled: Bool) {
        UserDefaults.standard.set(enabled, forKey: Self.preference)
        microphoneEnabled = enabled
        if !enabled {
            stopInput(message: "Microphone disabled in Nova.")
        } else {
            inputMessage = "Ready to test. Nova requests permission only when you start."
        }
    }

    func toggleInput() {
        if isTesting || isRequesting {
            stopInput()
            return
        }
        guard Self.microphoneAllowed else { return }
        generation += 1
        let ticket = generation
        isRequesting = true
        inputMessage = "Checking microphone permission…"
        let session = AVAudioSession.sharedInstance()
        session.requestRecordPermission { [weak self] granted in
            DispatchQueue.main.async {
                guard let self = self, self.generation == ticket else { return }
                self.isRequesting = false
                guard Self.microphoneAllowed else { return }
                guard granted else {
                    self.inputMessage = "Microphone permission denied. Enable it in iOS Settings → Nova."
                    return
                }
                guard UIApplication.shared.applicationState != .background else {
                    self.inputMessage = "Return to Nova and start the test again."
                    return
                }
                self.startInput(ticket: ticket)
            }
        }
    }

    private func startInput(ticket: Int) {
        do {
            try NovaAudio.beginCapture()
            let capture = NovaMicrophone()
            try capture.start(onLevel: { [weak self] value in
                DispatchQueue.main.async {
                    guard let self = self, self.generation == ticket, self.isTesting else { return }
                    self.level = value
                }
            })
            microphone = capture
            isTesting = true
            inputMessage = "Speak to test your microphone."
        } catch {
            stopInput(message: "Microphone test failed: \(error.localizedDescription)")
            NovaAudio.endCapture()
        }
    }

    func stopInput(message: String = "Input test stopped.", restoreSession: Bool = true) {
        generation += 1
        let wasCapturing = microphone != nil || isTesting
        microphone?.stop()
        microphone = nil
        isTesting = false
        isRequesting = false
        level = 0
        inputMessage = message
        if restoreSession && wasCapturing { NovaAudio.endCapture() }
    }

    func testOutput() {
        player?.stop()
        NovaAudio.configure()
        do {
            let tone = try AVAudioPlayer(data: Self.beep())
            player = tone
            tone.prepareToPlay()
            if tone.play() {
                outputMessage = "Beep sent to the current audio output."
            } else {
                outputMessage = "Unable to play the test tone."
            }
        } catch {
            outputMessage = "Speaker test failed: \(error.localizedDescription)"
        }
    }

    func stopAll(restoreSession: Bool = true) {
        if isTesting || isRequesting || microphone != nil {
            stopInput(restoreSession: restoreSession)
        }
        player?.stop()
        player = nil
    }

    // A short, gently faded 880 Hz PCM WAV in memory; no asset or download.
    private static func beep() -> Data {
        let rate = 44_100
        let count = rate / 4
        var data = Data()
        func ascii(_ s: String) { data.append(contentsOf: s.utf8) }
        func u16(_ n: UInt16) {
            data.append(UInt8(truncatingIfNeeded: n))
            data.append(UInt8(truncatingIfNeeded: n >> 8))
        }
        func u32(_ n: UInt32) {
            u16(UInt16(truncatingIfNeeded: n))
            u16(UInt16(truncatingIfNeeded: n >> 16))
        }
        ascii("RIFF"); u32(UInt32(36 + count * 2)); ascii("WAVE")
        ascii("fmt "); u32(16); u16(1); u16(1)
        u32(UInt32(rate)); u32(UInt32(rate * 2)); u16(2); u16(16)
        ascii("data"); u32(UInt32(count * 2))
        for i in 0..<count {
            let fade = min(1.0, min(Double(i) / 441, Double(count - 1 - i) / 441))
            let sample = Int16(sin(2 * Double.pi * 880 * Double(i) / Double(rate)) * fade * 8_000)
            u16(UInt16(bitPattern: sample))
        }
        return data
    }
}

struct NovaVoiceChatSettingsSection: View {
    @ObservedObject private var tests = NovaVoiceTests.shared

    var body: some View {
        Section("Voice Chat") {
            Toggle("Enable microphone", isOn: Binding(
                get: { tests.microphoneEnabled },
                set: { tests.setMicrophoneEnabled($0) }
            ))
            Text("Off prevents Nova's voice capture. The microphone is never opened just by enabling this setting.")
                .font(.footnote)
                .foregroundStyle(.secondary)

            Button("Test voice output") { tests.testOutput() }
            if !tests.outputMessage.isEmpty {
                Text(tests.outputMessage).font(.footnote).foregroundStyle(.secondary)
            }

            Button(tests.isTesting ? "Stop voice input test" :
                    tests.isRequesting ? "Cancel voice input test" : "Test voice input") {
                tests.toggleInput()
            }
            .disabled(!tests.microphoneEnabled)

            ProgressView(value: tests.level, total: 1)
                .tint(.green)
                .accessibilityLabel("Microphone level")
                .accessibilityValue("\(Int(tests.level * 100)) percent")

            if !tests.inputMessage.isEmpty {
                Text(tests.inputMessage).font(.footnote).foregroundStyle(.secondary)
            }
            Text("Tests are local only. No voice is recorded to disk or sent to a server.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}
