import AVFoundation
import Speech

/// The microphone and the voice, on one audio engine. Answers play through
/// the same engine as the microphone with Apple's voice processing on, so
/// its echo cancellation keeps the phone from hearing itself and the user
/// can talk over an answer. Recognition gives one growing transcript per run.
@MainActor
final class SpeechIO: NSObject, AVSpeechSynthesizerDelegate {
    /// The current run's whole transcript so far, and whether it is final.
    var onTranscript: ((String, Bool) -> Void)?
    /// A recognition run ended; carries what to tell the user when it failed.
    var onListenEnded: ((String?) -> Void)?
    var onSpoken: (() -> Void)?
    /// A phone call or Siri took the audio.
    var onInterrupted: (() -> Void)?
    var onLevel: ((Float) -> Void)?

    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let writer = AVSpeechSynthesizer()
    /// Plain playback when the engine cannot play (or a voice writes no audio).
    private let fallback = AVSpeechSynthesizer()
    private let tap = TapBox()
    private var engineOn = false
    private var task: SFSpeechRecognitionTask?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var listenGen = 0
    private var speechGen = 0
    private var scheduled = 0
    private var played = 0
    private var written = false
    private var speaking = false
    private var observers: [NSObjectProtocol] = []
    let playFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 24_000, channels: 1, interleaved: false)!

    var isListening: Bool { task != nil }
    var isSpeaking: Bool { speaking }

    override init() {
        super.init()
        fallback.delegate = self
        engine.attach(player)
        tap.onLevel = { [weak self] level in
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.onLevel?(level) } }
        }
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
            guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt, raw == AVAudioSession.InterruptionType.began.rawValue else { return }
            MainActor.assumeIsolated { self?.onInterrupted?() }
        })
        // Headphones in or out change the input format: rebuild the engine.
        observers.append(center.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reconfigure() }
        })
    }

    /// Asks for Speech Recognition and the microphone; says why when refused.
    static func authorize() async -> String? {
        let speech = await withCheckedContinuation { (c: CheckedContinuation<SFSpeechRecognizerAuthorizationStatus, Never>) in
            SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) }
        }
        guard speech == .authorized else {
            return "Speech Recognition is off for Switchyard. Turn it on in Settings › Privacy & Security › Speech Recognition."
        }
        let mic = await withCheckedContinuation { (c: CheckedContinuation<Bool, Never>) in
            AVAudioApplication.requestRecordPermission { c.resume(returning: $0) }
        }
        guard mic else { return "The microphone is off for Switchyard. Turn it on in Settings › Privacy & Security › Microphone." }
        return nil
    }

    // MARK: Engine

    private func startEngine() throws {
        guard !engineOn else { return }
        let session = AVAudioSession.sharedInstance()
        try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
        try session.setActive(true)
        // Echo cancellation; the simulator and some routes refuse it, and then
        // the echo check on the transcript is what is left.
        if !engine.inputNode.isVoiceProcessingEnabled { try? engine.inputNode.setVoiceProcessingEnabled(true) }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw VoiceProblem("No microphone was found.") }
        input.removeTap(onBus: 0)
        let box = tap
        input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in box.feed(buffer) }
        engine.connect(player, to: engine.mainMixerNode, format: playFormat)
        engine.prepare()
        try engine.start()
        engineOn = true
    }

    private func stopEngine() {
        guard engineOn else { return }
        engine.inputNode.removeTap(onBus: 0)
        player.stop()
        engine.stop()
        engineOn = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private func reconfigure() {
        guard engineOn else { return }
        engineOn = false
        engine.inputNode.removeTap(onBus: 0)
        let wasListening = task != nil
        if speaking { hush(); onSpoken?() }
        do { try startEngine() } catch { endRun(gen: listenGen, problem: "The microphone stopped.") ; return }
        // The run's audio format changed under it: end it, and the controller starts another.
        if wasListening { endRun(gen: listenGen, problem: nil) }
    }

    /// Stops the engine when nothing needs it, so the microphone light goes out.
    func idle() {
        if task == nil, !speaking { stopEngine() }
    }

    // MARK: Listening

    func startListening(locale: Locale = .current) throws {
        guard task == nil else { return }
        guard let recognizer = SFSpeechRecognizer(locale: locale) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")) else {
            throw VoiceProblem("Speech recognition doesn't support your language.")
        }
        guard recognizer.isAvailable else { throw VoiceProblem("Speech recognition isn't available right now. Check your connection.") }
        try startEngine()
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        request.taskHint = .dictation
        // On the phone: private, and no one-minute limit per run.
        if recognizer.supportsOnDeviceRecognition { request.requiresOnDeviceRecognition = true }
        listenGen += 1
        let gen = listenGen
        self.request = request
        tap.request = request
        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            let text = result?.bestTranscription.formattedString
            let final = result?.isFinal ?? false
            let code = (error as NSError?).map { ($0.domain, $0.code, $0.localizedDescription) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.recognized(gen: gen, text: text, final: final, error: code) }
            }
        }
    }

    private func recognized(gen: Int, text: String?, final: Bool, error: (String, Int, String)?) {
        guard gen == listenGen else { return }
        if let text, !text.isEmpty { onTranscript?(text, final) }
        if final { endRun(gen: gen, problem: nil); return }
        if let error { endRun(gen: gen, problem: Self.problem(error)) }
    }

    private func endRun(gen: Int, problem: String?) {
        guard gen == listenGen, task != nil else { return }
        listenGen += 1
        tap.request = nil
        request?.endAudio()
        task?.cancel()
        task = nil
        request = nil
        onListenEnded?(problem)
    }

    /// `abort` drops words not yet final (voice mode, after sending); a plain
    /// stop lets the last words arrive (dictation).
    func stopListening(abort: Bool) {
        guard task != nil else { return }
        if abort {
            listenGen += 1
            tap.request = nil
            task?.cancel()
            task = nil
            request = nil
            return
        }
        tap.request = nil
        request?.endAudio()
        let gen = listenGen
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(1.5))
            guard let self, self.listenGen == gen else { return }
            self.endRun(gen: gen, problem: nil)
        }
    }

    /// Nothing is said for a pause, a cancel, or our own stop: those are not failures.
    nonisolated static func problem(_ error: (String, Int, String)) -> String? {
        let (domain, code, text) = error
        if domain == "kAFAssistantErrorDomain" {
            switch code {
            case 203, 216, 301, 1101, 1107, 1110: return nil
            case 1700: return "Speech Recognition is off for Switchyard. Turn it on in Settings."
            default: break
            }
        }
        if domain == NSURLErrorDomain { return "Speech recognition needs a network connection." }
        if domain == "kLSRErrorDomain", code == 301 { return nil }
        return "Voice input stopped: \(text)"
    }

    // MARK: Speaking

    func speak(_ text: String, rate: Float) {
        hush()
        speechGen += 1
        let gen = speechGen
        speaking = true
        let utterance = Self.utterance(text, rate: rate)
        do { try startEngine() } catch {
            fallback.speak(utterance)
            return
        }
        scheduled = 0
        played = 0
        written = false
        let convert = SpeechConverter(to: playFormat)
        writer.write(utterance) { [weak self] buffer in
            // Converted here: the synthesizer may reuse its buffer once this returns.
            guard let pcm = buffer as? AVAudioPCMBuffer else { return }
            let out = pcm.frameLength == 0 ? nil : convert.run(pcm)
            let done = pcm.frameLength == 0
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.wrote(gen: gen, out, done: done) }
            }
        }
        // Some voices write nothing; read it the plain way rather than stay silent.
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(4))
            guard let self, self.speechGen == gen, self.scheduled == 0, !self.written else { return }
            self.writer.stopSpeaking(at: .immediate)
            self.fallback.speak(Self.utterance(text, rate: rate))
        }
    }

    private static func utterance(_ text: String, rate: Float) -> AVSpeechUtterance {
        let u = AVSpeechUtterance(string: text)
        u.voice = AVSpeechSynthesisVoice(language: AVSpeechSynthesisVoice.currentLanguageCode())
        u.rate = min(AVSpeechUtteranceMaximumSpeechRate, max(AVSpeechUtteranceMinimumSpeechRate, AVSpeechUtteranceDefaultSpeechRate * rate))
        return u
    }

    private func wrote(gen: Int, _ buffer: AVAudioPCMBuffer?, done: Bool) {
        guard gen == speechGen, speaking else { return }
        if done { written = true; finishIfPlayed(gen); return }
        guard let buffer, engineOn else { return }
        scheduled += 1
        player.scheduleBuffer(buffer, completionCallbackType: .dataPlayedBack) { [weak self] _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let self, self.speechGen == gen else { return }
                    self.played += 1
                    self.finishIfPlayed(gen)
                }
            }
        }
        if !player.isPlaying { player.play() }
    }

    private func finishIfPlayed(_ gen: Int) {
        guard gen == speechGen, speaking, written, played >= scheduled else { return }
        speaking = false
        onSpoken?()
    }

    func hush() {
        speechGen += 1
        speaking = false
        writer.stopSpeaking(at: .immediate)
        fallback.stopSpeaking(at: .immediate)
        if engineOn { player.stop() }
    }

    func shutdown() {
        stopListening(abort: true)
        hush()
        stopEngine()
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                guard self.speaking else { return }
                self.speaking = false
                self.onSpoken?()
            }
        }
    }
}

struct VoiceProblem: Error {
    let message: String
    init(_ message: String) { self.message = message }
}

/// What the realtime audio thread reads: the run to feed, and a level meter.
private final class TapBox: @unchecked Sendable {
    private let lock = NSLock()
    private var _request: SFSpeechAudioBufferRecognitionRequest?
    private var count = 0
    var onLevel: ((Float) -> Void)?

    var request: SFSpeechAudioBufferRecognitionRequest? {
        get { lock.lock(); defer { lock.unlock() }; return _request }
        set { lock.lock(); _request = newValue; lock.unlock() }
    }

    func feed(_ buffer: AVAudioPCMBuffer) {
        request?.append(buffer)
        count += 1
        guard count % 3 == 0, let data = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return }
        let n = Int(buffer.frameLength)
        var sum: Float = 0
        for i in stride(from: 0, to: n, by: 2) { sum += data[i] * data[i] }
        let rms = sqrt(sum / Float(max(1, n / 2)))
        let db = 20 * log10(max(rms, 0.000_01))
        onLevel?(min(1, max(0, (db + 55) / 45)))
    }
}

/// Converts the synthesizer's buffers to the player's format.
final class SpeechConverter: @unchecked Sendable {
    private let target: AVAudioFormat
    private var converter: AVAudioConverter?
    private var source: AVAudioFormat?

    init(to target: AVAudioFormat) { self.target = target }

    func run(_ input: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        if source != input.format {
            source = input.format
            converter = AVAudioConverter(from: input.format, to: target)
        }
        guard let converter else { return nil }
        let ratio = target.sampleRate / input.format.sampleRate
        let capacity = AVAudioFrameCount(Double(input.frameLength) * ratio + 1024)
        guard let output = AVAudioPCMBuffer(pcmFormat: target, frameCapacity: capacity) else { return nil }
        var given = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if given { status.pointee = .noDataNow; return nil }
            given = true
            status.pointee = .haveData
            return input
        }
        return error == nil && output.frameLength > 0 ? output : nil
    }
}
