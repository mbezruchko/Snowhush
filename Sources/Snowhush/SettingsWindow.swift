import AppKit
import SwiftUI

/// What the Settings window shows, as groups of rows that read and change the app through closures: the window keeps
/// no state of its own, so it never disagrees with the menu.
final class SettingsModel: ObservableObject {
    struct Item: Identifiable {
        enum Kind {
            case toggle(isOn: () -> Bool, flip: () -> Void)
            case slider(value: () -> Double, set: (Double) -> Void, range: ClosedRange<Double>)
            case button(() -> Void)
        }

        let id = UUID()
        var title: () -> String
        var kind: Kind
        var isEnabled: () -> Bool = { true }
        var isHidden: () -> Bool = { false }
    }

    struct Group: Identifiable {
        let id = UUID()
        var title: String?
        var items: [Item]
        var footer: String?
    }

    var groups: [Group] = []

    /// Has the window read everything again.
    func touch() { objectWillChange.send() }
}

private struct SettingsView: View {
    @ObservedObject var model: SettingsModel

    var body: some View {
        Form {
            ForEach(model.groups) { group in
                Section {
                    ForEach(group.items.filter { !$0.isHidden() }) { item in
                        row(item).disabled(!item.isEnabled())
                    }
                } header: {
                    if let title = group.title { Text(title) }
                } footer: {
                    if let footer = group.footer {
                        Text(footer).font(.footnote).foregroundStyle(.tertiary).frame(maxWidth: .infinity, alignment: .center)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 400, height: 560)
    }

    @ViewBuilder
    private func row(_ item: SettingsModel.Item) -> some View {
        switch item.kind {
        case let .toggle(isOn, flip):
            Toggle(item.title(), isOn: Binding(get: isOn, set: { _ in
                flip()
                model.touch()
            }))
        case let .slider(value, set, range):
            LabeledContent(item.title()) {
                HStack(spacing: 10) {
                    Slider(value: Binding(get: value, set: { next in
                        set(next)
                        model.touch()
                    }), in: range)
                    .frame(width: 170)
                    Text(MenuSlider.percent(value()))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .frame(width: 40, alignment: .trailing)
                }
            }
        case let .button(action):
            Button(item.title(), action: action)
        }
    }
}

/// The Settings window: what is set once and left, out of the menu's way.
final class SettingsController {
    let model = SettingsModel()
    private var window: NSWindow?

    func show() {
        model.touch()
        if window == nil {
            let host = NSHostingController(rootView: SettingsView(model: model))
            let window = NSWindow(contentViewController: host)
            window.styleMask = [.titled, .closable]
            window.title = "Snowhush"
            window.isReleasedWhenClosed = false
            window.center()
            self.window = window
        }
        NSApp.activate()
        window?.makeKeyAndOrderFront(nil)
    }
}
