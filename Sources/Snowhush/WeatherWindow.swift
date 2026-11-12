import AppKit

@MainActor
final class WeatherWindow: NSWindowController, NSSearchFieldDelegate {
    private let sync: WeatherSync
    private let enabled = NSButton(checkboxWithTitle: tr("weather.effects"), target: nil, action: nil)
    private let preset = NSPopUpButton()
    private var effectPickers: [WeatherEffect: NSPopUpButton] = [:]
    private let locationSection = NSStackView()
    private let manualNote = NSTextField(wrappingLabelWithString: "")
    private let automatic = NSButton(radioButtonWithTitle: tr("weather.automatic"), target: nil, action: nil)
    private let city = NSButton(radioButtonWithTitle: tr("weather.city"), target: nil, action: nil)
    private let search = NSSearchField()
    private let searchButton = NSButton(title: tr("weather.search"), target: nil, action: nil)
    private let results = NSPopUpButton()
    private let useButton = NSButton(title: tr("weather.use"), target: nil, action: nil)
    private let summary = NSTextField(wrappingLabelWithString: "")
    private let detail = NSTextField(wrappingLabelWithString: "")
    private let searchStatus = NSTextField(wrappingLabelWithString: "")
    private let refreshButton = NSButton(title: tr("weather.refresh"), target: nil, action: nil)
    private var places: [WeatherPlace] = []
    private var searching: Task<Void, Never>?
    private var searchGeneration = UUID()
    private var choosingCity = false

    init(sync: WeatherSync) {
        self.sync = sync
        let height = min(720, max(400, (NSScreen.main?.visibleFrame.height ?? 820) - 100))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 560, height: height),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = tr("weather.title")
        window.isReleasedWhenClosed = false
        window.contentMinSize = NSSize(width: 520, height: 400)
        super.init(window: window)
        window.center()
        choosingCity = !Preferences.weatherAutomatic
        build()
        update()
    }

    required init?(coder: NSCoder) { nil }

    func show() {
        update()
        NSApp.activate()
        showWindow(nil)
        window?.makeKeyAndOrderFront(nil)
    }

    private func build() {
        guard let content = window?.contentView else { return }
        let heading = NSTextField(labelWithString: tr("weather.title"))
        heading.font = .systemFont(ofSize: 22, weight: .semibold)
        let intro = NSTextField(wrappingLabelWithString: tr("weather.customize.intro"))
        intro.textColor = .secondaryLabelColor
        enabled.target = self
        enabled.action = #selector(toggleEnabled)
        let effectsHeading = NSTextField(labelWithString: tr("weather.customize.heading"))
        effectsHeading.font = .systemFont(ofSize: 14, weight: .semibold)
        preset.addItems(withTitles: ["weather.preset.custom", "weather.preset.weather", "weather.preset.snow", "weather.preset.fog", "weather.preset.none"].map(tr))
        preset.setAccessibilityLabel(tr("weather.preset"))
        preset.target = self
        preset.action = #selector(choosePreset)
        let rows: [[NSView]] = WeatherEffect.allCases.enumerated().map { index, effect in
            let label = NSTextField(labelWithString: tr(effect.key))
            let picker = NSPopUpButton()
            picker.addItems(withTitles: WeatherEffectRule.allCases.map { tr($0.key) })
            picker.setAccessibilityLabel(tr(effect.key))
            picker.tag = index
            picker.target = self
            picker.action = #selector(chooseRule(_:))
            effectPickers[effect] = picker
            return [label, picker]
        }
        let effectsGrid = NSGridView(views: rows)
        effectsGrid.rowSpacing = 8
        effectsGrid.columnSpacing = 24
        effectsGrid.column(at: 0).xPlacement = .leading
        effectsGrid.column(at: 1).xPlacement = .trailing
        let help = NSTextField(wrappingLabelWithString: tr("weather.customize.help"))
        help.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        help.textColor = .secondaryLabelColor
        manualNote.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        manualNote.textColor = .secondaryLabelColor
        automatic.target = self
        automatic.action = #selector(chooseAutomatic)
        city.target = self
        city.action = #selector(chooseCity)
        search.placeholderString = tr("weather.search.placeholder")
        search.setAccessibilityLabel(tr("weather.search.placeholder"))
        search.delegate = self
        search.target = self
        search.action = #selector(findCity)
        searchButton.target = self
        searchButton.action = #selector(findCity)
        useButton.target = self
        useButton.action = #selector(useCity)
        useButton.bezelStyle = .rounded
        results.setAccessibilityLabel(tr("weather.results"))
        searchStatus.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        searchStatus.textColor = .secondaryLabelColor
        summary.font = .systemFont(ofSize: 15, weight: .medium)
        detail.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        detail.textColor = .secondaryLabelColor
        refreshButton.target = self
        refreshButton.action = #selector(refresh)
        let provider = NSButton(title: tr("weather.provider"), target: self, action: #selector(openProvider))
        provider.bezelStyle = .inline
        provider.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        provider.contentTintColor = .linkColor
        let privacy = NSTextField(wrappingLabelWithString: tr("weather.privacy"))
        privacy.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        privacy.textColor = .secondaryLabelColor

        let query = NSStackView(views: [search, searchButton])
        query.spacing = 8
        let selection = NSStackView(views: [results, useButton])
        selection.spacing = 8
        let status = NSStackView(views: [summary, detail, refreshButton])
        status.orientation = .vertical
        status.alignment = .leading
        status.spacing = 6
        let locationHeading = NSTextField(labelWithString: tr("weather.location.heading"))
        locationHeading.font = .systemFont(ofSize: 14, weight: .semibold)
        locationSection.orientation = .vertical
        locationSection.alignment = .leading
        locationSection.spacing = 12
        for view in [locationHeading, automatic, city, query, selection, searchStatus, status, privacy, provider] {
            locationSection.addArrangedSubview(view)
        }
        let stack = NSStackView(views: [heading, intro, enabled, effectsHeading, preset, effectsGrid, help, manualNote, locationSection])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(scroll)
        let document = WeatherSettingsContent()
        document.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = document
        document.addSubview(stack)
        var constraints = [
            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: content.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            document.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),
            stack.leadingAnchor.constraint(equalTo: document.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: document.trailingAnchor, constant: -24),
            stack.topAnchor.constraint(equalTo: document.topAnchor, constant: 24),
            stack.bottomAnchor.constraint(equalTo: document.bottomAnchor, constant: -24),
            search.widthAnchor.constraint(greaterThanOrEqualToConstant: 260),
        ]
        for view in [intro, preset, effectsGrid, help, manualNote, locationSection, query, selection, searchStatus, status, privacy] {
            constraints.append(view.widthAnchor.constraint(equalTo: stack.widthAnchor))
        }
        for view in [summary, detail] { constraints.append(view.widthAnchor.constraint(equalTo: stack.widthAnchor)) }
        results.setContentHuggingPriority(.defaultLow, for: .horizontal)
        search.setContentHuggingPriority(.defaultLow, for: .horizontal)
        NSLayoutConstraint.activate(constraints)
        content.layoutSubtreeIfNeeded()
    }

    func update() {
        enabled.state = sync.enabled ? .on : .off
        let effects = Preferences.weatherEffects
        let presets: [WeatherEffects] = [.allWeather, .snowOnly, .fogOnly, .none]
        preset.selectItem(at: presets.firstIndex(of: effects).map { $0 + 1 } ?? 0)
        for (effect, picker) in effectPickers {
            picker.selectItem(at: WeatherEffectRule.allCases.firstIndex(of: effects[effect]) ?? 1)
        }
        locationSection.isHidden = !effects.needsWeather
        manualNote.isHidden = effects.needsWeather
        manualNote.stringValue = tr(effects == .none ? "weather.none.description" : "weather.manual.description")
        automatic.state = choosingCity ? .off : .on
        city.state = choosingCity ? .on : .off
        search.isEnabled = choosingCity
        searchButton.isEnabled = choosingCity && searching == nil
        results.isEnabled = choosingCity && !places.isEmpty
        useButton.isEnabled = choosingCity && !places.isEmpty
        refreshButton.isEnabled = sync.enabled && sync.needsWeather && sync.status != .locating && sync.status != .updating
        if let snapshot = sync.snapshot, let condition = snapshot.reading.condition {
            summary.stringValue = "\(snapshot.place.name) · \(Int(snapshot.reading.temperature.rounded()))° · \(tr(condition.key))"
            let time = snapshot.reading.date.formatted(date: .omitted, time: .shortened)
            detail.stringValue = tr(sync.status == .ready ? "weather.updated" : sync.status.key).replacingOccurrences(of: "{time}", with: time)
        } else {
            summary.stringValue = tr(sync.status.key)
            detail.stringValue = tr("weather.interval")
        }
    }

    @objc private func toggleEnabled() {
        if enabled.state == .on { NSApp.activate() }
        sync.setEnabled(enabled.state == .on)
        update()
    }

    @objc private func chooseRule(_ sender: NSPopUpButton) {
        guard WeatherEffect.allCases.indices.contains(sender.tag),
              WeatherEffectRule.allCases.indices.contains(sender.indexOfSelectedItem) else { return }
        var effects = Preferences.weatherEffects
        effects[WeatherEffect.allCases[sender.tag]] = WeatherEffectRule.allCases[sender.indexOfSelectedItem]
        sync.setEffects(effects)
        update()
    }

    @objc private func choosePreset() {
        let presets: [WeatherEffects] = [.allWeather, .snowOnly, .fogOnly, .none]
        guard presets.indices.contains(preset.indexOfSelectedItem - 1) else { return }
        sync.setEffects(presets[preset.indexOfSelectedItem - 1])
        update()
    }

    @objc private func chooseAutomatic() {
        choosingCity = false
        cancelSearch()
        sync.useAutomaticLocation()
        update()
    }

    @objc private func chooseCity() {
        choosingCity = true
        update()
        window?.makeFirstResponder(search)
    }

    func controlTextDidChange(_ obj: Notification) {
        cancelSearch()
        places = []
        results.removeAllItems()
        searchStatus.stringValue = ""
        update()
    }

    private func cancelSearch() {
        searchGeneration = UUID()
        searching?.cancel()
        searching = nil
    }

    @objc private func findCity() {
        guard choosingCity else { return }
        let query = search.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard query.count >= 2 else { return }
        cancelSearch()
        let ticket = searchGeneration
        places = []
        results.removeAllItems()
        searchStatus.stringValue = tr("weather.searching")
        searching = Task { [weak self] in
            do {
                let found = try await WeatherAPI().search(query)
                guard let self, !Task.isCancelled, ticket == searchGeneration else { return }
                places = found
                results.addItems(withTitles: found.map(\.title))
                searchStatus.stringValue = found.isEmpty ? tr("weather.noResults") : tr("weather.pickResult")
                searching = nil
                update()
            } catch {
                guard let self, !Task.isCancelled, ticket == searchGeneration else { return }
                searching = nil
                searchStatus.stringValue = tr("weather.status.failed")
                update()
            }
        }
        update()
    }

    @objc private func useCity() {
        guard places.indices.contains(results.indexOfSelectedItem) else { return }
        sync.use(places[results.indexOfSelectedItem])
        update()
    }

    @objc private func refresh() { sync.refresh() }
    @objc private func openProvider() { NSWorkspace.shared.open(URL(string: "https://open-meteo.com/")!) }
}

private final class WeatherSettingsContent: NSView {
    override var isFlipped: Bool { true }
}
