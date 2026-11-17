import AppKit
import simd

/// Where in a wallpaper's picture a figure can walk: ground that faces up at least a little, with no drop on the way, in
/// stretches big enough to wander about, a cell to every `cell` points from the screen's top left.
struct SnowmanGround {
    /// Tells one ground from another.
    let id = Int.random(in: 1...Int.max)
    let width: Int
    let height: Int
    let cell: Float
    let screen: SIMD2<Float>
    /// How near each cell is, on the picture's own scale; below zero where it can't be walked.
    let near: [Float]
    /// Which stretch of ground each cell is in: one can't be walked to from another.
    private let group: [Int32]
    let far: Float
    let close: Float
    let light: WallpaperLight

    private static let cellPoints = 8

    init?(scene: WallpaperScene, screen size: CGSize) {
        let relief = scene.relief
        let step = max(Self.cellPoints / max(relief.cell, 1), 1)
        let width = relief.width / step, height = relief.height / step
        guard width > 4, height > 4 else { return nil }
        let holds = [UInt8](relief.hold), nears = [UInt8](relief.near)
        guard holds.count >= relief.width * relief.height, nears.count >= relief.width * relief.height else { return nil }
        let screen = SIMD2(Float(size.width), Float(size.height))
        let cell = Float(step * relief.cell)
        let far = scene.terrain.far, close = scene.terrain.close
        var near = [Float](repeating: -1, count: width * height)
        for cy in 0..<height {
            let top = Float(cy) * cell
            // Not in the sky's part of the screen, nor under the Dock.
            guard top > screen.y * 0.1, top + cell < screen.y - 70 else { continue }
            for cx in 0..<width {
                let left = Float(cx) * cell
                guard left > 20, left + cell < screen.x - 20 else { continue }
                var hold = 0, lowest: UInt8 = 255, highest: UInt8 = 0, sum = 0
                for y in (cy * step)..<(cy * step + step) {
                    for x in (cx * step)..<(cx * step + step) {
                        let i = y * relief.width + x
                        hold += Int(holds[i])
                        let n = nears[i]
                        lowest = min(lowest, n)
                        highest = max(highest, n)
                        sum += Int(n)
                    }
                }
                let count = step * step
                let average = Float(sum) / Float(count) / 255
                let nearness = (average - far) / max(close - far, 0.01)
                guard Float(hold) / Float(count) / 255 >= 0.2, Float(lowest) / 255 > Wallpaper.ground + 0.03,
                      Float(highest - lowest) / 255 < 0.06, nearness > 0.04, nearness < 0.97 else { continue }
                near[cy * width + cx] = average
            }
        }
        // Only stretches big enough to walk about in.
        var group = [Int32](repeating: -1, count: width * height)
        var sizes: [Int] = []
        for start in near.indices where near[start] >= 0 && group[start] < 0 {
            let id = Int32(sizes.count)
            var stack = [start], count = 0
            group[start] = id
            while let i = stack.popLast() {
                count += 1
                let x = i % width, y = i / width
                for dy in -1...1 {
                    for dx in -1...1 where dx != 0 || dy != 0 {
                        let nx = x + dx, ny = y + dy
                        guard nx >= 0, nx < width, ny >= 0, ny < height else { continue }
                        let j = ny * width + nx
                        guard group[j] < 0, near[j] >= 0, abs(near[j] - near[i]) <= Self.rise(dx, dy) else { continue }
                        group[j] = id
                        stack.append(j)
                    }
                }
            }
            sizes.append(count)
        }
        guard sizes.contains(where: { $0 >= 60 }) else { return nil }
        for i in near.indices where group[i] < 0 || sizes[Int(group[i])] < 60 {
            near[i] = -1
            group[i] = -1
        }
        self.group = group
        self.width = width
        self.height = height
        self.cell = cell
        self.screen = screen
        self.near = near
        self.far = far
        self.close = close
        self.light = scene.light
    }

    /// How much nearer or farther the next cell may be for a figure to step onto it.
    private static func rise(_ dx: Int, _ dy: Int) -> Float { dx != 0 && dy != 0 ? 0.07 : 0.05 }

    /// How near `near` is between the farthest and the nearest of the picture, 0 to 1.
    func nearness(_ near: Float) -> Float { min(max((near - far) / max(close - far, 0.01), 0), 1) }

    /// How tall a snowman stands, in points, as near as `near`: the farther off, the smaller.
    func height(near: Float) -> Float {
        (22 + 124 * pow(nearness(near), 1.35)) * screen.y / 1440
    }

    func walkable(_ cell: SIMD2<Int>) -> Bool {
        cell.x >= 0 && cell.x < width && cell.y >= 0 && cell.y < height && near[cell.y * width + cell.x] >= 0
    }

    /// The cell under a point, in the screen's points from its bottom left.
    func cell(at point: SIMD2<Float>) -> SIMD2<Int> {
        SIMD2(Int(point.x / cell), Int((screen.y - point.y) / cell))
    }

    /// The middle of a cell, in the screen's points from its bottom left.
    func point(of cell: SIMD2<Int>) -> SIMD2<Float> {
        SIMD2((Float(cell.x) + 0.5) * self.cell, screen.y - (Float(cell.y) + 0.5) * self.cell)
    }

    /// How near the ground is at a point, between the cells about it that can be walked.
    func near(at point: SIMD2<Float>) -> Float {
        let fx = point.x / cell - 0.5, fy = (screen.y - point.y) / cell - 0.5
        let x0 = Int(fx.rounded(.down)), y0 = Int(fy.rounded(.down))
        let ax = fx - Float(x0), ay = fy - Float(y0)
        var sum: Float = 0, weight: Float = 0
        for (dx, dy, w) in [(0, 0, (1 - ax) * (1 - ay)), (1, 0, ax * (1 - ay)), (0, 1, (1 - ax) * ay), (1, 1, ax * ay)] {
            let c = SIMD2(x0 + dx, y0 + dy)
            guard walkable(c), w > 0 else { continue }
            sum += near[c.y * width + c.x] * w
            weight += w
        }
        if weight > 0 { return sum / weight }
        let c = self.cell(at: point)
        return walkable(c) ? near[c.y * width + c.x] : far
    }

    /// The light snow takes on at a point, 0 to 1 a channel.
    func tint(at point: SIMD2<Float>) -> SIMD3<Float> {
        let pixels = light.pixels
        guard light.width > 0, light.height > 0, pixels.count >= light.width * light.height * 4 else { return SIMD3(repeating: 1) }
        let x = min(max(Int(point.x) / max(light.cell, 1), 0), light.width - 1)
        let y = min(max(Int(screen.y - point.y) / max(light.cell, 1), 0), light.height - 1)
        let i = (y * light.width + x) * 4
        return pixels.withUnsafeBytes { (bytes: UnsafeRawBufferPointer) -> SIMD3<Float> in
            SIMD3<Float>(Float(bytes[i]), Float(bytes[i + 1]), Float(bytes[i + 2])) / 255
        }
    }

    /// A cell to stand on, picked at random among those neither far off nor close if `middling`, and in the same
    /// stretch as `with` if given.
    func anywhere(using random: inout SystemRandomNumberGenerator, middling: Bool, with other: SIMD2<Int>? = nil) -> SIMD2<Int>? {
        let same = other.flatMap { walkable($0) ? group[$0.y * width + $0.x] : nil }
        let cells = near.indices.filter { i in
            guard near[i] >= 0, same == nil || group[i] == same else { return false }
            let n = nearness(near[i])
            return !middling || (n > 0.2 && n < 0.75)
        }
        guard let i = cells.randomElement(using: &random)
                ?? near.indices.filter({ near[$0] >= 0 && (same == nil || group[$0] == same) }).randomElement(using: &random) else { return nil }
        return SIMD2(i % width, i / width)
    }

    /// The way along the ground from one cell to another, its cells' middles in the screen's points; empty if there
    /// is none.
    func path(from start: SIMD2<Int>, to goal: SIMD2<Int>) -> [SIMD2<Float>] {
        guard walkable(start), walkable(goal), start != goal else { return [] }
        let count = width * height
        var cost = [Float](repeating: .infinity, count: count)
        var came = [Int32](repeating: -1, count: count)
        var open = Heap()
        func estimate(_ i: Int) -> Float {
            let dx = Float(i % width - goal.x), dy = Float(i / width - goal.y)
            return (dx * dx + dy * dy).squareRoot()
        }
        let first = start.y * width + start.x, last = goal.y * width + goal.x
        cost[first] = 0
        open.push(estimate(first), first)
        var visited = 0
        while let i = open.pop() {
            if i == last { break }
            visited += 1
            if visited > 6000 { return [] }
            let x = i % width, y = i / width
            for dy in -1...1 {
                for dx in -1...1 where dx != 0 || dy != 0 {
                    let c = SIMD2(x + dx, y + dy)
                    guard walkable(c) else { continue }
                    let j = c.y * width + c.x
                    guard abs(near[j] - near[i]) <= Self.rise(dx, dy) else { continue }
                    let step: Float = dx != 0 && dy != 0 ? 1.414 : 1
                    let through = cost[i] + step
                    if through < cost[j] {
                        cost[j] = through
                        came[j] = Int32(i)
                        open.push(through + estimate(j), j)
                    }
                }
            }
        }
        guard came[last] >= 0 else { return [] }
        var cells: [Int] = [last]
        var i = last
        while i != first, came[i] >= 0 {
            i = Int(came[i])
            cells.append(i)
        }
        return cells.reversed().dropFirst().map { point(of: SIMD2($0 % width, $0 / width)) }
    }
}

/// The cells still to look at, the most promising first.
private struct Heap {
    private var items: [(Float, Int)] = []

    mutating func push(_ key: Float, _ value: Int) {
        items.append((key, value))
        var i = items.count - 1
        while i > 0 {
            let parent = (i - 1) / 2
            guard items[i].0 < items[parent].0 else { break }
            items.swapAt(i, parent)
            i = parent
        }
    }

    mutating func pop() -> Int? {
        guard let top = items.first else { return nil }
        let last = items.removeLast()
        if !items.isEmpty {
            items[0] = last
            var i = 0
            while true {
                let left = 2 * i + 1, right = left + 1
                var smallest = i
                if left < items.count, items[left].0 < items[smallest].0 { smallest = left }
                if right < items.count, items[right].0 < items[smallest].0 { smallest = right }
                guard smallest != i else { break }
                items.swapAt(i, smallest)
                i = smallest
            }
        }
        return top.1
    }
}

/// A snowman's pictures, drawn once in white for its light to tint: its body with the left arm, the right arm that
/// throws, and the Santa hat that can be knocked off. Sizes and places are in parts of its height, from its feet.
enum SnowmanArt {
    struct Part {
        let image: CGImage
        /// What the image covers, in parts of the snowman's height: left, bottom, right, top.
        let rect: SIMD4<Float>
    }

    static let bodyRect = SIMD4<Float>(-0.5, -0.06, 0.5, 0.84)
    static let armRect = SIMD4<Float>(-0.03, -0.06, 0.34, 0.2)
    static let hatRect = SIMD4<Float>(-0.21, -0.05, 0.17, 0.27)
    /// The right shoulder, where the throwing arm turns.
    static let shoulder = SIMD2<Float>(0.125, 0.47)
    /// The hand at the end of the throwing arm, from the shoulder, the arm held straight out.
    static let reach = SIMD2<Float>(0.29, 0.1)
    /// Where the hat sits on the head.
    static let crown = SIMD2<Float>(-0.005, 0.745)
    /// The three balls: middle and radius.
    static let balls: [(SIMD2<Float>, Float)] = [(SIMD2(0, 0.19), 0.21), (SIMD2(0, 0.448), 0.155), (SIMD2(0, 0.66), 0.115)]

    static func parts(height: Float = 220, scale: CGFloat = 2) -> (body: Part, arm: Part, hat: Part)? {
        guard let body = draw(bodyRect, height: height, scale: scale, drawBody),
              let arm = draw(armRect, height: height, scale: scale, drawArm),
              let hat = draw(hatRect, height: height, scale: scale, drawHat) else { return nil }
        return (Part(image: body, rect: bodyRect), Part(image: arm, rect: armRect), Part(image: hat, rect: hatRect))
    }

    /// Draws into a picture covering `rect`, with the context's units set to the snowman's height and its origin at
    /// his feet.
    private static func draw(_ rect: SIMD4<Float>, height: Float, scale: CGFloat, _ body: (CGContext) -> Void) -> CGImage? {
        let width = Int((CGFloat(rect.z - rect.x) * CGFloat(height) * scale).rounded(.up))
        let tall = Int((CGFloat(rect.w - rect.y) * CGFloat(height) * scale).rounded(.up))
        guard let context = CGContext(data: nil, width: width, height: tall, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        let unit = CGFloat(height) * scale
        context.scaleBy(x: unit, y: unit)
        context.translateBy(x: CGFloat(-rect.x), y: CGFloat(-rect.y))
        context.setShouldAntialias(true)
        context.interpolationQuality = .high
        body(context)
        return context.makeImage()
    }

    private static func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
        CGColor(srgbRed: r, green: g, blue: b, alpha: a)
    }

    /// A ball of packed snow, lit from the upper left, bluer in its shade, its surface a little rough.
    private static func snowball(_ context: CGContext, center: CGPoint, radius: CGFloat, seed: UInt64) {
        let circle = CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
        context.saveGState()
        context.addEllipse(in: circle)
        context.clip()
        let colors = [color(1, 1, 1), color(0.95, 0.96, 0.99), color(0.8, 0.85, 0.94), color(0.62, 0.69, 0.83)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors, locations: [0, 0.35, 0.75, 1]) {
            let light = CGPoint(x: center.x - radius * 0.38, y: center.y + radius * 0.42)
            context.drawRadialGradient(gradient, startCenter: light, startRadius: 0, endCenter: CGPoint(x: center.x + radius * 0.1, y: center.y - radius * 0.1),
                                       endRadius: radius * 1.25, options: [.drawsAfterEndLocation])
        }
        // Packed snow is not smooth: small dents and crumbs catch the light and the shade.
        var state = seed
        func next() -> CGFloat {
            state = state &* 6364136223846793005 &+ 1442695040888963407
            return CGFloat(state >> 33) / CGFloat(UInt64(1) << 31)
        }
        for _ in 0..<160 {
            let angle = next() * 2 * .pi, out = next().squareRoot() * radius
            let p = CGPoint(x: center.x + cos(angle) * out, y: center.y + sin(angle) * out)
            let r = radius * (0.015 + next() * 0.035)
            let lit = next() > 0.5
            context.setFillColor(lit ? color(1, 1, 1, 0.35) : color(0.55, 0.62, 0.78, 0.18))
            context.fillEllipse(in: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 1.6))
        }
        // The rim against what is behind it is a touch lighter, as snow scatters light at its edge.
        context.setStrokeColor(color(0.9, 0.93, 1, 0.35))
        context.setLineWidth(radius * 0.05)
        context.strokeEllipse(in: circle.insetBy(dx: radius * 0.025, dy: radius * 0.025))
        context.restoreGState()
    }

    private static func drawBody(_ context: CGContext) {
        // His shadow on the snow he stands on.
        context.saveGState()
        let shade = [color(0.1, 0.14, 0.25, 0.32), color(0.1, 0.14, 0.25, 0)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: shade, locations: [0, 1]) {
            context.translateBy(x: 0.04, y: 0.005)
            context.scaleBy(x: 1, y: 0.22)
            context.drawRadialGradient(gradient, startCenter: .zero, startRadius: 0, endCenter: .zero, endRadius: 0.27, options: [])
        }
        context.restoreGState()
        // The left arm goes behind the middle ball.
        twig(context, from: CGPoint(x: -0.12, y: 0.475), to: CGPoint(x: -0.39, y: 0.6), width: 0.013)
        for (k, (middle, radius)) in balls.enumerated() {
            if k > 0 {
                // Where a ball sits on the one below, it shades it.
                let below = balls[k - 1]
                context.saveGState()
                context.addEllipse(in: CGRect(x: CGFloat(below.0.x - below.1), y: CGFloat(below.0.y - below.1), width: CGFloat(below.1 * 2), height: CGFloat(below.1 * 2)))
                context.clip()
                let shade = [color(0.35, 0.42, 0.6, 0.35), color(0.35, 0.42, 0.6, 0)] as CFArray
                if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: shade, locations: [0, 1]) {
                    let c = CGPoint(x: CGFloat(middle.x), y: CGFloat(middle.y - radius * 0.85))
                    context.drawRadialGradient(gradient, startCenter: c, startRadius: CGFloat(radius * 0.6), endCenter: c, endRadius: CGFloat(radius * 1.15), options: [])
                }
                context.restoreGState()
            }
            snowball(context, center: CGPoint(x: CGFloat(middle.x), y: CGFloat(middle.y)), radius: CGFloat(radius), seed: UInt64(k + 3))
        }
        // Coal buttons down the middle ball.
        for k in 0..<3 {
            let y = CGFloat(balls[1].0.y) + 0.07 - CGFloat(k) * 0.058
            coal(context, at: CGPoint(x: 0.004 * CGFloat(k), y: y), radius: 0.016)
        }
        scarf(context)
        // The face: coal eyes and a smile, and a carrot.
        let head = CGPoint(x: CGFloat(balls[2].0.x), y: CGFloat(balls[2].0.y))
        coal(context, at: CGPoint(x: head.x - 0.038, y: head.y + 0.03), radius: 0.014)
        coal(context, at: CGPoint(x: head.x + 0.04, y: head.y + 0.032), radius: 0.014)
        for k in 0..<5 {
            let angle = CGFloat.pi * (1.25 + 0.125 * CGFloat(k))
            coal(context, at: CGPoint(x: head.x + cos(angle) * 0.058, y: head.y - 0.005 + sin(angle) * 0.045), radius: 0.008)
        }
        carrot(context, from: CGPoint(x: head.x + 0.005, y: head.y + 0.005))
    }

    private static func coal(_ context: CGContext, at point: CGPoint, radius: CGFloat) {
        context.setFillColor(color(0.07, 0.07, 0.08))
        context.fillEllipse(in: CGRect(x: point.x - radius, y: point.y - radius * 0.9, width: radius * 2, height: radius * 1.8))
        context.setFillColor(color(0.6, 0.62, 0.7, 0.55))
        context.fillEllipse(in: CGRect(x: point.x - radius * 0.45, y: point.y + radius * 0.2, width: radius * 0.5, height: radius * 0.4))
    }

    private static func carrot(_ context: CGContext, from base: CGPoint) {
        let tip = CGPoint(x: base.x + 0.115, y: base.y - 0.022)
        let path = CGMutablePath()
        path.move(to: CGPoint(x: base.x - 0.004, y: base.y + 0.017))
        path.addQuadCurve(to: tip, control: CGPoint(x: base.x + 0.06, y: base.y + 0.012))
        path.addQuadCurve(to: CGPoint(x: base.x - 0.002, y: base.y - 0.016), control: CGPoint(x: base.x + 0.05, y: base.y - 0.022))
        path.closeSubpath()
        context.saveGState()
        context.addPath(path)
        context.clip()
        let colors = [color(1, 0.62, 0.22), color(0.93, 0.42, 0.08), color(0.7, 0.27, 0.05)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors, locations: [0, 0.5, 1]) {
            context.drawLinearGradient(gradient, start: CGPoint(x: base.x, y: base.y + 0.02), end: CGPoint(x: base.x, y: base.y - 0.02), options: [])
        }
        context.setStrokeColor(color(0.55, 0.22, 0.05, 0.6))
        context.setLineWidth(0.003)
        for k in 1...3 {
            let x = base.x + 0.022 * CGFloat(k)
            context.move(to: CGPoint(x: x, y: base.y + 0.014 - 0.002 * CGFloat(k)))
            context.addLine(to: CGPoint(x: x + 0.004, y: base.y + 0.004 - 0.002 * CGFloat(k)))
        }
        context.strokePath()
        context.restoreGState()
    }

    private static func scarf(_ context: CGContext) {
        let neck = CGFloat(balls[1].0.y + balls[1].1 * 0.86)
        let red = color(0.78, 0.1, 0.12), dark = color(0.52, 0.05, 0.08), stripe = color(0.96, 0.94, 0.9)
        // The tail hanging down his front, a little to his left.
        let tail = CGMutablePath()
        tail.move(to: CGPoint(x: -0.06, y: neck))
        tail.addLine(to: CGPoint(x: -0.1, y: neck - 0.13))
        tail.addLine(to: CGPoint(x: -0.05, y: neck - 0.14))
        tail.addLine(to: CGPoint(x: -0.02, y: neck - 0.01))
        tail.closeSubpath()
        context.setFillColor(dark)
        context.addPath(tail)
        context.fillPath()
        context.saveGState()
        context.addPath(tail)
        context.clip()
        context.setFillColor(stripe)
        for k in 0..<2 {
            let y = neck - 0.06 - CGFloat(k) * 0.04
            context.fill(CGRect(x: -0.12, y: y, width: 0.12, height: 0.012))
        }
        context.restoreGState()
        // Wound round the neck.
        let band = CGRect(x: -0.12, y: neck - 0.022, width: 0.24, height: 0.05)
        context.setFillColor(red)
        context.fillEllipse(in: band)
        context.saveGState()
        context.addEllipse(in: band)
        context.clip()
        context.setFillColor(stripe)
        for k in 0..<4 {
            context.fill(CGRect(x: -0.1 + CGFloat(k) * 0.06, y: neck - 0.03, width: 0.012, height: 0.07))
        }
        let shade = [color(0, 0, 0, 0), color(0, 0, 0, 0.3)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: shade, locations: [0, 1]) {
            context.drawLinearGradient(gradient, start: CGPoint(x: 0, y: neck + 0.03), end: CGPoint(x: 0, y: neck - 0.025), options: [])
        }
        context.restoreGState()
    }

    /// A bare twig with a fork or two.
    private static func twig(_ context: CGContext, from start: CGPoint, to end: CGPoint, width: CGFloat) {
        context.saveGState()
        context.setStrokeColor(color(0.33, 0.21, 0.12))
        context.setLineCap(.round)
        context.setLineWidth(width)
        let mid = CGPoint(x: (start.x + end.x) / 2 + (end.y - start.y) * 0.06, y: (start.y + end.y) / 2 - (end.x - start.x) * 0.06)
        context.move(to: start)
        context.addQuadCurve(to: end, control: mid)
        context.strokePath()
        context.setLineWidth(width * 0.6)
        let along = CGPoint(x: end.x - start.x, y: end.y - start.y)
        let fork = CGPoint(x: start.x + along.x * 0.7, y: start.y + along.y * 0.7)
        context.move(to: fork)
        context.addLine(to: CGPoint(x: fork.x + along.x * 0.18 - along.y * 0.25, y: fork.y + along.y * 0.18 + along.x * 0.25))
        context.move(to: end)
        context.addLine(to: CGPoint(x: end.x + along.x * 0.1 + along.y * 0.16, y: end.y + along.y * 0.1 - along.x * 0.16))
        context.move(to: end)
        context.addLine(to: CGPoint(x: end.x + along.x * 0.14 - along.y * 0.1, y: end.y + along.y * 0.14 + along.x * 0.1))
        context.strokePath()
        context.restoreGState()
    }

    private static func drawArm(_ context: CGContext) {
        twig(context, from: .zero, to: CGPoint(x: CGFloat(reach.x), y: CGFloat(reach.y)), width: 0.013)
    }

    private static func drawHat(_ context: CGContext) {
        // The red cone flops over to his right, the pompom at its tip.
        let cone = CGMutablePath()
        cone.move(to: CGPoint(x: -0.1, y: 0.015))
        cone.addCurve(to: CGPoint(x: -0.14, y: 0.205), control1: CGPoint(x: -0.095, y: 0.11), control2: CGPoint(x: -0.06, y: 0.21))
        cone.addCurve(to: CGPoint(x: 0.1, y: 0.015), control1: CGPoint(x: -0.02, y: 0.25), control2: CGPoint(x: 0.09, y: 0.16))
        cone.closeSubpath()
        context.saveGState()
        context.addPath(cone)
        context.clip()
        let colors = [color(0.9, 0.18, 0.18), color(0.72, 0.08, 0.1), color(0.48, 0.04, 0.07)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors, locations: [0, 0.55, 1]) {
            context.drawLinearGradient(gradient, start: CGPoint(x: -0.08, y: 0.16), end: CGPoint(x: 0.1, y: 0.0), options: [])
        }
        context.restoreGState()
        // The white fur brim and the pompom.
        fur(context, in: CGRect(x: -0.125, y: -0.025, width: 0.25, height: 0.06), seed: 11)
        fur(context, in: CGRect(x: -0.185, y: 0.165, width: 0.07, height: 0.07), seed: 17)
    }

    private static func fur(_ context: CGContext, in rect: CGRect, seed: UInt64) {
        context.setFillColor(color(0.93, 0.93, 0.95))
        context.fillEllipse(in: rect)
        var state = seed
        func next() -> CGFloat {
            state = state &* 6364136223846793005 &+ 1442695040888963407
            return CGFloat(state >> 33) / CGFloat(UInt64(1) << 31)
        }
        context.saveGState()
        context.addEllipse(in: rect)
        context.clip()
        for _ in 0..<60 {
            let p = CGPoint(x: rect.minX + next() * rect.width, y: rect.minY + next() * rect.height)
            let r = min(rect.width, rect.height) * (0.08 + next() * 0.12)
            let lit = p.y > rect.midY + (next() - 0.5) * rect.height * 0.4
            context.setFillColor(lit ? color(1, 1, 1, 0.8) : color(0.7, 0.74, 0.82, 0.5))
            context.fillEllipse(in: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 2))
        }
        context.restoreGState()
    }
}

/// A snowman who lives in the wallpaper's picture: he wanders over its ground, the farther off the smaller, now and then
/// throws a snowball out of the picture at the screen, and reels when one thrown at him hits him.
final class SnowmanBot {
    private(set) var ground: SnowmanGround
    /// Where his feet are, in the screen's points from its bottom left.
    private(set) var foot: SIMD2<Float>
    private var path: [SIMD2<Float>] = []
    private(set) var facing: Float = 1
    private var stride: Float = 0
    private var restUntil: Double = 0
    private var nextThrow: Double
    private var random = SystemRandomNumberGenerator()

    private enum Act {
        case idle
        case windingUp(Float)
        case throwing(Float)
        case reeling(Float)
    }
    private var act = Act.idle
    private var armAngle: Float = -0.45
    private var lean: Float = 0
    private var bounce: Float = 0

    private enum Hat {
        case worn(grow: Float)
        case flying(position: SIMD2<Float>, velocity: SIMD2<Float>, turn: Float, spin: Float)
        case lying(position: SIMD2<Float>, turn: Float, since: Float)
    }
    private var hat = Hat.worn(grow: 1)
    private var hatFade: Float = 1

    /// One standing at `foot`.
    init(ground: SnowmanGround, foot: SIMD2<Float>, now: Double) {
        self.ground = ground
        self.foot = foot
        nextThrow = now + Double.random(in: 4...8)
        restUntil = now + 1.5
    }

    init?(ground: SnowmanGround, now: Double) {
        var random = SystemRandomNumberGenerator()
        guard let start = ground.anywhere(using: &random, middling: true) else { return nil }
        self.ground = ground
        foot = ground.point(of: start)
        nextThrow = now + Double.random(in: 4...8)
        restUntil = now + 1.5
    }

    /// His height where he stands, in points.
    var height: Float { ground.height(near: near) }
    /// How near he is in the picture.
    var near: Float { ground.near(at: foot) }
    /// The light he stands in.
    var tint: SIMD3<Float> { ground.tint(at: foot) }

    /// The bounds of all he draws, his hat flying off included, in the screen's points: left, bottom, right, top.
    var reach: SIMD4<Float> {
        let h = height
        return SIMD4(foot.x - 0.8 * h, foot.y - 0.35 * h, foot.x + 0.8 * h, foot.y + 1.25 * h)
    }

    /// Each part's place: what its picture covers, in parts of his height, turned and moved onto the screen as
    /// `matrix` times that, plus `offset`, in points; and how much of it shows.
    struct Placement {
        var matrix: simd_float2x2
        var offset: SIMD2<Float>
        var alpha: Float
    }

    private func rotation(_ angle: Float) -> simd_float2x2 {
        simd_float2x2(SIMD2(cos(angle), sin(angle)), SIMD2(-sin(angle), cos(angle)))
    }

    /// His body, his throwing arm, and his hat.
    var placements: (body: Placement, arm: Placement, hat: Placement) {
        let h = height
        let mirror = simd_float2x2(SIMD2(facing, 0), SIMD2(0, 1))
        let body = rotation(lean * facing) * mirror * h
        let base = foot + SIMD2(0, bounce * h)
        let shoulder = base + body * SnowmanArt.shoulder
        let arm = rotation(lean * facing) * mirror * rotation(armAngle) * h
        let hatPlacement: Placement
        switch hat {
        case let .worn(grow):
            let m = rotation(lean * facing) * mirror * rotation(0.08) * (h * grow)
            hatPlacement = Placement(matrix: m, offset: base + body * SnowmanArt.crown, alpha: 1)
        case let .flying(position, _, turn, _):
            hatPlacement = Placement(matrix: rotation(turn) * mirror * h, offset: position, alpha: 1)
        case let .lying(position, turn, _):
            hatPlacement = Placement(matrix: rotation(turn) * mirror * h, offset: position, alpha: hatFade)
        }
        return (Placement(matrix: body, offset: base, alpha: 1), Placement(matrix: arm, offset: shoulder, alpha: 1), hatPlacement)
    }

    /// His throwing arm is raised behind him, winding up.
    var armBehind: Bool {
        if case .windingUp = act { return true }
        return false
    }

    /// The tip of his throwing arm, in points.
    var hand: SIMD2<Float> {
        let arm = placements.arm
        return arm.offset + arm.matrix * SnowmanArt.reach
    }

    /// Whether a point is on him.
    func contains(_ point: SIMD2<Float>) -> Bool {
        let body = placements.body
        let inverse = body.matrix.inverse
        let local = inverse * (point - body.offset)
        let onHat: Bool
        if case .worn = hat { onHat = abs(local.x) < 0.13 && local.y > 0.72 && local.y < 0.95 } else { onHat = false }
        return onHat || SnowmanArt.balls.contains { simd_distance(local, $0.0) < $0.1 * 1.1 }
    }

    /// Moves him on by `dt`. `canThrow` says whether he can be seen to throw now; `throwBall` sends a snowball from
    /// his hand, as big across as it is there. Returns whether anything of him moved.
    func step(dt: Float, now: Double, canThrow: Bool, throwBall: (SIMD2<Float>, Float) -> Void) -> Bool {
        var moved = stepHat(dt: dt)
        switch act {
        case .idle:
            if canThrow, now >= nextThrow, path.isEmpty || now >= nextThrow + 4 {
                act = .windingUp(0)
                path = []
                return true
            }
            if !path.isEmpty {
                walk(dt: dt)
                moved = true
            } else {
                if lean != 0 || bounce != 0 || armAngle != -0.45 {
                    lean *= max(0, 1 - dt * 10)
                    bounce *= max(0, 1 - dt * 10)
                    armAngle += (-0.45 - armAngle) * min(1, dt * 8)
                    if abs(lean) < 0.001 { lean = 0 }
                    if bounce < 0.0005 { bounce = 0 }
                    if abs(armAngle + 0.45) < 0.002 { armAngle = -0.45 }
                    moved = true
                }
                if now >= restUntil { wander(now: now) }
            }
        case let .windingUp(t):
            let t = t + dt / 0.5
            let ease = t * t * (3 - 2 * t)
            armAngle = -0.45 + (2.35 + 0.45) * min(ease, 1)
            lean = -0.07 * min(ease, 1)
            act = t >= 1 ? .throwing(0) : .windingUp(t)
            moved = true
        case let .throwing(t):
            if t == 0 {
                throwBall(hand, height * 0.1)
                nextThrow = now + Double.random(in: 7...14, using: &random)
            }
            let t = t + dt / 0.16
            armAngle = 2.35 - (2.35 + 1.0) * min(t, 1)
            lean = -0.07 + 0.17 * min(t, 1)
            act = t >= 1 ? .idle : .throwing(t)
            if t >= 1 { restUntil = now + Double.random(in: 0.8...2, using: &random) }
            moved = true
        case let .reeling(t):
            let t = t + dt
            lean = 0.28 * sin(t * 17) * exp(-t * 3.6)
            armAngle = -0.45 + 0.9 * sin(t * 11) * exp(-t * 3)
            if t >= 1.1 {
                act = .idle
                restUntil = now + 0.4
            } else {
                act = .reeling(t)
            }
            moved = true
        }
        return moved
    }

    private func wander(now: Double) {
        let start = ground.cell(at: foot)
        for _ in 0..<6 {
            guard let goal = ground.anywhere(using: &random, middling: false, with: start) else { return }
            let distance = simd_length(SIMD2<Float>(Float(goal.x - start.x), Float(goal.y - start.y)))
            guard distance > 12, distance < 90 else { continue }
            let found = ground.path(from: start, to: goal)
            if !found.isEmpty {
                path = found
                return
            }
        }
        restUntil = now + 2
    }

    private func walk(dt: Float) {
        guard let next = path.first else { return }
        let h = height
        // Seen in perspective, he covers less of the screen the farther off he is.
        let speed = 0.42 * h
        let way = next - foot
        let distance = simd_length(way)
        if abs(way.x) > 1 { facing = way.x > 0 ? 1 : -1 }
        if distance <= speed * dt {
            foot = next
            path.removeFirst()
            if path.isEmpty { restUntil = CACurrentMediaTime() + Double.random(in: 1.5...5, using: &random) }
        } else {
            foot += way / distance * speed * dt
        }
        // He waddles: rocks from side to side and bobs with each step.
        stride += dt * 9
        lean = sin(stride) * 0.06
        bounce = abs(sin(stride)) * 0.022
    }

    /// Hit by a snowball: he reels, and his hat flies off.
    func hit(at point: SIMD2<Float>, now: Double) {
        act = .reeling(0)
        path = []
        nextThrow = min(nextThrow, now + Double.random(in: 1.4...2.4, using: &random))
        if case .worn = hat {
            let place = placements.hat
            let h = height
            let away: Float = point.x < foot.x ? 1 : -1
            hat = .flying(position: place.offset, velocity: SIMD2(away * Float.random(in: 0.5...1.0, using: &random), Float.random(in: 1.4...2.0, using: &random)) * h,
                          turn: 0.08 * facing, spin: away * -Float.random(in: 5...9, using: &random))
        }
    }

    private func stepHat(dt: Float) -> Bool {
        let h = height
        switch hat {
        case let .worn(grow):
            guard grow < 1 else { return false }
            hat = .worn(grow: min(1, grow + dt / 0.35))
            return true
        case var .flying(position, velocity, turn, spin):
            velocity.y -= 5.5 * h * dt
            position += velocity * dt
            turn += spin * dt
            let floor = foot.y + 0.01 * h
            if position.y <= floor, velocity.y < 0 {
                position.y = floor
                hat = .lying(position: position, turn: turn.truncatingRemainder(dividingBy: 2 * .pi) * 0.3 + (spin > 0 ? 0.4 : -0.4), since: 0)
            } else {
                hat = .flying(position: position, velocity: velocity, turn: turn, spin: spin)
            }
            return true
        case let .lying(position, turn, since):
            let since = since + dt
            hatFade = since < 3.5 ? 1 : max(0, 1 - (since - 3.5) / 0.6)
            if since >= 4.1 {
                hatFade = 1
                hat = .worn(grow: 0)
            } else {
                hat = .lying(position: position, turn: turn, since: since)
            }
            return since >= 3.5
        }
    }
}
