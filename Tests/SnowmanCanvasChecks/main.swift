import AppKit
func tr(_ key: String) -> String { key }
final class MemoryDefaults: UserDefaults {
    var values: [String: Any] = [:]
    override func data(forKey key: String) -> Data? { values[key] as? Data }
    override func bool(forKey key: String) -> Bool { values[key] as? Bool ?? false }
    override func set(_ value: Any?, forKey key: String) { values[key] = value }
}
let app = NSApplication.shared
let defaults = MemoryDefaults()
let canvas = SnowmanCanvas(frame: NSRect(x: 0, y: 0, width: 1200, height: 800), displayID: 0, defaults: defaults)
func event(_ type: NSEvent.EventType, _ x: CGFloat, _ y: CGFloat, option: Bool = true, time: TimeInterval = 0, shift: Bool = false) -> NSEvent {
    NSEvent.mouseEvent(with: type, location: NSPoint(x: x, y: y), modifierFlags: (option ? NSEvent.ModifierFlags.option : []).union(shift ? .shift : []), timestamp: time, windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
}
func saved() -> [[String: Any]] {
    guard let data = defaults.values.values.compactMap({ $0 as? Data }).first else { return [] }
    return (try! JSONSerialization.jsonObject(with: data)) as! [[String: Any]]
}
var snow: CGFloat = .pi * 12 * 12
var returned: CGFloat = 0
canvas.takeSnow = { _, _, limit in let amount = min(limit, snow); snow -= amount; return amount }
canvas.returnSnow = { _, area in returned += area; snow += area }
canvas.finished = { canvas.editing = false }
assert(canvas.hitTest(NSPoint(x: 400, y: 100)) == nil)
canvas.editing = true
canvas.mouseDown(with: event(.leftMouseDown, 400, 100, option: false))
assert(!canvas.dragging && saved().isEmpty)
canvas.editing = true
canvas.mouseDown(with: event(.leftMouseDown, 400, 100))
canvas.mouseUp(with: event(.leftMouseUp, 400, 100))
assert(saved().count == 1 && snow < 0.001)
assert(abs((saved()[0]["radii"] as! [Double])[0] - 12) < 0.001)
// Empty desktop cannot manufacture snow.
canvas.editing = true
canvas.mouseDown(with: event(.leftMouseDown, 800, 200))
assert(!canvas.dragging && saved().count == 1)
// Roll a smaller ball and snap it to the existing base without changing its area.
snow = .pi * 8 * 8
canvas.mouseDown(with: event(.leftMouseDown, 600, 100))
canvas.mouseDragged(with: event(.leftMouseDragged, 400, 115.8))
canvas.mouseUp(with: event(.leftMouseUp, 400, 115.8))
assert(saved().count == 1)
assert((saved()[0]["radii"] as! [Double]).count == 2)
// A third ball completes the snowman.
snow = .pi * 5 * 5
canvas.editing = true
canvas.mouseDown(with: event(.leftMouseDown, 700, 100))
canvas.mouseDragged(with: event(.leftMouseDragged, 400, 126))
canvas.mouseUp(with: event(.leftMouseUp, 400, 126))
assert((saved()[0]["radii"] as! [Double]).count == 3)
// Cancellation refunds only newly gathered snow.
snow = 100
canvas.editing = true
canvas.mouseDown(with: event(.leftMouseDown, 800, 100))
canvas.cancelOperation(nil)
assert(abs(returned - 100) < 0.001 && saved().count == 1)
assert(!canvas.dragging && canvas.hitTest(NSPoint(x: 400, y: 100)) == nil)
let restored = SnowmanCanvas(frame: canvas.frame, displayID: 0, defaults: defaults)
assert(restored.containsFigure(at: NSPoint(x: 400, y: 100)))
var crumbled: CGFloat = 0
restored.returnSnow = { _, area in crumbled += area }
restored.editing = true
restored.rightMouseDown(with: event(.rightMouseDown, 400, 100))
assert(abs(crumbled - .pi * (144 + 64 + 25)) < 0.01)
assert(saved().isEmpty)
// Real height-field collection: area is conserved and foliage is not consumed.
let layer = SnowLayer(origin: .zero, columns: 100, columnWidth: 2, depth: 40, opensAtEdges: false, phases: (0, 0))
layer.replace(heights: Array(repeating: 20, count: 100), leaves: Array(repeating: 5, count: 100))
let before = layer.heights.reduce(0, +) * 2
let taken = SnowSculpting.take(at: SIMD2(100, 18), radius: 14, limit: 150, from: [layer])
let after = layer.heights.reduce(0, +) * 2
assert(taken > 0 && taken <= 150 && abs(before - after - taken) < 0.01)
assert(layer.leaves.allSatisfy { $0 == 5 })
layer.occluders = [SIMD4(0, 0, 200, 100)]
assert(SnowSculpting.take(at: SIMD2(50, 18), radius: 14, limit: 150, from: [layer]) == 0)
assert(SnowSculpting.take(at: SIMD2(500, 500), radius: 14, limit: 150, from: [layer]) == 0)
print("PASS: Option gesture, real snow mass, no snow creation, stacking, cancellation, persistence, occlusion, foliage, click-through")

// Gravity, damping and collision remain bounded and settle exactly on their support.
var motion = SnowmanMotion()
motion.velocity = CGPoint(x: 350, y: 80)
var position = CGPoint(x: 200, y: 300)
for _ in 0..<900 { motion.advance(position: &position, radius: 20, floor: 25, width: 600, dt: 1.0 / 60) }
assert(abs(position.y - 25) < 0.01 && abs(motion.velocity.x) < 1 && motion.velocity.y == 0)
assert(position.x >= 24 && position.x <= 576 && !motion.moving)
motion.advance(position: &position, radius: 20, floor: 25, width: 600, dt: 100)
assert(position.x.isFinite && position.y.isFinite)
// A supported figure follows its window, and loses that support when the window closes.
let ground = SnowLayer(origin: .zero, columns: 500, columnWidth: 2, depth: 40, opensAtEdges: false, phases: (0, 0))
let ledge = SnowLayer(origin: SIMD2(100, 180), columns: 100, columnWidth: 2, depth: 40, opensAtEdges: false, phases: (0, 0))
let firstSupport = SnowSculpting.support(at: CGPoint(x: 150, y: 200), previous: nil, in: [ground, ledge])!
assert(firstSupport.id == ObjectIdentifier(ledge))
ledge.origin = SIMD2(350, 230)
let carried = SnowSculpting.support(at: CGPoint(x: 150, y: 200), previous: firstSupport, in: [ground, ledge])!
assert(carried.id == firstSupport.id && carried.origin.x - firstSupport.origin.x == 250)
let dropped = SnowSculpting.support(at: CGPoint(x: 400, y: 230), previous: carried, in: [ground])!
assert(dropped.id == ObjectIdentifier(ground) && dropped.height == 0)
// Plain clicks break only the sphere aimed at; the remainder and its snow are conserved.
let sceneDefaults = MemoryDefaults()
let sceneKey = defaults.values.keys.first { defaults.values[$0] is Data }!
sceneDefaults.set(try! JSONSerialization.data(withJSONObject: [["x": 0.5, "y": 0.1, "radii": [40, 28, 18], "scarf": 0]]), forKey: sceneKey)
let scene = SnowmanCanvas(frame: NSRect(x: 0, y: 0, width: 600, height: 360), displayID: 0, defaults: sceneDefaults)
var released: CGFloat = 0
scene.returnSnow = { _, area in released += area }
scene.editing = true
scene.mouseDown(with: event(.leftMouseDown, 300, 75, option: false))
scene.mouseUp(with: event(.leftMouseUp, 300, 75, option: false))
let remainder = try! JSONSerialization.jsonObject(with: sceneDefaults.data(forKey: sceneKey)!) as! [[String: Any]]
assert((remainder[0]["radii"] as! [Double]) == [28, 18])
assert(abs(released - .pi * 1600) < 0.01)
scene.active = false
print("PASS: gravity, stable landing, moving windows, closed windows, per-ball click destruction")
// Consistent trajectories at different refresh rates; a stalled/negative frame cannot reverse time.
func flight(_ rate: Int) -> CGPoint {
    var motion = SnowmanMotion()
    motion.velocity = CGPoint(x: 120, y: 30)
    var point = CGPoint(x: 150, y: 250)
    for _ in 0..<rate { motion.advance(position: &point, radius: 15, floor: 0, width: 1200, dt: 1 / CGFloat(rate)) }
    return point
}
let slow = flight(30), fast = flight(120)
assert(hypot(slow.x - fast.x, slow.y - fast.y) < 0.05)
let beforeInvalid = position
motion.advance(position: &position, radius: 20, floor: 25, width: 600, dt: -1)
assert(position == beforeInvalid)
var flick = SnowmanThrow()
for i in 0...10 { flick.record(CGPoint(x: CGFloat(i) * 4, y: 0), at: Double(i) * 0.008) }
let throwSpeed = flick.velocity(at: 0.08)
assert(abs(throwSpeed.x - 500) < 0.01 && throwSpeed.y == 0)
flick.record(CGPoint(x: 40.02, y: 0), at: 0.0801) // a tiny final event must not kill the throw
assert(flick.velocity(at: 0.081).x > 450)
assert(flick.velocity(at: 0.3) == .zero) // hold still, then release: place rather than throw
var fastFlick = SnowmanThrow()
fastFlick.record(.zero, at: 0)
fastFlick.record(CGPoint(x: 100, y: 100), at: 0.02)
assert(abs(hypot(fastFlick.velocity(at: 0.02).x, fastFlick.velocity(at: 0.02).y) - 900) < 0.01)
// Two nearby stacks: attach to the nearest eligible one, regardless of insertion order.
let magneticDefaults = MemoryDefaults()
magneticDefaults.set(try! JSONSerialization.data(withJSONObject: [
    ["x": 0.4, "y": 0.1, "radii": [40], "scarf": 0],
    ["x": 0.44, "y": 0.1, "radii": [40], "scarf": 1],
    ["x": 400.0 / 600, "y": 100.0 / 360, "radii": [10], "scarf": 2]
]), forKey: sceneKey)
let magnetic = SnowmanCanvas(frame: NSRect(x: 0, y: 0, width: 600, height: 360), displayID: 0, defaults: magneticDefaults)
magnetic.editing = true
magnetic.mouseDown(with: event(.leftMouseDown, 400, 110, option: false))
magnetic.mouseDragged(with: event(.leftMouseDragged, 247, 112, option: false))
magnetic.mouseUp(with: event(.leftMouseUp, 247, 112, option: false))
let stacks = try! JSONSerialization.jsonObject(with: magneticDefaults.data(forKey: sceneKey)!) as! [[String: Any]]
assert(stacks.count == 2 && (stacks[0]["radii"] as! [Double]) == [40, 10])
assert((stacks[1]["radii"] as! [Double]) == [40])
magnetic.active = false
print("PASS: frame-rate consistency, invalid time, stable flicks, paused release, speed limit, nearest magnetic stack")
// Surface memory: pressure changes cohesion, never snow area or leaf content.
func drift(_ profile: [Float]) -> SnowLayer {
    let layer = SnowLayer(origin: .zero, columns: profile.count, columnWidth: 2, depth: 40, opensAtEdges: false, phases: (0, 0))
    layer.replace(heights: profile); layer.refreshTop()
    return layer
}
let tracked = drift(Array(repeating: 12, count: 41))
let snowArea = tracked.heights.reduce(0, +) * 2
for _ in 0..<12 { tracked.compact(20, pressure: 0.3) }
assert(tracked.firmness(20) > 0.98 && tracked.heights.reduce(0, +) * 2 == snowArea)
assert(tracked.sweepYield(20, speed: 20) < 0.32)
assert(tracked.sweepYield(20, speed: 700) == 1)
let firmBefore = tracked.firmness(20)
tracked.deposit(0.2, at: tracked.centerX(20), wide: true, fresh: true)
assert(tracked.firmness(20) < firmBefore && tracked.firmness(20) > 0.9)
for _ in 0..<30 { tracked.deposit(2, at: tracked.centerX(20), wide: true, fresh: true) }
assert(tracked.firmness(20) == 0)
// Banks retain a groove after the cursor leaves; a fresh blanket softens it gradually.
var profile = Array(repeating: Float(12), count: 41)
profile[20] = 8; profile[19] = 9; profile[21] = 9
profile[17] = 16; profile[23] = 16; profile[16] = 13; profile[24] = 13
let softDrift = drift(profile), firmDrift = drift(profile)
for i in 15...25 { firmDrift.compact(i, pressure: 1) }
let areaBefore = firmDrift.heights.reduce(0, +) * 2
for _ in 0..<300 {
    _ = softDrift.relax(repose: 0.5, rounding: 0.12)
    _ = firmDrift.relax(repose: 0.5, rounding: 0.12)
}
assert(firmDrift.heights[20] < softDrift.heights[20] - 0.5)
assert(abs(firmDrift.heights.reduce(0, +) * 2 - areaBefore) < 0.02)
let depthBefore = firmDrift.heights[17] - firmDrift.heights[20]
var added: Float = 0
for _ in 0..<100 {
    for i in 0..<firmDrift.count {
        added += 0.4 - firmDrift.deposit(0.4, at: firmDrift.centerX(i), wide: true, fresh: true)
    }
    _ = firmDrift.relax(repose: 0.5, rounding: 0.12)
}
assert(firmDrift.heights[17] - firmDrift.heights[20] < depthBefore * 0.5)
assert(abs(firmDrift.heights.reduce(0, +) * 2 - areaBefore - added) < 0.15)
// Lifecycle invariants: surface memory survives resizing and follows moving columns.
tracked.compact(20, pressure: 1)
let remembered = tracked.crust[20]
tracked.resize(columns: 50)
assert(tracked.crust.count == 50 && tracked.crust[20] == remembered && tracked.crust[49] == 0)
_ = tracked.slideColumn(toward: 1)
assert(tracked.crust[21] == remembered)
tracked.remove(snow: 1000, from: 21)
assert(tracked.crust[21] == 0)
let foliage = drift(Array(repeating: 10, count: 20))
foliage.replace(heights: Array(repeating: 10, count: 20), leaves: Array(repeating: 10, count: 20))
foliage.compact(10, pressure: 1)
assert(foliage.firmness(10) == 0 && foliage.leaves[10] == 10)
print("PASS: compaction without mass loss, slow/fast resistance, persistent grooves, gradual snowfall recovery, resizing, foliage")
// Contact conserves momentum and damps energy; a held object is never displaced by another ball.
var bodyA = SnowmanContact.Body(center: CGPoint(x: 0, y: 0), velocity: CGPoint(x: 100, y: 0), radius: 10, mass: 100)
var bodyB = SnowmanContact.Body(center: CGPoint(x: 19, y: 0), velocity: .zero, radius: 10, mass: 100)
let contactSpeed = SnowmanContact.resolve(&bodyA, &bodyB)!
assert(contactSpeed == 100 && bodyB.velocity.x > 0)
assert(abs(bodyA.velocity.x * 100 + bodyB.velocity.x * 100 - 10_000) < 0.01)
assert(bodyA.velocity.x * bodyA.velocity.x + bodyB.velocity.x * bodyB.velocity.x <= 10_000)
assert(bodyB.center.x - bodyA.center.x >= 19.9)
var held = SnowmanContact.Body(center: .zero, velocity: CGPoint(x: 30, y: 0), radius: 10, mass: 100, held: true)
var pushed = SnowmanContact.Body(center: CGPoint(x: 18, y: 0), velocity: .zero, radius: 10, mass: 100)
_ = SnowmanContact.resolve(&held, &pushed)
assert(held.center == .zero && pushed.center.x > 19.9)
// A heavier figure leaves a deeper, finite footprint; displaced snow is banked, not deleted.
let lightBed = drift(Array(repeating: 20, count: 100))
let heavyBed = drift(Array(repeating: 20, count: 100))
for _ in 0..<240 {
    SnowSculpting.press(on: lightBed, x: 100, radius: 25, mass: 500, dt: 1.0 / 60)
    SnowSculpting.press(on: heavyBed, x: 100, radius: 25, mass: 8000, dt: 1.0 / 60)
}
assert(heavyBed.heights[50] < lightBed.heights[50] - 1)
assert(abs(heavyBed.heights.reduce(0, +) * 2 - 4000) < 0.1)
let settledFootprint = heavyBed.heights[50]
for _ in 0..<240 { SnowSculpting.press(on: heavyBed, x: 100, radius: 25, mass: 8000, dt: 1.0 / 60) }
assert(abs(heavyBed.heights[50] - settledFootprint) < 0.02)
// Equal weight spreads across a larger sphere; the centre sinks more than the edge of its contact patch.
let narrowFoot = drift(Array(repeating: 20, count: 150))
let wideFoot = drift(Array(repeating: 20, count: 150))
let slowFoot = drift(Array(repeating: 20, count: 150))
let fastFoot = drift(Array(repeating: 20, count: 150))
for _ in 0..<120 {
    SnowSculpting.press(on: narrowFoot, x: 151, radius: 25, mass: 3000, dt: 1.0 / 60)
    SnowSculpting.press(on: wideFoot, x: 151, radius: 50, mass: 3000, dt: 1.0 / 60)
}
assert(narrowFoot.heights[75] < wideFoot.heights[75] - 0.5)
assert(narrowFoot.heights[75] < narrowFoot.heights[80])
for _ in 0..<60 { SnowSculpting.press(on: slowFoot, x: 151, radius: 30, mass: 4000, dt: 1.0 / 30) }
for _ in 0..<240 { SnowSculpting.press(on: fastFoot, x: 151, radius: 30, mass: 4000, dt: 1.0 / 120) }
assert(zip(slowFoot.heights, fastFoot.heights).allSatisfy { abs($0 - $1) < 0.03 })
let pristineFoot = slowFoot.heights
assert(SnowSculpting.press(on: slowFoot, x: .nan, radius: 30, mass: 4000, dt: 0.1) == 0)
assert(SnowSculpting.press(on: slowFoot, x: -1000, radius: 30, mass: 4000, dt: 0.1) == 0)
assert(slowFoot.heights == pristineFoot)
let mixedFoot = SnowLayer(origin: .zero, columns: 100, columnWidth: 2, depth: 40, opensAtEdges: true, phases: (0, 0))
mixedFoot.replace(heights: Array(repeating: 20, count: 100), leaves: Array(repeating: 3, count: 100))
mixedFoot.refreshTop()
for _ in 0..<120 { SnowSculpting.press(on: mixedFoot, x: 3, radius: 30, mass: 4000, dt: 1.0 / 60) }
assert(mixedFoot.leaves.allSatisfy { $0 == 3 })
assert(abs(mixedFoot.heights.reduce(0, +) * 2 + mixedFoot.spill.left + mixedFoot.spill.right - 4000) < 0.1)
print("PASS: spherical pressure, weight distribution, 30/120 Hz footprint consistency, edge mass conservation, preserved foliage, invalid inputs")
// Actual falling snow is caught once, and shedding followed by Escape never duplicates it.
let dustDefaults = MemoryDefaults()
dustDefaults.set(try! JSONSerialization.data(withJSONObject: [["x": 0.5, "y": 0.0, "radii": [40, 28, 18], "scarf": 0]]), forKey: sceneKey)
let dustScene = SnowmanCanvas(frame: NSRect(x: 0, y: 0, width: 600, height: 360), displayID: 0, defaults: dustDefaults)
var shed: CGFloat = 0
dustScene.returnSnow = { _, area in shed += area }
assert(!dustScene.catchSnow(x: 300, from: 300, to: 0, area: 20, above: 250))
assert(dustScene.catchSnow(x: 300, from: 300, to: 0, area: 20, above: 0))
dustScene.editing = true
dustScene.mouseDown(with: event(.leftMouseDown, 300, 40, option: false))
dustScene.mouseDragged(with: event(.leftMouseDragged, 350, 40, option: false))
dustScene.cancelOperation(nil)
assert(abs(shed - 20) < 0.001)
let afterCancel = try! JSONSerialization.jsonObject(with: dustDefaults.data(forKey: sceneKey)!) as! [[String: Any]]
assert(afterCancel[0]["dust"] == nil)
dustScene.active = false
assert(!dustScene.catchSnow(x: 300, from: 300, to: 0, area: 20, above: 0))
assert(defaults.bool(forKey: "snowman.gestureLearned"))
// A thrown ball knocks a head loose, keeping the original snow area in the scene.
let impactDefaults = MemoryDefaults()
impactDefaults.set(try! JSONSerialization.data(withJSONObject: [
    ["x": 140.0 / 600, "y": 0.0, "radii": [15], "scarf": 0],
    ["x": 200.0 / 600, "y": 0.0, "radii": [40, 28, 18], "scarf": 1]
]), forKey: sceneKey)
let impactScene = SnowmanCanvas(frame: NSRect(x: 0, y: 0, width: 600, height: 360), displayID: 0, defaults: impactDefaults)
impactScene.editing = true
impactScene.mouseDown(with: event(.leftMouseDown, 140, 15, option: false, time: 1))
impactScene.mouseDragged(with: event(.leftMouseDragged, 150, 15, option: false, time: 1.025))
impactScene.mouseUp(with: event(.leftMouseUp, 150, 15, option: false, time: 1.026))
for _ in 0..<900 { impactScene.advance(dt: 1.0 / 60) }
let pieces = try! JSONSerialization.jsonObject(with: impactDefaults.data(forKey: sceneKey)!) as! [[String: Any]]
assert(pieces.count >= 3)
let keptArea = pieces.reduce(0.0) { sum, figure in sum + (figure["radii"] as! [Double]).reduce(0) { $0 + $1 * $1 } }
assert(abs(keptArea - (225 + 1600 + 784 + 324)) < 0.01)
impactScene.active = false
print("PASS: momentum, damped contacts, held bodies, weight footprints, snow conservation, dust capture/cancellation, impact breakup, learned hint")
// Shift-drag lifts exactly the aimed sphere. Escape restores the stack without duplicating dust.
func separationScene() -> (SnowmanCanvas, MemoryDefaults) {
    let store = MemoryDefaults()
    store.set(try! JSONSerialization.data(withJSONObject: [["x": 0.5, "y": 0.1, "radii": [40, 28, 18], "scarf": 2, "dust": 8]]), forKey: sceneKey)
    let view = SnowmanCanvas(frame: NSRect(x: 0, y: 0, width: 600, height: 360), displayID: 0, defaults: store)
    return (view, store)
}
let (separate, separateStore) = separationScene()
var separationDust: CGFloat = 0
separate.returnSnow = { _, area in separationDust += area }
separate.editing = true
separate.mouseDown(with: event(.leftMouseDown, 300, 166, option: false, time: 1, shift: true))
separate.mouseDragged(with: event(.leftMouseDragged, 440, 220, option: false, time: 1.1, shift: true))
separate.cancelOperation(nil)
let restoredStack = try! JSONSerialization.jsonObject(with: separateStore.data(forKey: sceneKey)!) as! [[String: Any]]
assert(restoredStack.count == 1 && (restoredStack[0]["radii"] as! [Double]) == [40, 28, 18])
assert(separationDust == 8 && restoredStack[0]["dust"] == nil)
separate.editing = true
separate.mouseDown(with: event(.leftMouseDown, 300, 130, option: false, time: 2, shift: true))
separate.mouseDragged(with: event(.leftMouseDragged, 440, 180, option: false, time: 2.1, shift: true))
separate.mouseUp(with: event(.leftMouseUp, 440, 180, option: false, time: 2.3, shift: true))
let separated = try! JSONSerialization.jsonObject(with: separateStore.data(forKey: sceneKey)!) as! [[String: Any]]
assert(separated.count == 2 && (separated[0]["radii"] as! [Double]) == [40, 18])
assert((separated[1]["radii"] as! [Double]) == [28])
assert(separationDust == 8)
separate.active = false
// Shift-click without dragging is harmless; it must not break the sphere.
let (shiftClick, shiftStore) = separationScene()
shiftClick.editing = true
shiftClick.mouseDown(with: event(.leftMouseDown, 300, 166, option: false, shift: true))
shiftClick.mouseUp(with: event(.leftMouseUp, 300, 166, option: false, shift: true))
let unchangedStack = try! JSONSerialization.jsonObject(with: shiftStore.data(forKey: sceneKey)!) as! [[String: Any]]
assert((unchangedStack[0]["radii"] as! [Double]) == [40, 28, 18])
shiftClick.active = false
// Suspension commits the latest moving position instead of the previous mouse-up position.
let (suspending, suspensionStore) = separationScene()
suspending.editing = true
suspending.mouseDown(with: event(.leftMouseDown, 300, 76, option: false, time: 3))
suspending.mouseDragged(with: event(.leftMouseDragged, 340, 100, option: false, time: 3.08))
suspending.mouseUp(with: event(.leftMouseUp, 340, 100, option: false, time: 3.081))
let atRelease = suspensionStore.data(forKey: sceneKey)!
suspending.advance(dt: 0.05)
suspending.active = false
let atSuspension = suspensionStore.data(forKey: sceneKey)!
assert(atRelease != atSuspension)
let resumed = SnowmanCanvas(frame: suspending.frame, displayID: 0, defaults: suspensionStore)
resumed.preserveScene()
let resumedScene = try! JSONSerialization.jsonObject(with: suspensionStore.data(forKey: sceneKey)!) as! NSArray
let suspendedScene = try! JSONSerialization.jsonObject(with: atSuspension) as! NSArray
assert(resumedScene == suspendedScene)
resumed.active = false
print("PASS: sphere separation, canceled separation, dust conservation, harmless Shift-click, suspension/restart persistence")
// One continuous weather field drives every display. Gusts are bounded and ramp smoothly.
var previousWind = SnowWeather.sample(at: 0).wind
var sawLift = false
for tick in 1...36000 {
    let sample = SnowWeather.sample(at: Double(tick) / 60)
    assert(abs(sample.wind) <= 55.01 && abs(sample.wind - previousWind) < 0.3)
    assert((0...1).contains(sample.lift))
    sawLift = sawLift || sample.lift > 0.3
    previousWind = sample.wind
}
assert(sawLift && SnowWeather.sample(at: 100).wind == SnowWeather.sample(at: 100).wind)
assert(SnowWeather.sample(at: .infinity).wind == 0)
// Dense snow melts more slowly; particles, leaves and area remain well behaved.
let looseMelt = drift(Array(repeating: 12, count: 30)), denseMelt = drift(Array(repeating: 12, count: 30))
for i in 0..<denseMelt.count { denseMelt.compact(i, pressure: 1) }
for _ in 0..<60 {
    _ = looseMelt.thaw(dt: 1.0 / 60, down: 0, rate: 1, settle: 100)
    _ = denseMelt.thaw(dt: 1.0 / 60, down: 0, rate: 1, settle: 100)
}
assert(denseMelt.heights[15] > looseMelt.heights[15] + 0.3)
let negativeMelt = denseMelt.heights
_ = denseMelt.thaw(dt: -1, down: 0, rate: 1, settle: 100)
assert(denseMelt.heights == negativeMelt)
// Ledges hold a lip briefly, then release only the snow already there, never its leaves.
let cornice = SnowLayer(origin: .zero, columns: 30, columnWidth: 2, depth: 40, opensAtEdges: true, phases: (0, 0))
cornice.replace(heights: Array(repeating: 20, count: 30), leaves: Array(repeating: 3, count: 30))
cornice.refreshTop()
let corniceArea = cornice.heights.reduce(0, +) * 2
assert(cornice.breakCornices(dt: 1.0 / 60).isEmpty)
var brokenArea: Float = 0
for _ in 0..<600 { brokenArea += cornice.breakCornices(dt: 1.0 / 60).reduce(0) { $0 + $1.area } }
assert(brokenArea > 0)
assert(abs(cornice.heights.reduce(0, +) * 2 + brokenArea - corniceArea) < 0.02)
assert(cornice.leaves.allSatisfy { $0 == 3 })
assert(denseMelt.breakCornices(dt: 100).isEmpty) // solid ground never breaks off a ledge
// All six GPU rows are populated; firm-snow highlights and edge extension survive buffer export.
var exported = [Float](repeating: .nan, count: cornice.count * SnowLayer.rows)
exported.withUnsafeMutableBufferPointer { cornice.writeSmoothed(into: $0.baseAddress!) }
assert(SnowLayer.rows == 6 && exported.allSatisfy(\.isFinite))
assert(exported[(cornice.count * 5)...].allSatisfy { $0 == 1 })
print("PASS: bounded coherent wind, smooth gusts, density-aware melting, timed cornice fracture, conserved mass, six GPU rows")
