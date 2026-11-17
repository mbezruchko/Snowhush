import Accelerate
import CoreML
import CoreVideo
import Foundation
import simd

/// The wallpaper's geometry beyond what one look of the depth network gives: finer depth from looking at it part by
/// part, its edges moved onto the picture's own, and which way its surfaces face.
extension Wallpaper {
    /// The depth network, loaded for one wallpaper and let go after.
    final class Network {
        private let model: MLModel
        private let output: String
        let width: Int
        let height: Int

        init?(url: URL) {
            let configuration = MLModelConfiguration()
            configuration.computeUnits = .cpuAndNeuralEngine
            guard let model = try? MLModel(contentsOf: url, configuration: configuration),
                  let constraint = model.modelDescription.inputDescriptionsByName["image"]?.imageConstraint,
                  let output = model.modelDescription.outputDescriptionsByName.keys.first else { return nil }
            self.model = model
            self.output = output
            width = constraint.pixelsWide
            height = constraint.pixelsHigh
        }

        /// The depth of a part of a picture as the network gives it, `width` by `height`: the larger, the nearer, on a
        /// scale of its own each time.
        func depth(of image: CGImage, part: CGRect) -> [Float]? {
            guard let piece = image.cropping(to: part) else { return nil }
            var input: CVPixelBuffer?
            guard CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_32BGRA, nil, &input) == kCVReturnSuccess, let input else { return nil }
            CVPixelBufferLockBaseAddress(input, [])
            let context = CGContext(data: CVPixelBufferGetBaseAddress(input), width: width, height: height, bitsPerComponent: 8,
                                    bytesPerRow: CVPixelBufferGetBytesPerRow(input), space: CGColorSpaceCreateDeviceRGB(),
                                    bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
            context?.interpolationQuality = .high
            context?.draw(piece, in: CGRect(x: 0, y: 0, width: width, height: height))
            CVPixelBufferUnlockBaseAddress(input, [])
            guard context != nil,
                  let features = try? MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: input)]),
                  let result = try? model.prediction(from: features),
                  let depth = result.featureValue(for: output)?.imageBufferValue else { return nil }
            return Self.floats(of: depth)
        }

        /// Half floats read through vImage: Swift's Float16 isn't there on Intel Macs.
        private static func floats(of buffer: CVPixelBuffer) -> [Float]? {
            CVPixelBufferLockBaseAddress(buffer, .readOnly)
            defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
            let width = CVPixelBufferGetWidth(buffer), height = CVPixelBufferGetHeight(buffer)
            guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
            var values = [Float](repeating: 0, count: width * height)
            let format = CVPixelBufferGetPixelFormatType(buffer)
            if format == kCVPixelFormatType_OneComponent32Float {
                for y in 0..<height {
                    let row = (base + y * CVPixelBufferGetBytesPerRow(buffer)).assumingMemoryBound(to: Float.self)
                    for x in 0..<width { values[y * width + x] = row[x] }
                }
                return values
            }
            guard format == kCVPixelFormatType_OneComponent16Half else { return nil }
            values.withUnsafeMutableBytes { out in
                var source = vImage_Buffer(data: base, height: vImagePixelCount(height), width: vImagePixelCount(width),
                                           rowBytes: CVPixelBufferGetBytesPerRow(buffer))
                var target = vImage_Buffer(data: out.baseAddress, height: vImagePixelCount(height), width: vImagePixelCount(width), rowBytes: width * 4)
                vImageConvert_Planar16FtoPlanarF(&source, &target, 0)
            }
            return values
        }
    }

    /// A grid of values over the screen, top row first.
    struct Grid {
        var width: Int
        var height: Int
        var values: [Float]

        init(width: Int, height: Int, values: [Float]) {
            self.width = width
            self.height = height
            self.values = values
        }

        init(width: Int, height: Int, _ value: (_ x: Int, _ y: Int) -> Float) {
            self.width = width
            self.height = height
            values = [Float](repeating: 0, count: width * height)
            for y in 0..<height {
                for x in 0..<width { values[y * width + x] = value(x, y) }
            }
        }

        /// Bilinear, at a point given in cells from the top left, with cell centres at halves.
        func value(atX x: Float, y: Float) -> Float {
            let u = min(max(x - 0.5, 0), Float(width - 1)), v = min(max(y - 0.5, 0), Float(height - 1))
            let x0 = Int(u), y0 = Int(v)
            let x1 = min(x0 + 1, width - 1), y1 = min(y0 + 1, height - 1)
            let fx = u - Float(x0), fy = v - Float(y0)
            let top = values[y0 * width + x0] * (1 - fx) + values[y0 * width + x1] * fx
            let bottom = values[y1 * width + x0] * (1 - fx) + values[y1 * width + x1] * fx
            return top * (1 - fy) + bottom * fy
        }

        /// The mean over a square `2 * radius + 1` cells across around each cell, by running sums.
        func blurred(radius: Int) -> Grid {
            guard radius > 0 else { return self }
            var across = [Float](repeating: 0, count: values.count)
            for y in 0..<height {
                var sum: Float = 0
                var count = 0
                let row = y * width
                for x in 0..<min(radius, width) { sum += values[row + x]; count += 1 }
                for x in 0..<width {
                    if x + radius < width { sum += values[row + x + radius]; count += 1 }
                    if x - radius - 1 >= 0 { sum -= values[row + x - radius - 1]; count -= 1 }
                    across[row + x] = sum / Float(count)
                }
            }
            var down = [Float](repeating: 0, count: values.count)
            var sums = [Float](repeating: 0, count: width)
            var count = 0
            for y in 0..<min(radius, height) {
                for x in 0..<width { sums[x] += across[y * width + x] }
                count += 1
            }
            for y in 0..<height {
                if y + radius < height {
                    let row = (y + radius) * width
                    for x in 0..<width { sums[x] += across[row + x] }
                    count += 1
                }
                if y - radius - 1 >= 0 {
                    let row = (y - radius - 1) * width
                    for x in 0..<width { sums[x] -= across[row + x] }
                    count -= 1
                }
                let scale = 1 / Float(count)
                let row = y * width
                for x in 0..<width { down[row + x] = sums[x] * scale }
            }
            return Grid(width: width, height: height, values: down)
        }
    }

    /// The network's look at the whole screen, and finer depth made of it and of looks at the screen part by part: a
    /// cell to every two points. Each part is seen in more detail than the whole but on a scale of its own, so it is
    /// fitted to the whole and lends it only its detail; in the sky, where there is none to lend, it lends nothing.
    static func depths(of shown: Shown, network: Network, parts: (across: Int, down: Int) = (3, 2)) -> (whole: DepthMap, fine: Grid)? {
        guard let image = shown.image(),
              let raw = network.depth(of: image, part: CGRect(x: 0, y: 0, width: shown.width, height: shown.height)) else { return nil }
        let low = raw.min() ?? 0, high = raw.max() ?? 1
        let range = max(high - low, 1e-6)
        let whole = Grid(width: network.width, height: network.height, values: raw.map { ($0 - low) / range })
        let cell: Float = 2
        let width = shown.width / Int(cell), height = shown.height / Int(cell)
        // The whole, spread over the fine grid, and its smooth part: what is left of it once the detail of about the
        // size the parts see better is taken out.
        let wholeAcross = Float(network.width) / Float(shown.width), wholeDown = Float(network.height) / Float(shown.height)
        let spread = Grid(width: width, height: height) { x, y in
            whole.value(atX: (Float(x) + 0.5) * cell * wholeAcross, y: (Float(y) + 0.5) * cell * wholeDown)
        }
        let radius = 6
        let smooth = spread.blurred(radius: radius)
        var detail = [Float](repeating: 0, count: width * height)
        var weight = [Float](repeating: 0, count: width * height)
        // Parts overlapping by a quarter.
        let partWidth = Float(shown.width) / (Float(parts.across) - 0.25 * Float(parts.across - 1))
        let partHeight = Float(shown.height) / (Float(parts.down) - 0.25 * Float(parts.down - 1))
        for row in 0..<parts.down {
            for column in 0..<parts.across {
                let left = (Float(column) * partWidth * 0.75).rounded(), top = (Float(row) * partHeight * 0.75).rounded()
                let part = CGRect(x: CGFloat(left), y: CGFloat(top), width: CGFloat(min(partWidth.rounded(), Float(shown.width) - left)),
                                  height: CGFloat(min(partHeight.rounded(), Float(shown.height) - top)))
                guard let seen = network.depth(of: image, part: part) else { continue }
                // Fitted to the whole by least squares: seen * a + b.
                var sums = (s: Double(0), w: Double(0), ss: Double(0), sw: Double(0))
                let across = Float(part.width) / Float(network.width), down = Float(part.height) / Float(network.height)
                var matching = [Float](repeating: 0, count: seen.count)
                for v in 0..<network.height {
                    for u in 0..<network.width {
                        let x = left + (Float(u) + 0.5) * across, y = top + (Float(v) + 0.5) * down
                        let w = whole.value(atX: x * wholeAcross, y: y * wholeDown)
                        matching[v * network.width + u] = w
                        let s = seen[v * network.width + u]
                        sums.s += Double(s); sums.w += Double(w); sums.ss += Double(s * s); sums.sw += Double(s * w)
                    }
                }
                let n = Double(seen.count)
                let spreadOfSeen = sums.ss - sums.s * sums.s / n
                guard spreadOfSeen > 1e-9 else { continue }
                let a = Float((sums.sw - sums.s * sums.w / n) / spreadOfSeen), b = Float((sums.w - Double(a) * sums.s) / n)
                // A part that goes the other way from the whole has misread what it saw.
                guard a > 0 else { continue }
                let fitted = Grid(width: network.width, height: network.height, values: seen.map { $0 * a + b })
                // The same size of detail in the part's own cells.
                let own = fitted.blurred(radius: max(Int((Float(radius) * cell / across).rounded()), 1))
                let x0 = Int(left / cell), x1 = min(Int((left + Float(part.width)) / cell), width)
                let y0 = Int(top / cell), y1 = min(Int((top + Float(part.height)) / cell), height)
                for y in y0..<y1 {
                    for x in x0..<x1 {
                        let px = ((Float(x) + 0.5) * cell - left) / across, py = ((Float(y) + 0.5) * cell - top) / down
                        // Weighs most at the part's middle, fading to its edges, so parts meet without a seam.
                        let fx = min(px, Float(network.width) - px) / (Float(network.width) * 0.25)
                        let fy = min(py, Float(network.height) - py) / (Float(network.height) * 0.25)
                        let w = max(min(fx, fy, 1), 0.001)
                        detail[y * width + x] += w * (fitted.value(atX: px, y: py) - own.value(atX: px, y: py))
                        weight[y * width + x] += w
                    }
                }
            }
        }
        var fine = [Float](repeating: 0, count: width * height)
        for i in fine.indices {
            let lent = weight[i] > 0 ? detail[i] / weight[i] : spread.values[i] - smooth.values[i]
            // Nothing in the sky has detail worth lending.
            let ground = min(max((smooth.values[i] - sky) / (Wallpaper.ground - sky), 0), 1)
            fine[i] = min(max(smooth.values[i] + lent * ground + (spread.values[i] - smooth.values[i]) * (1 - ground), 0), 1)
        }
        return (DepthMap(width: whole.width, height: whole.height, values: whole.values), Grid(width: width, height: height, values: fine))
    }

    /// Moves the edges of a depth grid onto the edges of the picture: the guided filter, with the picture's lightness
    /// as the guide. Where the picture has an edge near a step in depth, the step takes its place and shape. Where the
    /// picture is even, a dark slope against a dark sky, the filter would only blur the depth, so there it stays as
    /// it was.
    static func snapped(_ depth: Grid, to lightness: [UInt8], radius: Int = 4, give: Float = 0.0004) -> Grid {
        guard lightness.count == depth.values.count else { return depth }
        let guide = Grid(width: depth.width, height: depth.height, values: lightness.map { Float($0) / 255 })
        func product(_ a: Grid, _ b: Grid) -> Grid {
            var values = [Float](repeating: 0, count: a.values.count)
            vDSP_vmul(a.values, 1, b.values, 1, &values, 1, vDSP_Length(values.count))
            return Grid(width: a.width, height: a.height, values: values)
        }
        let meanGuide = guide.blurred(radius: radius), meanDepth = depth.blurred(radius: radius)
        let meanSquare = product(guide, guide).blurred(radius: radius), meanBoth = product(guide, depth).blurred(radius: radius)
        var slope = [Float](repeating: 0, count: depth.values.count), offset = slope, edged = slope
        for i in slope.indices {
            let variance = max(meanSquare.values[i] - meanGuide.values[i] * meanGuide.values[i], 0)
            let together = meanBoth.values[i] - meanGuide.values[i] * meanDepth.values[i]
            slope[i] = together / (variance + give)
            offset[i] = meanDepth.values[i] - slope[i] * meanGuide.values[i]
            edged[i] = variance / (variance + 4 * give)
        }
        let meanSlope = Grid(width: depth.width, height: depth.height, values: slope).blurred(radius: radius)
        let meanOffset = Grid(width: depth.width, height: depth.height, values: offset).blurred(radius: radius)
        let meanEdged = Grid(width: depth.width, height: depth.height, values: edged).blurred(radius: radius)
        var values = [Float](repeating: 0, count: depth.values.count)
        for i in values.indices {
            let moved = meanSlope.values[i] * guide.values[i] + meanOffset.values[i]
            values[i] = min(max(depth.values[i] + (moved - depth.values[i]) * meanEdged.values[i], 0), 1)
        }
        return Grid(width: depth.width, height: depth.height, values: values)
    }
}
