import AppKit

/// Direct manipulation of the settled snow. No editor window and no global event tap.
final class SnowmanCanvas: NSView {
    private struct Figure: Codable {
        var x: Double
        var y: Double
        var radii: [Double]
        var scarf: Int
        var dust: Double?
        var motion = SnowmanMotion()
        enum CodingKeys: String, CodingKey { case x, y, radii, scarf, dust }
    }
    private var figures: [Figure] = []
    private var selected: Int?
    private var clickedBall = 0
    private var moved = false
    private var optionGesture = false
    private var separateGesture = false
    private var pendingSeparation: Int?
    private var separationSource: Int?
    private var throwGesture = SnowmanThrow()
    private var dragOrigin = CGPoint.zero
    private var animationTimer: Timer?
    private var timerInterval: TimeInterval = 0
    var powerSaving = false { didSet { if powerSaving != oldValue { refreshAnimation() } } }
    private var lastFrame: TimeInterval = 0
    private var sceneTime: CGFloat = 0
    private var lastLook = CGPoint.zero
    private var lastBlink = false
    private var dirtyPersistence = false
    private var savedAt: CGFloat = 0
    private var learnedGesture = false
    private var reducedMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    private var lastFeedback: CGFloat = -10
    private var lastGestureAt: CGFloat = -10
    var feedback: (CGFloat) -> Void = { _ in }
    var pressSnow: (CGPoint, CGFloat, CGFloat, CGFloat, SnowmanSupport) -> Void = { _, _, _, _, _ in }
    private struct Powder {
        var point: CGPoint
        var velocity: CGPoint
        var radius: CGFloat
        var life: CGFloat
    }
    private var powder: [Powder] = []
    var support: (CGPoint, SnowmanSupport?) -> SnowmanSupport? = { _, _ in nil }
    var active = true {
        didSet {
            guard active != oldValue else { return }
            if !active { preserveScene() }
            refreshAnimation()
        }
    }

    private var previous = NSPoint.zero
    private var start = NSPoint.zero
    private var beforeDrag: [Figure] = []
    private var collected: CGFloat = 0
    private var initialArea: CGFloat = 0
    private var scooping = false
    private var snapTarget: Int?
    private(set) var dragging = false
    private let storageKey: String
    private let defaults: UserDefaults
    var takeSnow: (NSPoint, CGFloat, CGFloat) -> CGFloat = { _, _, _ in 0 }
    var returnSnow: (NSPoint, CGFloat) -> Void = { _, _ in }
    var finished: () -> Void = {}
    var hover: NSPoint? { didSet { if hover != oldValue { needsDisplay = true } } }
    var editing = false {
        didSet {
            if !editing { hover = nil; selected = nil }
            needsDisplay = true
        }
    }
    private let colors: [NSColor] = [
        NSColor(srgbRed: 0.26, green: 0.43, blue: 0.48, alpha: 1),
        NSColor(srgbRed: 0.63, green: 0.32, blue: 0.26, alpha: 1),
        NSColor(srgbRed: 0.41, green: 0.36, blue: 0.54, alpha: 1)
    ]

    init(frame: NSRect, displayID: CGDirectDisplayID, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        learnedGesture = defaults.bool(forKey: "snowman.gestureLearned")
        let identifier = CGDisplayCreateUUIDFromDisplayID(displayID).map {
            CFUUIDCreateString(nil, $0.takeRetainedValue()) as String
        } ?? String(displayID)
        storageKey = "snowmen.\(identifier)"
        super.init(frame: frame)
        autoresizingMask = [.width, .height]
        if let data = defaults.data(forKey: storageKey),
           let saved = try? JSONDecoder().decode([Figure].self, from: data) {
            figures = Array(saved.filter {
                $0.x.isFinite && $0.y.isFinite && (0...1).contains($0.x) && (0...1).contains($0.y)
                    && (1...3).contains($0.radii.count) && $0.radii.allSatisfy { $0.isFinite && (0.1...65).contains($0) }
                    && (0...2).contains($0.scarf)
                    && ($0.dust == nil || ($0.dust!.isFinite && (0...1000).contains($0.dust!)))
            }.prefix(24))
        }
    }
    deinit { animationTimer?.invalidate() }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override var acceptsFirstResponder: Bool { editing }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { editing ? super.hitTest(point) : nil }

    private func origin(_ figure: Figure) -> NSPoint {
        let r = CGFloat(figure.radii[0])
        let height = CGFloat(figure.radii.dropLast().reduce(0, +)) * 1.65 + CGFloat(figure.radii.last!) * 2
        return NSPoint(x: min(max(CGFloat(figure.x) * bounds.width, r + 4), max(r + 4, bounds.width - r - 4)),
                       y: min(max(CGFloat(figure.y) * bounds.height, 0), max(0, bounds.height - height - 28)))
    }
    private func circles(_ figure: Figure) -> [NSRect] {
        let base = origin(figure)
        var y = base.y
        let stretch = reducedMotion ? 1 : 1 + figure.motion.spring
        return figure.radii.map { radius in
            let r = CGFloat(radius)
            defer { y += r * 1.65 * stretch }
            return NSRect(x: base.x - r, y: y, width: r * 2, height: r * 2 * stretch)
        }
    }
    private func figure(at point: NSPoint) -> Int? {
        figures.indices.reversed().first { index in
            circles(figures[index]).contains { rect in
                hypot(point.x - rect.midX, point.y - rect.midY) <= rect.width / 2 + 6
            }
        }
    }
    func containsFigure(at point: NSPoint) -> Bool { figure(at: point) != nil }
    func startAnimating() { refreshAnimation() }

    private func refreshAnimation() {
        let moving = figures.contains { $0.motion.moving } || !powder.isEmpty
        let interval: TimeInterval = moving ? (powerSaving ? 1.0 / 30 : 1.0 / 60) : (powerSaving ? 0.2 : 0.1)
        guard active, !figures.isEmpty || !powder.isEmpty else {
            animationTimer?.invalidate(); animationTimer = nil; timerInterval = 0
            return
        }
        guard animationTimer == nil || timerInterval != interval else { return }
        animationTimer?.invalidate()
        timerInterval = interval; lastFrame = ProcessInfo.processInfo.systemUptime
        let timer = Timer(timeInterval: interval, repeats: true) { [weak self] _ in
            guard let self else { return }
            let now = ProcessInfo.processInfo.systemUptime
            advance(dt: CGFloat(min(0.1, now - lastFrame)))
            lastFrame = now
        }
        timer.tolerance = interval * 0.1
        RunLoop.main.add(timer, forMode: .common)
        animationTimer = timer
    }

    /// Also callable deterministically by the interaction checks.
    func advance(dt: CGFloat) {
        guard active, dt.isFinite, dt > 0 else { return }
        reducedMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        sceneTime += min(dt, 0.1)
        var changed = false
        if !dragging {
            // Still figures only need one support/footprint check. Throws keep the fine collision steps.
            let moving = figures.contains { $0.motion.moving }
            let steps = moving ? max(1, Int(ceil(min(dt, 0.1) * 120))) : 1
            for _ in 0..<steps {
                changed = advanceBodies(dt: min(dt, 0.1) / CGFloat(steps)) || changed
                changed = resolveContacts() || changed
            }
        }
        for index in powder.indices {
            powder[index].velocity.y -= 450 * dt
            powder[index].point.x += powder[index].velocity.x * dt
            powder[index].point.y += powder[index].velocity.y * dt
            powder[index].life -= dt
        }
        if !powder.isEmpty { changed = true }
        powder.removeAll { $0.life <= 0 }
        let look = window.map { convert($0.convertPoint(fromScreen: NSEvent.mouseLocation), from: nil) } ?? .zero
        let blink = sceneTime.truncatingRemainder(dividingBy: 5.7) > 5.54
        if figures.contains(where: { $0.radii.count == 3 }) {
            if hypot(look.x - lastLook.x, look.y - lastLook.y) > 4 || blink != lastBlink { changed = true }
            lastLook = look; lastBlink = blink
        }
        if changed { needsDisplay = true }
        if dirtyPersistence, sceneTime - savedAt > 1, !dragging {
            save(); dirtyPersistence = false
        }
        refreshAnimation()
    }

    private func mass(_ figure: Figure) -> CGFloat {
        CGFloat(figure.radii.reduce(0) { $0 + .pi * $1 * $1 } + (figure.dust ?? 0))
    }

    private func advanceBodies(dt: CGFloat) -> Bool {
        var changed = false
        for index in figures.indices {
            var point = origin(figures[index])
            let old = point
            var motion = figures[index].motion
            let contact = CGPoint(x: point.x, y: point.y + CGFloat(figures[index].radii[0]))
            var on = support(contact, motion.support)
            if let oldSupport = motion.support, let on, oldSupport.id == on.id {
                point.x += on.origin.x - oldSupport.origin.x
                point.y += on.height - oldSupport.height
            }
            if let surface = on, point.y <= surface.height + 0.5, motion.velocity.y <= 0 {
                pressSnow(point, CGFloat(figures[index].radii[0]), mass(figures[index]), dt, surface)
                if let pressed = support(contact, surface), pressed.id == surface.id {
                    point.y += pressed.height - surface.height
                    on = pressed
                }
            }
            let floor = on?.height ?? 0
            if point.y > floor + 0.5 { motion.support = nil }
            let wasMoving = motion.moving
            motion.advance(position: &point, radius: CGFloat(figures[index].radii[0]), floor: floor, width: bounds.width, dt: dt)
            motion.support = abs(point.y - floor) < 0.5 && motion.velocity.y == 0 ? on : nil
            let displaced = hypot(point.x - old.x, point.y - old.y) > 0.01
            changed = changed || wasMoving || displaced || motion.moving
            if displaced { dirtyPersistence = true }
            figures[index].motion = motion
            place(index, at: point)
        }
        return changed
    }

    private func contactFeedback(_ strength: CGFloat) {
        guard sceneTime - lastFeedback > 0.12, sceneTime - lastGestureAt < 2 else { return }
        lastFeedback = sceneTime
        feedback(min(1, max(0, strength)))
    }

    /// Catch only flakes that cross an exposed upper surface before reaching a window or drift.
    @discardableResult
    func catchSnow(x: CGFloat, from: CGFloat, to: CGFloat, area: CGFloat, above: CGFloat) -> Bool {
        guard active, !dragging, area > 0, from >= to else { return false }
        var hit: (index: Int, height: CGFloat)?
        for index in figures.indices {
            guard abs(x - origin(figures[index]).x) < CGFloat(figures[index].radii.max() ?? 0) else { continue }
            for circle in circles(figures[index]) {
                let r = circle.width / 2, dx = x - circle.midX
                guard abs(dx) < r else { continue }
                let surface = circle.midY + sqrt(max(0, r * r - dx * dx))
                if from >= surface, to <= surface, surface > above, hit == nil || surface > hit!.height {
                    hit = (index, surface)
                }
            }
        }
        guard let hit else { return false }
        let old = CGFloat(figures[hit.index].dust ?? 0)
        let cap = min(500, CGFloat(figures[hit.index].radii.reduce(0, +)) * 3)
        let kept = min(area, max(0, cap - old))
        figures[hit.index].dust = Double(old + kept)
        if area > kept { returnSnow(CGPoint(x: x, y: hit.height), area - kept) }
        dirtyPersistence = true; needsDisplay = true
        return true
    }

    private func shedDust(_ index: Int) {
        let area = CGFloat(figures[index].dust ?? 0)
        guard area > 0 else { return }
        figures[index].dust = nil
        // Shedding is permanent even if the user cancels the subsequent movement.
        if dragging, index < beforeDrag.count { beforeDrag[index].dust = nil }
        returnSnow(origin(figures[index]), area)
        burst(at: CGPoint(x: origin(figures[index]).x, y: circles(figures[index]).last!.maxY), radius: min(16, sqrt(area)))
        dirtyPersistence = true
    }

    @discardableResult
    private func resolveContacts() -> Bool {
        guard figures.count > 1 else { return false }
        var changed = false
        var knocked = Set<Int>()
        for a in 0..<(figures.count - 1) {
            for b in (a + 1)..<figures.count {
                if dragging {
                    if a != selected && b != selected { continue }
                    if snapTarget != nil { continue }
                    if a == separationSource || b == separationSource { continue }
                }
                let horizontal = abs(origin(figures[a]).x - origin(figures[b]).x)
                guard horizontal < CGFloat((figures[a].radii.max() ?? 0) + (figures[b].radii.max() ?? 0)) else { continue }
                var contact: (NSRect, NSRect, CGFloat)?
                for first in circles(figures[a]) {
                    for second in circles(figures[b]) {
                        let overlap = (first.width + second.width) / 2 - hypot(first.midX - second.midX, first.midY - second.midY)
                        if overlap > 0.1, contact == nil || overlap > contact!.2 { contact = (first, second, overlap) }
                    }
                }
                guard let (first, second, _) = contact else { continue }
                let centerA = CGPoint(x: first.midX, y: first.midY), centerB = CGPoint(x: second.midX, y: second.midY)
                var bodyA = SnowmanContact.Body(center: centerA, velocity: figures[a].motion.velocity, radius: first.width / 2, mass: mass(figures[a]), held: dragging && selected == a)
                var bodyB = SnowmanContact.Body(center: centerB, velocity: figures[b].motion.velocity, radius: second.width / 2, mass: mass(figures[b]), held: dragging && selected == b)
                guard let impact = SnowmanContact.resolve(&bodyA, &bodyB) else { continue }
                let originA = origin(figures[a]), originB = origin(figures[b])
                place(a, at: CGPoint(x: originA.x + bodyA.center.x - centerA.x, y: originA.y + bodyA.center.y - centerA.y))
                place(b, at: CGPoint(x: originB.x + bodyB.center.x - centerB.x, y: originB.y + bodyB.center.y - centerB.y))
                figures[a].motion.velocity = bodyA.velocity; figures[b].motion.velocity = bodyB.velocity
                if impact > 50 {
                    figures[a].motion.springVelocity -= min(1, impact / 500)
                    figures[b].motion.springVelocity -= min(1, impact / 500)
                    shedDust(a); shedDust(b)
                    contactFeedback(impact / 700)
                }
                if impact > 240 && !dragging {
                    if figures[a].radii.count > 1 { knocked.insert(a) }
                    if figures[b].radii.count > 1 { knocked.insert(b) }
                }
                changed = true; dirtyPersistence = true
            }
        }
        for index in knocked.sorted() where figures.count < 24 {
            let head = circles(figures[index]).last!
            let radius = figures[index].radii.removeLast()
            var loose = Figure(x: Double(head.midX / bounds.width), y: Double(head.minY / bounds.height), radii: [radius], scarf: figures[index].scarf)
            loose.motion.velocity = CGPoint(x: figures[index].motion.velocity.x + (figures[index].motion.velocity.x < 0 ? -100 : 100), y: 100)
            figures.append(loose)
            burst(at: CGPoint(x: head.midX, y: head.minY), radius: 8)
        }
        return changed
    }

    private func burst(at center: CGPoint, radius: CGFloat) {
        guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion else { return }
        let count = min(32, max(10, Int(radius)))
        for i in 0..<count {
            let angle = CGFloat(i) * 2.39996
            let speed = 35 + CGFloat(i % 7) * 16
            powder.append(Powder(point: center, velocity: CGPoint(x: cos(angle) * speed, y: sin(angle) * speed + 55), radius: 1 + CGFloat(i % 3) * 0.65, life: 0.45 + CGFloat(i % 5) * 0.09))
        }
        if powder.count > 256 { powder.removeFirst(powder.count - 256) }
    }

    /// Commit completed interactions on sleep, display removal and app termination.
    func preserveScene() {
        cancelDrag()
        save()
    }

    private func save() {
        dirtyPersistence = false
        savedAt = sceneTime
        if let data = try? JSONEncoder().encode(figures) { defaults.set(data, forKey: storageKey) }
        refreshAnimation()
    }
    private func place(_ index: Int, at point: NSPoint) {
        figures[index].x = Double(min(1, max(0, point.x / max(1, bounds.width))))
        figures[index].y = Double(min(1, max(0, point.y / max(1, bounds.height))))
    }
    private func attachmentPoint(for index: Int, onto other: Int) -> CGPoint {
        let top = circles(figures[other]).last!
        return CGPoint(x: top.midX, y: top.minY + top.height * 0.825 + CGFloat(figures[index].radii[0]))
    }
    private func attachment(for index: Int) -> Int? {
        guard figures[index].radii.count == 1 else { return nil }
        let ball = circles(figures[index])[0]
        var nearest: (index: Int, distance: CGFloat)?
        for other in figures.indices {
            guard other != index, other != separationSource, figures[other].radii.count < 3,
                  figures[index].radii[0] <= figures[other].radii.last! * 0.9 else { continue }
            let center = attachmentPoint(for: index, onto: other)
            let distance = hypot(ball.midX - center.x, ball.midY - center.y)
            let reach = max(24, ball.width / 2 + 12)
            // A small release margin keeps the guide and haptic from chattering at its edge.
            if other == snapTarget, distance < reach * 1.3 { return other }
            if distance < reach, nearest == nil || distance < nearest!.distance { nearest = (other, distance) }
        }
        return nearest?.index
    }
    /// Escape and system interruptions put collected snow back instead of losing it.
    func cancelDrag() {
        guard dragging else { return }
        figures = beforeDrag
        if collected > initialArea { returnSnow(start, collected - initialArea) }
        dragging = false; scooping = false; collected = 0; snapTarget = nil
        save(); needsDisplay = true
    }
    override func cancelOperation(_ sender: Any?) { cancelDrag(); finished() }
    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { cancelOperation(nil) }
        else { super.keyDown(with: event) }
    }
    override func rightMouseDown(with event: NSEvent) {
        guard editing, !dragging, event.modifierFlags.contains(.option),
              let index = figure(at: convert(event.locationInWindow, from: nil)) else { return }
        let removed = figures.remove(at: index)
        burst(at: origin(removed), radius: CGFloat(removed.radii[0]))
        lastGestureAt = sceneTime
        contactFeedback(0.6)
        let area = mass(removed)
        returnSnow(origin(removed), CGFloat(area))
        selected = nil; save(); needsDisplay = true
        finished()
    }
    override func mouseDown(with event: NSEvent) {
        guard editing else { return }
        optionGesture = event.modifierFlags.contains(.option)
        separateGesture = event.modifierFlags.contains(.shift)
        pendingSeparation = nil; separationSource = nil
        let point = convert(event.locationInWindow, from: nil)
        guard optionGesture || figure(at: point) != nil else { finished(); return }
        window?.makeKey()
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)
        previous = p; start = p; beforeDrag = figures; collected = 0; initialArea = 0
        lastGestureAt = sceneTime
        moved = false; throwGesture = SnowmanThrow()
        throwGesture.record(p, at: event.timestamp)
        if let hit = figure(at: p) {
            selected = hit
            clickedBall = circles(figures[hit]).enumerated().min { a, b in
                hypot(p.x - a.element.midX, p.y - a.element.midY) < hypot(p.x - b.element.midX, p.y - b.element.midY)
            }?.offset ?? 0
            figures[hit].motion = SnowmanMotion()
            if optionGesture, event.clickCount == 2, figures[hit].radii.count == 3 {
                figures[hit].scarf = (figures[hit].scarf + 1) % colors.count
                save(); needsDisplay = true; return
            }
            if separateGesture && figures[hit].radii.count > 1 { pendingSeparation = hit }
            scooping = optionGesture && !separateGesture && figures[hit].radii.count == 1
            if scooping {
                initialArea = .pi * CGFloat(figures[hit].radii[0] * figures[hit].radii[0])
                collected = initialArea
            }
        } else {
            guard figures.count < 24 else { return }
            let area = takeSnow(p, 14, .pi * 12 * 12)
            guard area > 0 else { return }
            collected = area
            let radius = sqrt(area / .pi)
            figures.append(Figure(x: 0, y: 0, radii: [Double(radius)], scarf: figures.count % colors.count))
            selected = figures.count - 1
            place(selected!, at: NSPoint(x: p.x, y: p.y - radius))
            scooping = true
        }
        dragOrigin = origin(figures[selected!])
        dragging = true; needsDisplay = true
        if scooping {
            learnedGesture = true
            defaults.set(true, forKey: "snowman.gestureLearned")
            contactFeedback(0.2)
        }
    }
    override func mouseDragged(with event: NSEvent) {
        guard dragging, var selected else { return }
        lastGestureAt = sceneTime
        let p = convert(event.locationInWindow, from: nil)
        let dx = p.x - previous.x, dy = p.y - previous.y
        moved = moved || hypot(p.x - start.x, p.y - start.y) > 4
        throwGesture.record(p, at: event.timestamp)
        if moved, let source = pendingSeparation {
            guard figures.count < 24 else { cancelDrag(); finished(); return }
            shedDust(source)
            let ball = circles(figures[source])[clickedBall]
            let radius = figures[source].radii.remove(at: clickedBall)
            figures[source].motion.springVelocity = -0.5
            let loose = Figure(x: Double(ball.midX / max(1, bounds.width)),
                               y: Double(ball.minY / max(1, bounds.height)), radii: [radius], scarf: figures[source].scarf)
            figures.append(loose)
            selected = figures.count - 1
            self.selected = selected
            separationSource = source; pendingSeparation = nil
            dragOrigin = CGPoint(x: ball.midX, y: ball.minY)
            scooping = false
            contactFeedback(0.3)
        }
        if let source = separationSource, source != selected,
           hypot(p.x - start.x, p.y - start.y) > CGFloat(figures[selected].radii[0]) * 2 + 16 {
            separationSource = nil
        }
        if moved { shedDust(selected) }
        if scooping {
            // Sample the whole stroke so fast trackpad gestures do not skip narrow drifts.
            let steps = max(1, min(256, Int(ceil(hypot(dx, dy) / 5))))
            for step in 1...steps {
                let fraction = CGFloat(step) / CGFloat(steps)
                let point = NSPoint(x: previous.x + dx * fraction, y: previous.y + dy * fraction)
                let remaining = max(0, .pi * 65 * 65 - collected)
                if remaining > 0 {
                    collected += takeSnow(point, min(28, max(14, sqrt(collected / .pi))), remaining)
                }
            }
            let radius = sqrt(collected / .pi)
            figures[selected].radii[0] = Double(radius)
            place(selected, at: NSPoint(x: p.x, y: p.y - radius))
        } else {
            place(selected, at: NSPoint(x: dragOrigin.x + p.x - start.x, y: dragOrigin.y + p.y - start.y))
        }
        let target = attachment(for: selected)
        if target != snapTarget, target != nil {
            NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        }
        snapTarget = target
        if let target {
            let ball = circles(figures[selected])[0]
            let center = attachmentPoint(for: selected, onto: target)
            let offset = CGPoint(x: center.x - ball.midX, y: center.y - ball.midY)
            let distance = hypot(offset.x, offset.y)
            let pull = min(0.4, 8 / max(1, distance))
            let current = origin(figures[selected])
            place(selected, at: CGPoint(x: current.x + offset.x * pull, y: current.y + offset.y * pull))
        }
        figures[selected].motion.velocity = throwGesture.velocity(at: event.timestamp)
        _ = resolveContacts()
        if let surface = support(origin(figures[selected]), nil), abs(origin(figures[selected]).y - surface.height) < 2 {
            pressSnow(origin(figures[selected]), CGFloat(figures[selected].radii[0]), mass(figures[selected]), 1.0 / 60, surface)
        }
        previous = p; needsDisplay = true
    }
    override func mouseUp(with event: NSEvent) {
        guard dragging else { return }
        if !moved, !optionGesture, !separateGesture, let selected {
            shedDust(selected)
            contactFeedback(0.5)
            let ball = circles(figures[selected])[clickedBall]
            let radius = figures[selected].radii.remove(at: clickedBall)
            returnSnow(CGPoint(x: ball.midX, y: ball.minY), CGFloat(.pi * radius * radius))
            burst(at: CGPoint(x: ball.midX, y: ball.midY), radius: CGFloat(radius))
            if figures[selected].radii.isEmpty { figures.remove(at: selected) }
            else { figures[selected].motion.springVelocity = -1.5 }
            self.selected = nil
        } else if let selected, let target = attachment(for: selected) {
            figures[target].dust = (figures[target].dust ?? 0) + (figures[selected].dust ?? 0)
            figures[target].radii.append(figures[selected].radii[0])
            figures[target].motion.springVelocity = -1.6
            if figures[target].radii.count == 3 { figures[target].motion.greeting = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 2.4 }
            figures.remove(at: selected)
            self.selected = target > selected ? target - 1 : target
            contactFeedback(0.6)
        } else if let selected {
            figures[selected].motion.velocity = throwGesture.velocity(at: event.timestamp)
            if figures[selected].motion.velocity == .zero { figures[selected].motion.velocity.y = -2 }
        }
        dragging = false; scooping = false; collected = 0; snapTarget = nil
        save(); needsDisplay = true
        finished()
    }
    override func draw(_ dirtyRect: NSRect) {
        if editing, let hover, !dragging, !learnedGesture {
            let text = tr("snowman.gesture")
            let attributes: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 12, weight: .medium), .foregroundColor: NSColor.white]
            let size = (text as NSString).size(withAttributes: attributes)
            let box = NSRect(x: min(max(8, hover.x - size.width / 2 - 12), max(8, bounds.width - size.width - 32)),
                             y: min(bounds.height - 40, hover.y + 30), width: size.width + 24, height: 28)
            NSColor.black.withAlphaComponent(0.78).setFill()
            NSBezierPath(roundedRect: box, xRadius: 14, yRadius: 14).fill()
            (text as NSString).draw(at: NSPoint(x: box.minX + 12, y: box.minY + 6), withAttributes: attributes)
        }
        if let target = snapTarget, let selected {
            let top = circles(figures[target]).last!
            let radius = CGFloat(figures[selected].radii[0])
            let guide = NSBezierPath(ovalIn: NSRect(x: top.midX - radius - 4, y: top.minY + top.height * 0.825 - 4,
                                                  width: radius * 2 + 8, height: radius * 2 + 8))
            NSColor.controlAccentColor.withAlphaComponent(0.65).setStroke()
            guide.lineWidth = 2; guide.stroke()
        }
        for (index, figure) in figures.enumerated() {
            drawFigure(figure, seed: index)
        }
        for puff in powder {
            NSColor.white.withAlphaComponent(min(0.85, max(0, puff.life * 1.5))).setFill()
            NSBezierPath(ovalIn: NSRect(x: puff.point.x - puff.radius, y: puff.point.y - puff.radius, width: puff.radius * 2, height: puff.radius * 2)).fill()
        }
    }

    private func drawFigure(_ figure: Figure, seed: Int) {
        let balls = circles(figure)
        guard let base = balls.first else { return }
        let calm = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        let greeting = calm ? 0 : figure.motion.greeting
        NSGraphicsContext.saveGraphicsState()
        defer { NSGraphicsContext.restoreGraphicsState() }
        // A soft footprint anchors the figure to its ledge, including while it is in the air.
        let floor = support(origin(figure), figure.motion.support)?.height ?? 0
        let lift = max(0, base.minY - floor)
        NSColor(srgbRed: 0.19, green: 0.29, blue: 0.38, alpha: 0.17 / (1 + lift / 70)).setFill()
        let shadowWidth = base.width * (0.8 + min(0.5, lift / 200))
        NSBezierPath(ovalIn: NSRect(x: base.midX - shadowWidth / 2, y: floor - 2, width: shadowWidth, height: 7)).fill()
        for (part, ball) in balls.enumerated() {
            let r = ball.width / 2
            let path = NSBezierPath()
            // A quiet, uneven silhouette reads as packed snow rather than plastic spheres.
            for step in 0...64 {
                let angle = CGFloat(step) * .pi / 32
                let grain = 1 + 0.013 * sin(angle * 7 + CGFloat(seed + part)) + 0.009 * cos(angle * 11)
                let point = CGPoint(x: ball.midX + cos(angle) * r * grain,
                                    y: ball.midY + sin(angle) * ball.height / 2 * grain)
                if step == 0 { path.move(to: point) } else { path.line(to: point) }
            }
            path.close()
            NSGraphicsContext.saveGraphicsState()
            let shadow = NSShadow()
            shadow.shadowColor = NSColor(srgbRed: 0.16, green: 0.26, blue: 0.35, alpha: 0.2)
            shadow.shadowBlurRadius = 3; shadow.shadowOffset = NSSize(width: 0, height: -2)
            shadow.set()
            NSGradient(colors: [NSColor(srgbRed: 1, green: 0.995, blue: 0.97, alpha: 1),
                                NSColor(srgbRed: 0.91, green: 0.95, blue: 0.97, alpha: 1),
                                NSColor(srgbRed: 0.61, green: 0.74, blue: 0.81, alpha: 1)])?
                .draw(in: path, relativeCenterPosition: NSPoint(x: -0.38, y: 0.48))
            NSGraphicsContext.restoreGraphicsState()
            NSGraphicsContext.saveGraphicsState()
            path.addClip()
            for grain in 0..<44 {
                let angle = CGFloat(grain) * 2.39996 + (balls.count == 1 ? figure.motion.rotation : 0)
                let distance = sqrt(CGFloat(grain + 1) / 45) * r * 0.94
                let size: CGFloat = grain % 3 == 0 ? 1.2 : 0.7
                (grain % 2 == 0 ? NSColor.white.withAlphaComponent(0.5) : NSColor(srgbRed: 0.42, green: 0.58, blue: 0.67, alpha: 0.13)).setFill()
                NSBezierPath(ovalIn: NSRect(x: ball.midX + cos(angle) * distance, y: ball.midY + sin(angle) * distance, width: size, height: size)).fill()
            }
            NSGraphicsContext.restoreGraphicsState()
            let dust = min(1, CGFloat(figure.dust ?? 0) / max(1, CGFloat(figure.radii.reduce(0, +)) * 1.2))
            if dust > 0.02 {
                let cap = NSBezierPath()
                cap.move(to: CGPoint(x: ball.midX - r * 0.74, y: ball.midY + r * 0.68))
                cap.curve(to: CGPoint(x: ball.midX + r * 0.74, y: ball.midY + r * 0.68),
                          controlPoint1: CGPoint(x: ball.midX - r * 0.4, y: ball.maxY + 1),
                          controlPoint2: CGPoint(x: ball.midX + r * 0.4, y: ball.maxY + 1))
                cap.lineWidth = 1 + dust * 3
                cap.lineCapStyle = .round
                NSColor.white.withAlphaComponent(0.25 + dust * 0.7).setStroke(); cap.stroke()
            }
        }
        guard balls.count == 3 else { return }
        let head = balls[2], body = balls[1]
        let r = head.width / 2
        let lookX = calm ? 0 : min(1, max(-1, (lastLook.x - head.midX) / 180)) * r * 0.08
        let lookY = calm ? 0 : min(1, max(-1, (lastLook.y - head.midY) / 180)) * r * 0.05
        let scarf = colors[figure.scarf]
        // Two folds and a knitted edge; the loose end responds to the greeting and a throw.
        let flutter = greeting > 0 ? sin(sceneTime * 7) * r * 0.12 : min(r * 0.2, max(-r * 0.2, figure.motion.velocity.x * 0.012))
        let tail = NSBezierPath()
        tail.move(to: CGPoint(x: head.midX + r * 0.13, y: head.minY + r * 0.2))
        tail.curve(to: CGPoint(x: head.midX + r * 0.5 + flutter, y: head.minY - r * 0.9), controlPoint1: CGPoint(x: head.midX + r * 0.55, y: head.minY - r * 0.1), controlPoint2: CGPoint(x: head.midX + flutter, y: head.minY - r * 0.65))
        tail.lineWidth = max(4, r * 0.32); tail.lineCapStyle = .round
        scarf.setStroke(); tail.stroke()
        let collar = NSBezierPath(roundedRect: NSRect(x: head.minX + 1, y: head.minY + 1, width: head.width - 2, height: max(4, r * 0.36)), xRadius: 4, yRadius: 4)
        NSGradient(starting: scarf.blended(withFraction: 0.22, of: .white)!, ending: scarf)?.draw(in: collar, angle: -90)
        NSColor.white.withAlphaComponent(0.22).setStroke()
        let seam = NSBezierPath()
        seam.move(to: CGPoint(x: head.minX + 4, y: head.minY + 4)); seam.line(to: CGPoint(x: head.maxX - 4, y: head.minY + 4))
        seam.lineWidth = 0.7; seam.stroke()
        let coal = NSColor(srgbRed: 0.17, green: 0.22, blue: 0.26, alpha: 1)
        coal.setFill()
        let blink = !calm && lastBlink
        for side: CGFloat in [-1, 1] {
            let eye = NSRect(x: head.midX + side * r * 0.33 + lookX - r * 0.075,
                             y: head.midY + r * 0.18 + lookY, width: max(2, r * 0.15), height: blink ? 1 : max(2.5, r * 0.19))
            NSBezierPath(ovalIn: eye).fill()
        }
        for bead in 0..<5 {
            let angle = CGFloat(bead) * .pi / 6 + .pi * 7 / 6
            let size = max(1, r * 0.085)
            NSBezierPath(ovalIn: NSRect(x: head.midX + cos(angle) * r * 0.35 + lookX, y: head.midY + sin(angle) * r * 0.36, width: size, height: size)).fill()
        }
        for fraction in [0.3, 0.51, 0.7] {
            let size = max(2, body.width * 0.07)
            NSBezierPath(ovalIn: NSRect(x: body.midX - size / 2, y: body.minY + body.height * fraction, width: size, height: size)).fill()
        }
        let nose = NSBezierPath()
        nose.move(to: NSPoint(x: head.midX + lookX, y: head.midY + r * 0.08))
        nose.line(to: NSPoint(x: head.midX + r * 0.7 + lookX, y: head.midY - r * 0.06))
        nose.line(to: NSPoint(x: head.midX + lookX, y: head.midY - r * 0.17)); nose.close()
        NSGradient(starting: NSColor(srgbRed: 0.96, green: 0.65, blue: 0.34, alpha: 1), ending: NSColor(srgbRed: 0.77, green: 0.36, blue: 0.18, alpha: 1))?.draw(in: nose, angle: -90)
        NSColor(srgbRed: 0.37, green: 0.29, blue: 0.23, alpha: 1).setStroke()
        for side: CGFloat in [-1, 1] {
            let wave = side > 0 && greeting > 0 ? (sin(sceneTime * 10) * 0.35 + 0.6) * min(1, greeting * 2) : 0
            let root = CGPoint(x: body.midX + side * body.width * 0.4, y: body.midY)
            let reach = body.width * 0.33 + 6
            let tip = CGPoint(x: root.x + side * reach * cos(wave), y: root.y + reach * (0.35 + sin(wave)))
            let arm = NSBezierPath()
            arm.move(to: root); arm.line(to: tip)
            arm.move(to: CGPoint(x: tip.x - side * 6, y: tip.y - 3)); arm.line(to: CGPoint(x: tip.x - side * 6, y: tip.y + 5))
            arm.lineWidth = max(1.5, r * 0.095); arm.lineCapStyle = .round; arm.stroke()
        }
    }
}
