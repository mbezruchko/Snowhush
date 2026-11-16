import Accelerate
import AppKit
import CoreML
import CryptoKit

/// What snow on the wallpaper needs to know of it: where it can lie, and how the picture lights it.
struct WallpaperScene: Codable, Equatable {
    var ridges: [WallpaperRidge]
    var light: WallpaperLight
    var terrain: WallpaperTerrain
    var shade: WallpaperShade
}

/// How light the wallpaper is, a cell to every `cell` points, top row first, 0 to 255: snow on it shows first where
/// it is lighter than around, on flat ground and old snow, and last on the dark of trees and rock. Kept on disk as a PNG.
struct WallpaperShade: Codable, Equatable {
    var width: Int
    var height: Int
    var cell: Int
    var pixels: Data

    init(width: Int, height: Int, cell: Int, pixels: Data) {
        self.width = width
        self.height = height
        self.cell = cell
        self.pixels = pixels
    }

    private enum Keys: String, CodingKey { case width, height, cell, png }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: Keys.self)
        let width = try values.decode(Int.self, forKey: .width), height = try values.decode(Int.self, forKey: .height)
        self.width = width
        self.height = height
        cell = try values.decode(Int.self, forKey: .cell)
        let png = try values.decode(Data.self, forKey: .png)
        var decoded = Data(count: width * height)
        let read = decoded.withUnsafeMutableBytes { bytes -> Bool in
            guard let source = CGImageSourceCreateWithData(png as CFData, nil), let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
                  let context = CGContext(data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width,
                                          space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard read else { throw DecodingError.dataCorruptedError(forKey: .png, in: values, debugDescription: "unreadable shade") }
        pixels = decoded
    }

    func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: Keys.self)
        try values.encode(width, forKey: .width)
        try values.encode(height, forKey: .height)
        try values.encode(cell, forKey: .cell)
        let png = NSMutableData()
        guard let provider = CGDataProvider(data: pixels as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: width, space: CGColorSpaceCreateDeviceGray(),
                                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue), provider: provider, decode: nil,
                                  shouldInterpolate: false, intent: .defaultIntent),
              let destination = CGImageDestinationCreateWithData(png, "public.png" as CFString, 1, nil) else { return }
        CGImageDestinationAddImage(destination, image, nil)
        CGImageDestinationFinalize(destination)
        try values.encode(png as Data, forKey: .png)
    }
}

/// The picture's surfaces as depth sees them, a cell to every `cell` points, top row first: how near each is, and how
/// well it holds snow, from 0 for the sky, a cliff or the edge of something to 1 for ground facing up.
struct WallpaperTerrain: Codable, Equatable {
    var width: Int
    var height: Int
    var cell: Int
    /// 0 to 255 for 0 to 1.
    var near: Data
    var hold: Data
    /// How near the farthest and the nearest of the things in it are, the sky aside.
    var far: Float
    var close: Float
}

/// The light snow would take on across the wallpaper, a cell to every `cell` points, top row first, RGBA: from the
/// brightest light on what it lies on, so it glows where the picture glows and greys where it is dusk.
struct WallpaperLight: Codable, Equatable {
    var width: Int
    var height: Int
    var cell: Int
    var pixels: Data
}

/// The top of something in the wallpaper that snow can lie on: a ridge against the sky, a roof, a branch.
struct WallpaperRidge: Codable, Equatable {
    /// Where its first column starts, in points from the screen's left edge.
    var left: Float
    /// The top of the thing at each column, in points from the screen's bottom.
    var tops: [Float]
    /// How near it looks, from 0 for the horizon to 1 for right in front.
    var nearness: Float
}

/// The wallpaper as a display shows it, and the tops of the things in it found from a depth map: wherever a nearer
/// thing ends against something farther behind it, its top edge can catch snow. The depth comes once per picture
/// from a small network on the Neural Engine, and what is found is kept on disk, so it costs nothing on later launches.
enum Wallpaper {
    /// A display's wallpaper and how it is laid out on it.
    struct Picture: Equatable {
        let url: URL
        /// The screen's size in points.
        let size: CGSize
        /// Where the picture is drawn, in points from the screen's top left; it may reach past the screen.
        let placement: CGRect
        let fill: NSColor?
        let modified: Date
        let bytes: Int

        static func == (a: Picture, b: Picture) -> Bool {
            a.url == b.url && a.size == b.size && a.placement == b.placement && a.modified == b.modified && a.bytes == b.bytes
        }
    }

    /// Raise whenever what is found changes, so pictures already done are looked at again.
    private static let version = 15

    /// What the screen shows of its wallpaper now; nil for a moving or missing one.
    static func picture(on screen: NSScreen) -> Picture? {
        guard let url = NSWorkspace.shared.desktopImageURL(for: screen) else { return nil }
        let options = NSWorkspace.shared.desktopImageOptions(for: screen) ?? [:]
        let scaling = (options[.imageScaling] as? NSNumber).flatMap { NSImageScaling(rawValue: $0.uintValue) } ?? .scaleProportionallyUpOrDown
        let clips = (options[.allowClipping] as? NSNumber)?.boolValue ?? true
        return picture(url: url, size: screen.frame.size, scaling: scaling, clips: clips, fill: options[.fillColor] as? NSColor)
    }

    static func picture(url picked: URL, size: CGSize, scaling: NSImageScaling, clips: Bool, fill: NSColor?) -> Picture? {
        guard let url = picture(for: picked),
              let values = try? url.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey, .isRegularFileKey]),
              values.isRegularFile == true,
              let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let pixels = orientedSize(of: source) else { return nil }
        let placement: CGRect
        switch scaling {
        case .scaleAxesIndependently:
            placement = CGRect(origin: .zero, size: size)
        case .scaleNone:
            placement = CGRect(x: (size.width - pixels.width) / 2, y: (size.height - pixels.height) / 2, width: pixels.width, height: pixels.height)
        default:
            let scale = clips ? max(size.width / pixels.width, size.height / pixels.height) : min(size.width / pixels.width, size.height / pixels.height)
            let drawn = CGSize(width: pixels.width * scale, height: pixels.height * scale)
            placement = CGRect(x: (size.width - drawn.width) / 2, y: (size.height - drawn.height) / 2, width: drawn.width, height: drawn.height)
        }
        return Picture(url: url, size: size, placement: placement, fill: fill, modified: values.contentModificationDate ?? .distantPast,
                       bytes: values.fileSize ?? 0)
    }

    /// A wallpaper picked from the system's own is only a description of it: the picture itself is an asset the system
    /// downloads, found by its id. Nil until it's there.
    private static func picture(for url: URL) -> URL? {
        guard url.pathExtension == "madesktop" else { return url }
        guard let description = NSDictionary(contentsOf: url), let id = description["mobileAssetID"] as? String else { return nil }
        let assets = URL(fileURLWithPath: "/System/Library/AssetsV2/com_apple_MobileAsset_DesktopPicture")
        let found = (try? FileManager.default.contentsOfDirectory(at: assets, includingPropertiesForKeys: nil)) ?? []
        for asset in found where asset.pathExtension == "asset" {
            guard let info = NSDictionary(contentsOf: asset.appendingPathComponent("Info.plist")),
                  (info["MobileAssetProperties"] as? [String: Any])?["DesktopPictureID"] as? String == id else { continue }
            let data = (try? FileManager.default.contentsOfDirectory(at: asset.appendingPathComponent("AssetData"), includingPropertiesForKeys: nil)) ?? []
            return data.first { ["heic", "jpg", "jpeg", "png"].contains($0.pathExtension.lowercased()) }
        }
        return nil
    }

    /// The ridges in the picture and its light, from the cache when it was looked at before. Slow the first time: call
    /// it off the main thread.
    static func scene(in picture: Picture, columnWidth: Float, model: URL? = Bundle.main.url(forResource: modelName, withExtension: "mlmodelc")) -> WallpaperScene? {
        let file = cacheFile(for: picture, columnWidth: columnWidth)
        if let file, let data = try? Data(contentsOf: file), let kept = try? JSONDecoder().decode(WallpaperScene.self, from: data) {
            return kept
        }
        guard let model, let shown = render(picture), let depth = DepthMap(of: shown, model: model) else { return nil }
        let ridges = find(in: shown, depth: depth, columnWidth: columnWidth)
        let found = WallpaperScene(ridges: ridges, light: light(of: shown),
                                   terrain: terrain(of: depth, size: picture.size, ridges: ridges, columnWidth: columnWidth), shade: shade(of: shown))
        if let file, let data = try? JSONEncoder().encode(found) {
            let folder = file.deletingLastPathComponent()
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try? data.write(to: file, options: .atomic)
            forgetOld(in: folder)
        }
        return found
    }

    static let modelName = "DepthAnythingV2SmallF16P6"

    private static func cacheFile(for picture: Picture, columnWidth: Float) -> URL? {
        guard let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return nil }
        let key = [picture.url.path, "\(picture.size)", "\(picture.placement)", "\(picture.modified.timeIntervalSince1970)", "\(picture.bytes)",
                   "\(columnWidth)", "\(version)"].joined(separator: "|")
        let name = SHA256.hash(data: Data(key.utf8)).prefix(12).map { String(format: "%02x", $0) }.joined()
        return caches.appendingPathComponent("com.mbezruchko.Snowhush/Wallpaper/\(name).json")
    }

    /// Keeps the pictures looked at most recently, for a wallpaper that changes on its own every so often.
    private static func forgetOld(in folder: URL, keeping kept: Int = 32) {
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        guard files.count > kept else { return }
        let dated = files.map { ($0, (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast) }
        for (file, _) in dated.sorted(by: { $0.1 > $1.1 }).dropFirst(kept) { try? FileManager.default.removeItem(at: file) }
    }

    private static func orientedSize(of source: CGImageSource) -> CGSize? {
        let index = CGImageSourceGetPrimaryImageIndex(source)
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
              let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.doubleValue,
              let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.doubleValue, width > 0, height > 0 else { return nil }
        let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue ?? 1
        return orientation >= 5 ? CGSize(width: height, height: width) : CGSize(width: width, height: height)
    }

    /// The screen as the wallpaper fills it, a pixel to a point. A dynamic wallpaper's pictures all show the same
    /// scene, so any of them does.
    static func render(_ picture: Picture) -> Shown? {
        guard let source = CGImageSourceCreateWithURL(picture.url as CFURL, nil) else { return nil }
        let biggest = max(picture.placement.width, picture.placement.height)
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(biggest.rounded(.up)),
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, CGImageSourceGetPrimaryImageIndex(source), options as CFDictionary) else { return nil }
        let width = Int(picture.size.width), height = Int(picture.size.height)
        guard width > 0, height > 0 else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { bytes -> Bool in
            guard let context = CGContext(data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                                          space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return false }
            if let fill = picture.fill?.usingColorSpace(.deviceRGB) {
                context.setFillColor(fill.cgColor)
                context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            }
            context.interpolationQuality = .high
            let place = picture.placement
            context.draw(image, in: CGRect(x: place.minX, y: picture.size.height - place.maxY, width: place.width, height: place.height))
            return true
        }
        return drawn ? Shown(width: width, height: height, pixels: pixels) : nil
    }

    /// The screen's wallpaper pixels, top row first, RGBX.
    struct Shown {
        let width: Int
        let height: Int
        let pixels: [UInt8]

        func image() -> CGImage? {
            guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
            return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                           bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue), provider: provider, decode: nil,
                           shouldInterpolate: false, intent: .defaultIntent)
        }
    }

    /// Relative depth across the screen, top row first: 1 the nearest, 0 the farthest.
    struct DepthMap {
        let width: Int
        let height: Int
        let values: [Float]

        init(width: Int, height: Int, values: [Float]) {
            self.width = width
            self.height = height
            self.values = values
        }

        /// Runs the network once, then lets it go.
        init?(of shown: Shown, model url: URL) {
            let configuration = MLModelConfiguration()
            configuration.computeUnits = .cpuAndNeuralEngine
            guard let model = try? MLModel(contentsOf: url, configuration: configuration),
                  let constraint = model.modelDescription.inputDescriptionsByName["image"]?.imageConstraint,
                  let image = shown.image() else { return nil }
            let width = constraint.pixelsWide, height = constraint.pixelsHigh
            var input: CVPixelBuffer?
            guard CVPixelBufferCreate(nil, width, height, kCVPixelFormatType_32BGRA, nil, &input) == kCVReturnSuccess, let input else { return nil }
            CVPixelBufferLockBaseAddress(input, [])
            let context = CGContext(data: CVPixelBufferGetBaseAddress(input), width: width, height: height, bitsPerComponent: 8,
                                    bytesPerRow: CVPixelBufferGetBytesPerRow(input), space: CGColorSpaceCreateDeviceRGB(),
                                    bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
            context?.interpolationQuality = .high
            context?.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            CVPixelBufferUnlockBaseAddress(input, [])
            guard context != nil,
                  let features = try? MLDictionaryFeatureProvider(dictionary: ["image": MLFeatureValue(pixelBuffer: input)]),
                  let output = try? model.prediction(from: features),
                  let name = model.modelDescription.outputDescriptionsByName.keys.first,
                  let depth = output.featureValue(for: name)?.imageBufferValue else { return nil }
            guard let values = Self.floats(of: depth) else { return nil }
            let low = values.min() ?? 0, high = values.max() ?? 1
            let range = max(high - low, 1e-6)
            self.init(width: CVPixelBufferGetWidth(depth), height: CVPixelBufferGetHeight(depth), values: values.map { ($0 - low) / range })
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

        /// Bilinear, in the screen's points from its top left.
        func value(x: Float, y: Float, screen: SIMD2<Float>) -> Float {
            let u = min(max(x / screen.x * Float(width) - 0.5, 0), Float(width - 1))
            let v = min(max(y / screen.y * Float(height) - 0.5, 0), Float(height - 1))
            let x0 = Int(u), y0 = Int(v)
            let x1 = min(x0 + 1, width - 1), y1 = min(y0 + 1, height - 1)
            let fx = u - Float(x0), fy = v - Float(y0)
            let top = values[y0 * width + x0] * (1 - fx) + values[y0 * width + x1] * fx
            let bottom = values[y1 * width + x0] * (1 - fx) + values[y1 * width + x1] * fx
            return top * (1 - fy) + bottom * fy
        }
    }

    // MARK: - Light

    /// Snow lit by the picture. Snow is the brightest thing around and is lit as what it lies on is, so each cell
    /// takes the colour of the brightest light on the thing under it: the bright half of the pixels in each cell
    /// of a few at and below it, the one that is lightest. The snow keeps most of that light's hue and as much of
    /// its brightness as is there, so it glows orange under a sunset and greys at dusk, like the snow already in
    /// the picture.
    static func light(of shown: Shown, cell: Int = 16) -> WallpaperLight {
        let width = (shown.width + cell - 1) / cell, height = (shown.height + cell - 1) / cell
        func lightness(_ r: Float, _ g: Float, _ b: Float) -> Float { 0.2126 * r + 0.7152 * g + 0.0722 * b }
        var level = [Float](repeating: 0, count: width * height)
        var counts = [Float](repeating: 0, count: width * height)
        shown.pixels.withUnsafeBufferPointer { p in
            for y in 0..<shown.height {
                let row = (y / cell) * width
                for x in 0..<shown.width {
                    let i = (y * shown.width + x) * 4
                    level[row + x / cell] += lightness(Float(p[i]), Float(p[i + 1]), Float(p[i + 2]))
                    counts[row + x / cell] += 1
                }
            }
        }
        for k in level.indices { level[k] /= max(counts[k], 1) }
        var bright = [SIMD4<Float>](repeating: .zero, count: width * height)
        shown.pixels.withUnsafeBufferPointer { p in
            for y in 0..<shown.height {
                let row = (y / cell) * width
                for x in 0..<shown.width {
                    let i = (y * shown.width + x) * 4
                    let k = row + x / cell
                    let r = Float(p[i]), g = Float(p[i + 1]), b = Float(p[i + 2])
                    if lightness(r, g, b) >= level[k] { bright[k] += SIMD4(r, g, b, 255) }
                }
            }
        }
        let colours = bright.map { $0.w > 0 ? SIMD3($0.x, $0.y, $0.z) / $0.w : SIMD3(repeating: 0) }
        var pixels = [UInt8](repeating: 255, count: width * height * 4)
        for y in 0..<height {
            for x in 0..<width {
                var best = SIMD3<Float>(repeating: 0), lightest: Float = -1
                for dy in 0...2 {
                    for dx in -1...1 {
                        let c = colours[min(max(y + dy, 0), height - 1) * width + min(max(x + dx, 0), width - 1)]
                        let l = lightness(c.x, c.y, c.z)
                        if l > lightest { lightest = l; best = c }
                    }
                }
                let hue = SIMD3<Float>(repeating: 0.25) + best / max(best.x, best.y, best.z, 0.05) * 0.75
                let tint = hue * min(max(lightest * 1.15 + 0.1, 0.62), 1)
                let i = (y * width + x) * 4
                pixels[i] = UInt8(min(tint.x, 1) * 255)
                pixels[i + 1] = UInt8(min(tint.y, 1) * 255)
                pixels[i + 2] = UInt8(min(tint.z, 1) * 255)
            }
        }
        return WallpaperLight(width: width, height: height, cell: cell, pixels: Data(pixels))
    }

    // MARK: - Terrain

    /// The sky's nearness and less, in the depth map's own range.
    static let sky: Float = 0.06
    /// Clouds come out a little nearer than the sky: only what is nearer still is ground for snow to lie on.
    static let ground: Float = 0.12

    /// How near each spot of the picture is, and how well it holds snow. A surface facing up recedes as it rises on
    /// the screen, so its depth grows nearer going down; a cliff facing the viewer stays as near all the way down. Where
    /// the depth jumps at the top of something, the top holds snow up to its edge and the sky over it doesn't; above a
    /// ridge's own edge, sharper than the depth map's, nothing lies.
    static func terrain(of depth: DepthMap, size: CGSize, ridges: [WallpaperRidge], columnWidth: Float, cell: Int = 4) -> WallpaperTerrain {
        let screen = SIMD2(Float(size.width), Float(size.height))
        let width = Int((screen.x / Float(cell)).rounded(.up)), height = Int((screen.y / Float(cell)).rounded(.up))
        var near = [Float](repeating: 0, count: width * height)
        for y in 0..<height {
            for x in 0..<width {
                near[y * width + x] = depth.value(x: (Float(x) + 0.5) * Float(cell), y: (Float(y) + 0.5) * Float(cell), screen: screen)
            }
        }
        var hold = [Float](repeating: 0, count: width * height)
        for y in 1..<(height - 1) {
            for x in 1..<(width - 1) {
                let i = y * width + x
                guard near[i] > ground else { continue }
                let down = (near[i + width] - near[i - width]) / 2
                let across = (near[i + 1] - near[i - 1]) / 2
                let above = near[max(y - 2, 0) * width + x], below = near[min(y + 2, height - 1) * width + x]
                // Up in a jump in depth, nearer the far side than the near: the sky over the top of something.
                if below - above > 2 * jump.lowerBound && near[i] - above < (below - above) / 2 { continue }
                // The depth map squeezes the distance: far off, the same slope changes its depth less.
                let facing = smoothstep(rise.lowerBound, rise.upperBound, down / (near[i] + 0.15))
                let side = smoothstep(jump.lowerBound, jump.upperBound, abs(across))
                hold[i] = facing * (1 - side)
            }
        }
        // A ridge's own edge is sharper than the depth map's: its top holds snow right up to it, and nothing lies over it.
        func ridgeColumns(_ body: (_ x: Int, _ edge: Float) -> Void) {
            for ridge in ridges {
                for (k, top) in ridge.tops.enumerated() {
                    let x = Int((ridge.left + (Float(k) + 0.5) * columnWidth) / Float(cell))
                    let edge = screen.y - top - 3.5
                    if x >= 0, x < width, edge > 0 { body(x, edge) }
                }
            }
        }
        ridgeColumns { x, edge in
            for y in max(Int(edge / Float(cell)), 0)..<min(Int((edge + 16) / Float(cell)), height) { hold[y * width + x] = max(hold[y * width + x], 0.8) }
        }
        // Softened over a few cells, the edges of what holds snow follow the land rather than the cells.
        for pass in 0..<2 {
            var soft = hold
            let step = pass == 0 ? 1 : width
            for i in hold.indices {
                let a = i - step >= 0 && (pass == 1 || i % width > 0) ? hold[i - step] : hold[i]
                let b = i + step < hold.count && (pass == 1 || i % width < width - 1) ? hold[i + step] : hold[i]
                soft[i] = 0.25 * a + 0.5 * hold[i] + 0.25 * b
            }
            hold = soft
        }
        for i in hold.indices where near[i] <= ground { hold[i] = 0 }
        ridgeColumns { x, edge in
            for y in max(Int((edge - 28) / Float(cell)), 0)..<min(Int(edge / Float(cell)), height) { hold[y * width + x] = 0 }
        }
        let things = near.filter { $0 > sky }.sorted()
        let far = things.isEmpty ? sky : things[Int(Float(things.count - 1) * 0.02)]
        let close = things.isEmpty ? 1 : things[Int(Float(things.count - 1) * 0.98)]
        return WallpaperTerrain(width: width, height: height, cell: cell, near: Data(near.map { UInt8(min(max($0, 0), 1) * 255) }),
                                hold: Data(hold.map { UInt8(min(max($0, 0), 1) * 255) }), far: far, close: close)
    }

    /// How light the picture is, a cell to every `cell` points.
    static func shade(of shown: Shown, cell: Int = 2) -> WallpaperShade {
        let width = shown.width / cell, height = shown.height / cell
        var pixels = [UInt8](repeating: 0, count: width * height)
        shown.pixels.withUnsafeBufferPointer { p in
            for y in 0..<height {
                for x in 0..<width {
                    var sum: Float = 0
                    for dy in 0..<cell {
                        for dx in 0..<cell {
                            let i = ((y * cell + dy) * shown.width + x * cell + dx) * 4
                            sum += 0.2126 * Float(p[i]) + 0.7152 * Float(p[i + 1]) + 0.0722 * Float(p[i + 2])
                        }
                    }
                    pixels[y * width + x] = UInt8(min(sum / Float(cell * cell), 255))
                }
            }
        }
        return WallpaperShade(width: width, height: height, cell: cell, pixels: Data(pixels))
    }

    /// How fast, per cell down, the depth must grow nearer for a surface to hold snow at all, and to hold it fully.
    static let rise: ClosedRange<Float> = 0.001...0.006
    /// How steep a step in depth, per cell, is the edge of something rather than a surface.
    static let jump: ClosedRange<Float> = 0.012...0.03

    private static func smoothstep(_ a: Float, _ b: Float, _ x: Float) -> Float {
        let t = min(max((x - a) / (b - a), 0), 1)
        return t * t * (3 - 2 * t)
    }

    // MARK: - Finding the ridges

    /// How much nearer, in the depth map's own range, a thing must be than what is right above it for its top to count.
    private static let step: Float = 0.035
    /// How far apart in points, across one column, two points of a ridge can still be the same ridge.
    private static let link: Float = 6
    /// How far the colour edge may lie from where the depth map puts it, in points: the network's edges are soft and
    /// a few of its pixels off.
    private static let reach = 12
    /// The fewest points a ridge must run across to be kept.
    private static let shortest: Float = 64
    /// The most columns of ridges kept over the whole screen, to keep the snow on them cheap.
    private static let budget = 5000
    /// Ridges this near the screen's top are under the menu bar; this near its bottom, under the drift.
    private static let topMargin: Float = 40
    private static let bottomMargin: Float = 48

    private struct Chain {
        var first: Int
        var rows: [Float]
        var steps: [Float]
        var near: [Float]
        var missed = 0
    }

    static func find(in shown: Shown, depth: DepthMap, columnWidth: Float) -> [WallpaperRidge] {
        let screen = SIMD2(Float(shown.width), Float(shown.height))
        let columns = Int(screen.x / columnWidth)
        let rowHeight = screen.y / Float(depth.height)
        var open: [Chain] = []
        var done: [Chain] = []
        var profile = [Float](repeating: 0, count: depth.height)
        for c in 0..<columns {
            let x = (Float(c) + 0.5) * columnWidth
            for v in 0..<depth.height { profile[v] = depth.value(x: x, y: (Float(v) + 0.5) * rowHeight, screen: screen) }
            // Where the depth steps up going down: something nearer starts there, with something farther above it.
            var found: [(row: Float, step: Float, near: Float)] = []
            for v in 1..<(depth.height - 1) {
                let rise = profile[min(v + 2, depth.height - 1)] - profile[max(v - 2, 0)]
                let before = profile[min(v + 1, depth.height - 1)] - profile[max(v - 3, 0)]
                let after = profile[min(v + 3, depth.height - 1)] - profile[max(v - 1, 0)]
                guard rise > step, rise >= before, rise > after else { continue }
                found.append(((Float(v) + 0.5) * rowHeight, rise, profile[min(v + 4, depth.height - 1)]))
            }
            var taken = [Bool](repeating: false, count: found.count)
            for k in open.indices {
                let last = open[k].rows[open[k].rows.count - 1]
                var best: Int?
                for (j, point) in found.enumerated() where !taken[j] && abs(point.row - last) <= link * Float(open[k].missed + 1) {
                    if best == nil || abs(point.row - last) < abs(found[best!].row - last) { best = j }
                }
                if let best {
                    taken[best] = true
                    // Columns skipped on the way are filled in along a straight line.
                    for m in 0..<open[k].missed {
                        let t = Float(m + 1) / Float(open[k].missed + 1)
                        open[k].rows.append(last + (found[best].row - last) * t)
                        open[k].steps.append(0)
                        open[k].near.append(open[k].near.last ?? 0)
                    }
                    open[k].rows.append(found[best].row)
                    open[k].steps.append(found[best].step)
                    open[k].near.append(found[best].near)
                    open[k].missed = 0
                } else {
                    open[k].missed += 1
                }
            }
            done += open.filter { $0.missed > 3 }
            open.removeAll { $0.missed > 3 }
            for (j, point) in found.enumerated() where !taken[j] {
                open.append(Chain(first: c, rows: [point.row], steps: [point.step], near: [point.near]))
            }
        }
        done += open
        let minimum = Int(shortest / columnWidth)
        let paths = done.filter { $0.rows.count >= minimum }.map { snap($0, in: shown, columnWidth: columnWidth) }
        // How sharp the picture's clear edges are: next to them, a fainter edge is a ripple in the dark or a smooth
        // shading, not the top of anything.
        let all = paths.flatMap(\.strengths).sorted()
        let clear = all.isEmpty ? 0 : all[min(Int(Float(all.count) * 0.9), all.count - 1)]
        let pieces = paths.flatMap { cut($0, weak: max(0.035, 0.08 * clear), faint: max(0.06, 0.12 * clear), columnWidth: columnWidth) }
            .filter { $0.rows.count >= minimum }
        var ridges: [(ridge: WallpaperRidge, score: Float)] = []
        for piece in pieces {
            // Sunk a little into the thing's top, so no sky shows under the snow.
            let tops = piece.rows.map { screen.y - $0 - 4 }
            let lowest = tops.min() ?? 0, highest = tops.max() ?? 0
            guard lowest > bottomMargin, highest < screen.y - topMargin else { continue }
            let near = piece.near.reduce(0, +) / Float(piece.near.count)
            // Hardly nearer than the sky: the edge of a cloud.
            guard near > 0.1 else { continue }
            ridges.append((WallpaperRidge(left: Float(piece.first) * columnWidth, tops: tops, nearness: near), piece.score))
        }
        return keepBest(ridges, columnWidth: columnWidth)
    }

    /// A chain moved onto the picture's own edge: its row in each column and how sharp the edge is there.
    private struct Path {
        var first: Int
        var rows: [Float]
        var strengths: [Float]
        var steps: [Float]
        var near: [Float]
    }

    private struct Piece {
        var first: Int
        var rows: [Float]
        var near: [Float]
        var score: Float
    }

    /// Moves a chain from the depth map's soft edge onto the picture's own: the row in each column, within `reach`,
    /// where the colour changes most from above to below, with the path kept smooth.
    private static func snap(_ chain: Chain, in shown: Shown, columnWidth: Float) -> Path {
        let count = chain.rows.count
        let span = 2 * reach + 1
        var edge = [Float](repeating: 0, count: count * span)
        let width = Int(columnWidth)
        for i in 0..<count {
            let center = Int(chain.rows[i].rounded())
            let x0 = (chain.first + i) * width
            for s in 0..<span {
                edge[i * span + s] = contrast(shown, x: x0, width: width, row: center - reach + s)
            }
        }
        // Viterbi: each column picks a row; a column's row may move at most `turn` rows from its neighbour's, and
        // pays for moving at all.
        let turn = 3
        let bend: Float = 0.004
        let lower: Float = 0.006
        var total = [Float](repeating: 0, count: count * span)
        var from = [Int](repeating: 0, count: count * span)
        for s in 0..<span { total[s] = edge[s] + lower * Float(s - reach) }
        for i in 1..<count {
            let shift = Int(chain.rows[i].rounded()) - Int(chain.rows[i - 1].rounded())
            for s in 0..<span {
                var best = -Float.greatestFiniteMagnitude, pick = 0
                for d in -turn...turn {
                    let p = s + shift + d
                    guard p >= 0, p < span else { continue }
                    let value = total[(i - 1) * span + p] - bend * Float(d * d)
                    if value > best { best = value; pick = p }
                }
                // A haze or glow above a thing's top is part of what lies behind it: of two edges alike, the lower wins.
                total[i * span + s] = best + edge[i * span + s] + lower * Float(s - reach)
                from[i * span + s] = pick
            }
        }
        var rows = [Float](repeating: 0, count: count)
        var strengths = [Float](repeating: 0, count: count)
        var s = (0..<span).max { total[(count - 1) * span + $0] < total[(count - 1) * span + $1] } ?? reach
        for i in stride(from: count - 1, through: 0, by: -1) {
            rows[i] = Float(Int(chain.rows[i].rounded()) - reach + s)
            strengths[i] = edge[i * span + s]
            if i > 0 { s = from[i * span + s] }
        }
        // A little smoothing takes out the stair steps of whole rows.
        var smooth = rows
        for i in 1..<max(1, count - 1) { smooth[i] = 0.25 * rows[i - 1] + 0.5 * rows[i] + 0.25 * rows[i + 1] }
        return Path(first: chain.first, rows: smooth, strengths: strengths, steps: chain.steps, near: chain.near)
    }

    /// Cuts a path where the picture shows no edge to sit on for a while, or where it climbs too steeply for snow,
    /// and keeps the pieces whose edge is clear enough on the whole.
    private static func cut(_ path: Path, weak: Float, faint: Float, columnWidth: Float) -> [Piece] {
        let count = path.rows.count
        var pieces: [Piece] = []
        var start: Int?
        var gap = 0
        func close(_ end: Int) {
            guard let begin = start, end > begin else { return }
            let range = begin..<end
            let mean = range.reduce(Float(0)) { $0 + path.strengths[$1] } / Float(range.count)
            let rise = range.reduce(Float(0)) { $0 + path.steps[$1] } / Float(range.count)
            guard mean >= faint else { return }
            pieces.append(Piece(first: path.first + begin, rows: Array(path.rows[range]), near: Array(path.near[range]),
                                score: mean * rise * Float(range.count)))
        }
        // Snow slides off anything steeper than about 56 degrees, and a path that steep is mostly noise anyway.
        let steepest = 1.5 * columnWidth
        func steep(_ i: Int) -> Bool {
            let a = max(i - 2, 0), b = min(i + 2, count - 1)
            return abs(path.rows[b] - path.rows[a]) > steepest * Float(max(b - a, 1))
        }
        for i in 0..<count {
            if path.strengths[i] >= weak && !steep(i) {
                if start == nil { start = i }
                gap = 0
            } else if start != nil {
                gap += 1
                if gap >= 4 || steep(i) {
                    close(i - gap + 1)
                    start = nil
                    gap = 0
                }
            }
        }
        if start != nil { close(count - gap) }
        return pieces
    }

    /// How different the few rows just above `row` are from the few just below, over a column: the red, green and
    /// blue differences added up, 0 to 3.
    private static func contrast(_ shown: Shown, x: Int, width: Int, row: Int) -> Float {
        guard row >= 3, row + 3 < shown.height, x >= 0, x + width <= shown.width else { return 0 }
        var above = SIMD3<Float>(repeating: 0), below = SIMD3<Float>(repeating: 0)
        shown.pixels.withUnsafeBufferPointer { p in
            for dx in 0..<width {
                for dy in 1...3 {
                    let a = ((row - dy) * shown.width + x + dx) * 4
                    let b = ((row + dy) * shown.width + x + dx) * 4
                    above += SIMD3(Float(p[a]), Float(p[a + 1]), Float(p[a + 2]))
                    below += SIMD3(Float(p[b]), Float(p[b + 1]), Float(p[b + 2]))
                }
            }
        }
        let difference = above - below
        return (abs(difference.x) + abs(difference.y) + abs(difference.z)) / Float(3 * width * 255)
    }

    /// Drops ridges that double others, then keeps the strongest within the budget.
    private static func keepBest(_ ridges: [(ridge: WallpaperRidge, score: Float)], columnWidth: Float) -> [WallpaperRidge] {
        var kept: [WallpaperRidge] = []
        var used = 0
        for candidate in ridges.sorted(by: { $0.score > $1.score }) {
            let ridge = candidate.ridge
            let doubled = kept.contains { other in
                let first = max(ridge.left, other.left), last = min(ridge.left + Float(ridge.tops.count) * columnWidth, other.left + Float(other.tops.count) * columnWidth)
                guard last - first > 0 else { return false }
                var close = 0, all = 0
                var x = first
                while x < last {
                    let a = ridge.tops[min(Int((x - ridge.left) / columnWidth), ridge.tops.count - 1)]
                    let b = other.tops[min(Int((x - other.left) / columnWidth), other.tops.count - 1)]
                    if abs(a - b) < 6 { close += 1 }
                    all += 1
                    x += columnWidth
                }
                return close * 2 > all
            }
            guard !doubled, used + ridge.tops.count <= budget else { continue }
            kept.append(ridge)
            used += ridge.tops.count
        }
        return kept
    }
}
