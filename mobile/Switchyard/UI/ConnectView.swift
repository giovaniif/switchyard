import SwiftUI

/// Where the app starts: the host's HTTPS address, recent hosts, or the demo.
struct ConnectView: View {
    var sheet = false
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @FocusState private var focused: Bool
    @Namespace private var glass

    var body: some View {
        ZStack {
            AuroraBackground()
            ScrollView {
                VStack(spacing: 28) {
                    header
                    form
                    if !app.recent.isEmpty { recent }
                    footer
                }
                .padding(.horizontal, 22)
                .padding(.vertical, 40)
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .overlay(alignment: .topTrailing) {
            if sheet {
                Button {
                    dismiss()
                } label: {
                    Image(systemName: "xmark").font(.body.weight(.semibold)).frame(width: 30, height: 30)
                }
                .buttonStyle(.glass)
                .buttonBorderShape(.circle)
                .padding()
                .accessibilityLabel("Close")
            }
        }
        .onAppear {
            if let linked = app.linkedAddress { address = linked; app.linkedAddress = nil }
        }
    }

    private var header: some View {
        VStack(spacing: 14) {
            Text("Y")
                .font(.system(size: 54, weight: .semibold, design: .rounded))
                .foregroundStyle(.primary)
                .frame(width: 96, height: 96)
                .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 28))
            Text("Switchyard")
                .font(.largeTitle.weight(.bold))
            Text("Your agents keep working on your host. Check in, answer, and talk to them from here.")
                .font(.callout)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
        }
        .padding(.top, 20)
    }

    private var form: some View {
        GlassEffectContainer(spacing: 14) {
            VStack(spacing: 14) {
                HStack(spacing: 10) {
                    Image(systemName: "lock.fill").foregroundStyle(.secondary)
                    TextField("https://your-host.ts.net", text: $address)
                        .keyboardType(.URL)
                        .textContentType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .submitLabel(.go)
                        .focused($focused)
                        .onSubmit(connect)
                        .accessibilityLabel("Host address")
                }
                .padding(.horizontal, 18)
                .frame(height: 54)
                .glassEffect(.regular.interactive(), in: .capsule)
                .glassEffectID("field", in: glass)

                Button(action: connect) {
                    HStack {
                        if app.checking { ProgressView().tint(.white) }
                        Text(app.checking ? "Connecting…" : "Connect")
                            .font(.headline)
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: 30)
                }
                .buttonStyle(.glassProminent)
                .controlSize(.large)
                .disabled(app.checking || address.trimmingCharacters(in: .whitespaces).isEmpty)
                .glassEffectID("connect", in: glass)
            }
        }
        .overlay(alignment: .bottom) {
            if let error = app.connectError {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(Color.yardBad)
                    .multilineTextAlignment(.center)
                    .padding(12)
                    .glassEffect(.regular.tint(Color.yardBad.opacity(0.1)), in: .rect(cornerRadius: 16))
                    .offset(y: 86)
            }
        }
        .padding(.bottom, app.connectError == nil ? 0 : 80)
    }

    private var recent: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Recent hosts")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.leading, 6)
            GlassEffectContainer(spacing: 8) {
                VStack(spacing: 8) {
                    ForEach(app.recent, id: \.self) { value in
                        Button {
                            address = value
                            connect()
                        } label: {
                            HStack {
                                Image(systemName: "server.rack")
                                Text(URL(string: value)?.host ?? value).lineLimit(1)
                                Spacer()
                                Image(systemName: "chevron.right").font(.footnote).foregroundStyle(.tertiary)
                            }
                            .padding(.horizontal, 18)
                            .frame(height: 50)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .glassEffect(.regular.interactive(), in: .capsule)
                        .contextMenu {
                            Button("Forget", systemImage: "trash", role: .destructive) { app.forget(value) }
                        }
                    }
                }
            }
        }
    }

    private var footer: some View {
        VStack(spacing: 16) {
            Button {
                app.startDemo()
                dismiss()
            } label: {
                Label("Look around with demo sessions", systemImage: "sparkles")
                    .font(.subheadline.weight(.medium))
            }
            .buttonStyle(.glass)
            Text("Your phone needs the same private network as the host, such as Tailscale. Sessions keep running while the app is closed; nothing runs on the phone.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
    }

    private func connect() {
        focused = false
        Task {
            await app.connect(address)
            if app.connectError == nil, sheet { dismiss() }
        }
    }
}
