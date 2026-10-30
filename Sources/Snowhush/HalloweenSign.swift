import AppKit

/// The orange signs Halloween's ghosts hold up: a ragged board on a stick, its caption dripping.
enum HalloweenSign {
    static let captions = [
        "BOO!", "SCARE ME!", "I DARE YOU", "DON'T TOUCH", "SNOW WAY!", "CHILL OUT",
        "FREEZE!", "BRRR-OO!", "NO PEEKING", "NOT A SNOWMAN", "BACK TO WORK", "TRICK OR TREAT",
    ]
    /// Pixels per point the signs are drawn at.
    static let scale: CGFloat = 2
    static let stickLength: CGFloat = 20

    /// The sign for `caption`, drawn into premultiplied BGRA, its stick's foot at the middle of the bottom edge.
    static func draw(_ caption: String) -> CGContext? {
        var random = SeededRandom(seed: caption.unicodeScalars.reduce(5381) { ($0 &* 33) &+ UInt64($1.value) })
        let font = NSFont(name: "Impact", size: 13) ?? .boldSystemFont(ofSize: 13)
        let style = NSMutableParagraphStyle()
        style.alignment = .center
        style.lineSpacing = -2
        let text = NSAttributedString(string: caption, attributes: [
            .font: font, .foregroundColor: NSColor(red: 0.1, green: 0.05, blue: 0.04, alpha: 1), .paragraphStyle: style,
        ])
        let textSize = text.boundingRect(with: NSSize(width: 200, height: 200), options: [.usesLineFragmentOrigin]).size
        let board = CGSize(width: ceil(textSize.width) + 12, height: ceil(textSize.height) + 8)
        let size = CGSize(width: board.width + 8, height: board.height + 8 + stickLength)
        let width = Int(size.width * scale), height = Int(size.height * scale)
        guard let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else { return nil }
        context.scaleBy(x: scale, y: scale)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
        defer { NSGraphicsContext.restoreGraphicsState() }

        let stick = NSBezierPath(roundedRect: NSRect(x: size.width / 2 - 2.5, y: 0, width: 5, height: stickLength + 10), xRadius: 1.5, yRadius: 1.5)
        NSColor(red: 0.82, green: 0.38, blue: 0.1, alpha: 1).setFill()
        stick.fill()
        NSColor(red: 0.55, green: 0.22, blue: 0.06, alpha: 1).setStroke()
        stick.lineWidth = 0.8
        stick.stroke()

        // A board with a ragged, spiky edge: short steps along each side, nudged in and out.
        let rect = CGRect(x: 4, y: stickLength + 4, width: board.width, height: board.height)
        let outline = NSBezierPath()
        let corners = [CGPoint(x: rect.minX, y: rect.minY), CGPoint(x: rect.maxX, y: rect.minY),
                       CGPoint(x: rect.maxX, y: rect.maxY), CGPoint(x: rect.minX, y: rect.maxY)]
        for side in 0..<4 {
            let from = corners[side], to = corners[(side + 1) % 4]
            let length = hypot(to.x - from.x, to.y - from.y)
            let outward = CGPoint(x: (to.y - from.y) / length, y: -(to.x - from.x) / length)
            var travelled: CGFloat = 0
            while travelled < length {
                let t = travelled / length
                let spike = random.next() < 0.3 ? random.range(1.5, 3) : random.range(-0.8, 0.8)
                let point = CGPoint(x: from.x + (to.x - from.x) * t + outward.x * spike, y: from.y + (to.y - from.y) * t + outward.y * spike)
                if side == 0 && travelled == 0 { outline.move(to: point) } else { outline.line(to: point) }
                travelled += random.range(3, 6)
            }
        }
        outline.close()
        NSColor(red: 0.95, green: 0.47, blue: 0.13, alpha: 1).setFill()
        outline.fill()
        NSGraphicsContext.saveGraphicsState()
        outline.addClip()
        NSGradient(colors: [NSColor(red: 1, green: 0.62, blue: 0.3, alpha: 0.55), NSColor(red: 0.95, green: 0.47, blue: 0.13, alpha: 0)])?
            .draw(in: NSRect(x: rect.minX, y: rect.midY, width: rect.width * 0.7, height: rect.height / 2 + 4), angle: -30)
        NSGraphicsContext.restoreGraphicsState()
        NSColor(red: 0.7, green: 0.3, blue: 0.08, alpha: 1).setStroke()
        outline.lineWidth = 1
        outline.stroke()

        let textRect = NSRect(x: rect.midX - textSize.width / 2, y: rect.midY - textSize.height / 2 + 0.5, width: textSize.width, height: textSize.height)
        text.draw(with: textRect, options: [.usesLineFragmentOrigin])
        drip(below: text, in: textRect, context: context, random: &random)
        return context
    }

    /// Paint running down from the letters' feet: found by drawing the caption alone and looking for its lowest ink.
    private static func drip(below text: NSAttributedString, in rect: NSRect, context: CGContext, random: inout SeededRandom) {
        let width = Int(rect.width * scale), height = Int(rect.height * scale)
        guard width > 0, height > 0, let mask = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width,
            space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue
        ) else { return }
        mask.scaleBy(x: scale, y: scale)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: mask, flipped: false)
        let white = NSMutableAttributedString(attributedString: text)
        white.addAttribute(.foregroundColor, value: NSColor.white, range: NSRange(location: 0, length: white.length))
        white.draw(with: NSRect(origin: .zero, size: rect.size), options: [.usesLineFragmentOrigin])
        NSGraphicsContext.restoreGraphicsState()
        guard let data = mask.data?.assumingMemoryBound(to: UInt8.self) else { return }

        NSColor(red: 0.1, green: 0.05, blue: 0.04, alpha: 1).setFill()
        var column = Int(random.range(2, 5) * scale)
        while column < width {
            // Rows run top-down in memory: the first inked row from the bottom of each line is a letter's foot.
            var feet: [Int] = []
            var inked = false
            for row in (0..<height).reversed() {
                let ink = data[row * width + column] > 128
                if ink && !inked { feet.append(row) }
                inked = ink
            }
            for row in feet where random.next() < 0.35 {
                let x = rect.minX + CGFloat(column) / scale
                let y = rect.maxY - CGFloat(row) / scale
                let length = random.range(1, 3)
                let thickness = random.range(0.9, 1.4)
                NSBezierPath(roundedRect: NSRect(x: x - thickness / 2, y: y - length, width: thickness, height: length + 0.5), xRadius: thickness / 2, yRadius: thickness / 2).fill()
                NSBezierPath(ovalIn: NSRect(x: x - thickness * 0.65, y: y - length - thickness * 0.4, width: thickness * 1.3, height: thickness * 1.3)).fill()
            }
            column += Int(random.range(3, 7) * scale)
        }
    }
}

/// The same scatter of spikes and drips for a caption every time it is drawn.
private struct SeededRandom {
    private var state: UInt64

    init(seed: UInt64) {
        state = seed | 1
    }

    mutating func next() -> CGFloat {
        state ^= state << 13
        state ^= state >> 7
        state ^= state << 17
        return CGFloat(state >> 40) / CGFloat(1 << 24)
    }

    mutating func range(_ lower: CGFloat, _ upper: CGFloat) -> CGFloat {
        lower + (upper - lower) * next()
    }
}
