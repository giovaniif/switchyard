import AVFoundation
import XCTest
@testable import Switchyard

/// The path answers take to the speaker: the synthesizer writes buffers in
/// its voice's format, converted to the player's before they are scheduled.
final class SpeechTests: XCTestCase {
    func testWrittenSpeechConvertsToThePlayersFormat() throws {
        let target = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 24_000, channels: 1, interleaved: false)!
        let convert = SpeechConverter(to: target)
        let synth = AVSpeechSynthesizer()
        let utterance = AVSpeechUtterance(string: "Voice mode on.")
        utterance.voice = AVSpeechSynthesisVoice(language: "en-US")
        let done = expectation(description: "the last, empty buffer")
        var frames: AVAudioFrameCount = 0
        var formats = Set<String>()
        synth.write(utterance) { buffer in
            guard let pcm = buffer as? AVAudioPCMBuffer else { return }
            if pcm.frameLength == 0 { done.fulfill(); return }
            formats.insert("\(pcm.format.sampleRate)")
            if let out = convert.run(pcm) {
                XCTAssertEqual(out.format, target)
                frames += out.frameLength
            }
        }
        if XCTWaiter().wait(for: [done], timeout: 20) != .completed {
            throw XCTSkip("This simulator has no voice that writes audio.")
        }
        XCTAssertGreaterThan(frames, 4_800, "about a second of speech at 24 kHz; voice rate(s): \(formats)")
    }
}
