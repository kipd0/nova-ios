import Foundation
import AVFoundation
import UIKit

/// Owns the iOS session shared by game playback and optional voice capture.
/// All methods are called on the main thread.
enum NovaAudio {
    private static var observers: [NSObjectProtocol] = []
    private static var capturing = false

    static func configure() {
        installObservers()
        do {
            try applySession(capture: capturing)
            let session = AVAudioSession.sharedInstance()
            NovaDiagnostics.record(
                "audio.session.active; sampleRate=\(Int(session.sampleRate)); outputs=\(session.currentRoute.outputs.count)"
            )
        } catch {
            NovaDiagnostics.record("audio.session.failed: \(error.localizedDescription)")
        }
    }

    static func beginCapture() throws {
        installObservers()
        guard NovaVoiceTests.microphoneAllowed else {
            throw NSError(domain: "NovaAudio", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "Microphone is disabled in Nova."])
        }
        do {
            try applySession(capture: true)
            capturing = true
        } catch {
            capturing = false
            configure()
            throw error
        }
    }

    static func endCapture() {
        guard capturing else { return }
        capturing = false
        configure()
    }

    private static func applySession(capture: Bool) throws {
        let session = AVAudioSession.sharedInstance()
        if capture {
            try session.setCategory(.playAndRecord, mode: .default,
                                    options: [.defaultToSpeaker, .allowBluetooth])
            // The hardware route may negotiate a different rate; use its actual format.
            try? session.setPreferredSampleRate(48_000)
        } else {
            try session.setCategory(.playback, mode: .default)
        }
        try session.setActive(true)
    }

    private static func installObservers() {
        guard observers.isEmpty else { return }
        let center = NotificationCenter.default
        let session = AVAudioSession.sharedInstance()
        observers.append(center.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: session, queue: .main
        ) { notification in
            guard let value = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                  let type = AVAudioSession.InterruptionType(rawValue: value) else { return }
            if type == .began {
                NovaVoiceTests.shared.stopAll(restoreSession: false)
                capturing = false
            } else if type == .ended {
                configure()
            }
        })
        observers.append(center.addObserver(
            forName: UIApplication.didBecomeActiveNotification,
            object: nil, queue: .main
        ) { _ in configure() })
        observers.append(center.addObserver(
            forName: UIApplication.didEnterBackgroundNotification,
            object: nil, queue: .main
        ) { _ in
            NovaVoiceTests.shared.stopAll(restoreSession: false)
            capturing = false
        })
        observers.append(center.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: session, queue: .main
        ) { notification in
            guard let raw = notification.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
                  let reason = AVAudioSession.RouteChangeReason(rawValue: raw),
                  reason == .oldDeviceUnavailable || reason == .newDeviceAvailable else { return }
            if capturing {
                NovaVoiceTests.shared.stopInput(message: "Audio route changed. Start the input test again.")
            }
        })
    }
}
