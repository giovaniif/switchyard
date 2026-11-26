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
        let done = expectation(description: "the synthesizer went quiet")
        var frames: AVAudioFrameCount = 0
        var formats = Set<String>()
        var sequence: [AVAudioFrameCount] = []
        var finished = false
        let lock = NSLock()
        synth.write(utterance) { buffer in
            guard let pcm = buffer as? AVAudioPCMBuffer else { return }
            lock.lock(); defer { lock.unlock() }
            sequence.append(pcm.frameLength)
            guard pcm.frameLength > 0 else {
                if !finished, frames > 0 {
                    finished = true
                    // Wait a little: SpeechIO treats an empty buffer as the end only if nothing follows.
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1) { done.fulfill() }
                }
                return
            }
            formats.insert("\(pcm.format.sampleRate)")
            if let out = convert.run(pcm) {
                XCTAssertEqual(out.format, target)
                frames += out.frameLength
            }
        }
        if XCTWaiter().wait(for: [done], timeout: 20) != .completed {
            throw XCTSkip("This simulator has no voice that writes audio.")
        }
        lock.lock(); defer { lock.unlock() }
        print("speech buffers:", sequence)
        XCTAssertGreaterThan(frames, 4_800, "about a second of speech at 24 kHz; voice rate(s): \(formats)")
    }
}
