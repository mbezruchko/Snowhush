import AppKit
import Metal
import QuartzCore
import simd

/// Draws a snowman standing in the wallpaper's picture into a small layer of its own that goes along with him: hidden
/// wherever the picture is nearer than he is, lit as the picture is where he stands, and paler the farther off he is.
final class SnowmanRenderer {
    private struct Uniforms {
        /// The layer's bottom left and size, and the screen's size, in points.
        var origin: SIMD2<Float>
        var size: SIMD2<Float>
        var screen: SIMD2<Float>
        /// How near he is in the picture, and how near between its farthest and nearest, 0 to 1.
        var near: Float
        var nearness: Float
        var tint: SIMD4<Float>
    }

    private struct Part {
        var rect: SIMD4<Float>
        var matrix: SIMD4<Float>
        var offset: SIMD2<Float>
        var alpha: Float
        var unused: Float = 0
        /// What the whole picture covers, for a part that is only some of it.
        var picture: SIMD4<Float>
        /// A circle outside which nothing shows: middle and radius, and whether there is one.
        var cut: SIMD4<Float> = .zero
    }

    private let device: MTLDevice
    private let queue: MTLCommandQueue
    private let pipeline: MTLRenderPipelineState
    private let textures: (body: MTLTexture, arm: MTLTexture, hat: MTLTexture)
    let layer = CAMetalLayer()

    /// What every snowman is drawn with, made once for each GPU.
    private static var made: [UInt64: (pipeline: MTLRenderPipelineState, textures: (MTLTexture, MTLTexture, MTLTexture))] = [:]

    init?(device: MTLDevice) {
        guard let queue = device.makeCommandQueue() else { return nil }
        self.device = device
        self.queue = queue
        if let made = Self.made[device.registryID] {
            pipeline = made.pipeline
            textures = (made.textures.0, made.textures.1, made.textures.2)
        } else {
            guard let made = Self.make(device: device, queue: queue) else { return nil }
            Self.made[device.registryID] = made
            pipeline = made.pipeline
            textures = (made.textures.0, made.textures.1, made.textures.2)
        }
        layer.device = device
        layer.pixelFormat = .bgra8Unorm
        layer.isOpaque = false
        layer.framebufferOnly = true
        // The layer moves with him: what it shows and where it is change in one go.
        layer.presentsWithTransaction = true
        layer.actions = ["position": NSNull(), "bounds": NSNull(), "frame": NSNull(), "contents": NSNull(), "hidden": NSNull()]
    }

    private static func make(device: MTLDevice, queue: MTLCommandQueue) -> (pipeline: MTLRenderPipelineState, textures: (MTLTexture, MTLTexture, MTLTexture))? {
        guard let library = try? device.makeLibrary(source: snowmanShaderSource, options: nil),
              let parts = SnowmanArt.parts() else { return nil }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = library.makeFunction(name: "snowmanVertex")
        descriptor.fragmentFunction = library.makeFunction(name: "snowmanFragment")
        let attachment = descriptor.colorAttachments[0]!
        attachment.pixelFormat = .bgra8Unorm
        attachment.isBlendingEnabled = true
        attachment.sourceRGBBlendFactor = .one
        attachment.sourceAlphaBlendFactor = .one
        attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
        attachment.destinationAlphaBlendFactor = .oneMinusSourceAlpha
        guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else { return nil }
        func texture(_ image: CGImage) -> MTLTexture? {
            let width = image.width, height = image.height
            var pixels = [UInt8](repeating: 0, count: width * height * 4)
            guard let context = CGContext(data: &pixels, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                                          space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            let descriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: width, height: height, mipmapped: true)
            descriptor.usage = .shaderRead
            guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
            texture.replace(region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: pixels, bytesPerRow: width * 4)
            return texture
        }
        guard let body = texture(parts.body.image), let arm = texture(parts.arm.image), let hat = texture(parts.hat.image) else { return nil }
        if let commandBuffer = queue.makeCommandBuffer(), let blit = commandBuffer.makeBlitCommandEncoder() {
            for texture in [body, arm, hat] { blit.generateMipmaps(for: texture) }
            blit.endEncoding()
            commandBuffer.commit()
        }
        return (pipeline, (body, arm, hat))
    }

    /// Draws `bot` over the picture whose relief is `relief`, on a screen `screen` points across, at `scale` pixels a
    /// point.
    func draw(_ bot: SnowmanBot, relief: MTLTexture, screen: SIMD2<Float>, scale: CGFloat) {
        let reach = bot.reach
        // Sized in steps, so that the layer's pictures need making again only now and then as he comes nearer.
        let step: Float = 24
        let width = ((reach.z - reach.x) / step).rounded(.up) * step, height = ((reach.w - reach.y) / step).rounded(.up) * step
        let origin = SIMD2((reach.x + reach.z - width) / 2, reach.y).rounded(.down)
        let frame = CGRect(x: CGFloat(origin.x), y: CGFloat(origin.y), width: CGFloat(width), height: CGFloat(height))
        let size = CGSize(width: frame.width * scale, height: frame.height * scale)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        if layer.contentsScale != scale { layer.contentsScale = scale }
        if layer.drawableSize != size { layer.drawableSize = size }
        layer.frame = frame
        guard let drawable = layer.nextDrawable(), let commandBuffer = queue.makeCommandBuffer() else { return }
        let pass = MTLRenderPassDescriptor()
        pass.colorAttachments[0].texture = drawable.texture
        pass.colorAttachments[0].loadAction = .clear
        pass.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 0)
        pass.colorAttachments[0].storeAction = .store
        guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else { return }
        encode(bot, relief: relief, screen: screen, origin: origin, size: SIMD2(width, height), into: encoder)
        encoder.endEncoding()
        commandBuffer.commit()
        commandBuffer.waitUntilScheduled()
        drawable.present()
    }

    /// Encodes `bot` into a pass over the region at `origin`, `size` points across.
    func encode(_ bot: SnowmanBot, relief: MTLTexture, screen: SIMD2<Float>, origin: SIMD2<Float>, size: SIMD2<Float>, into encoder: MTLRenderCommandEncoder) {
        let tint = bot.tint
        var uniforms = Uniforms(origin: origin, size: size, screen: screen, near: bot.near, nearness: bot.ground.nearness(bot.near),
                                tint: SIMD4(tint, 1))
        encoder.setRenderPipelineState(pipeline)
        encoder.setVertexBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
        encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 1)
        encoder.setFragmentTexture(relief, index: 1)
        let placements = bot.placements
        // The throwing arm goes behind him while he winds up, in front of him as it comes over.
        let armBehind = bot.armBehind
        var order: [(SnowmanBot.Placement, SIMD4<Float>, SIMD4<Float>, MTLTexture, SIMD4<Float>)] = []
        if bot.knockedOut {
            // Broken apart: each ball cut out of the body's picture, the arm, the hat flying off.
            for piece in bot.debris {
                order.append((piece.placement, piece.rect, piece.arm ? SnowmanArt.armRect : SnowmanArt.bodyRect, piece.arm ? textures.arm : textures.body, piece.cut))
            }
        } else {
            if armBehind { order.append((placements.arm, SnowmanArt.armRect, SnowmanArt.armRect, textures.arm, .zero)) }
            order.append((placements.body, SnowmanArt.bodyRect, SnowmanArt.bodyRect, textures.body, .zero))
            if !armBehind { order.append((placements.arm, SnowmanArt.armRect, SnowmanArt.armRect, textures.arm, .zero)) }
        }
        order.append((placements.hat, SnowmanArt.hatRect, SnowmanArt.hatRect, textures.hat, .zero))
        for (placement, rect, whole, texture, cut) in order where placement.alpha > 0.001 {
            let m = placement.matrix
            var part = Part(rect: rect, matrix: SIMD4<Float>(lowHalf: m.columns.0, highHalf: m.columns.1), offset: placement.offset, alpha: placement.alpha,
                            picture: whole, cut: cut)
            encoder.setVertexBytes(&part, length: MemoryLayout<Part>.stride, index: 0)
            encoder.setFragmentBytes(&part, length: MemoryLayout<Part>.stride, index: 0)
            encoder.setFragmentTexture(texture, index: 0)
            encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        }
    }

    /// Shows nothing until drawn again.
    func clear() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer.frame = .zero
        CATransaction.commit()
    }
}

private let snowmanShaderSource = """
#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    float2 origin;
    float2 size;
    float2 screen;
    float near;
    float nearness;
    float4 tint;
};

struct Part {
    float4 rect;
    float4 matrix;
    float2 offset;
    float alpha;
    float unused;
    float4 picture;
    float4 cut;
};

struct Out {
    float4 position [[position]];
    float2 uv;
    float2 world;
    float2 local;
};

vertex Out snowmanVertex(uint vid [[vertex_id]], constant Part &part [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 local = mix(part.rect.xy, part.rect.zw, corner);
    float2 world = part.offset + float2x2(part.matrix.xy, part.matrix.zw) * local;
    Out out;
    out.position = float4((world - u.origin) / u.size * 2.0 - 1.0, 0.0, 1.0);
    out.uv = float2((local.x - part.picture.x) / (part.picture.z - part.picture.x), 1.0 - (local.y - part.picture.y) / (part.picture.w - part.picture.y));
    out.world = world;
    out.local = local;
    return out;
}

fragment float4 snowmanFragment(Out in [[stage_in]], constant Part &part [[buffer(0)]], constant Uniforms &u [[buffer(1)]],
                                texture2d<float> picture [[texture(0)]], texture2d<float> relief [[texture(1)]]) {
    constexpr sampler smooth(filter::linear, mip_filter::linear, address::clamp_to_edge);
    float4 color = picture.sample(smooth, in.uv);
    // A ball cut out of the body: nothing of the others round it.
    if (part.cut.w > 0.0) color *= 1.0 - smoothstep(part.cut.z * 0.96, part.cut.z, distance(in.local, part.cut.xy));
    if (color.a < 0.003) discard_fragment();
    // Hidden behind whatever in the picture is nearer than he is: a rock, a tree, the brow of a hill.
    float2 at = float2(in.world.x / u.screen.x, 1.0 - in.world.y / u.screen.y);
    float ahead = relief.sample(smooth, at, level(0.0)).a;
    float shown = 1.0 - smoothstep(u.near + 0.012, u.near + 0.035, ahead);
    if (shown <= 0.0) discard_fragment();
    // Lit as the snow is where he stands, and paler with the air between the farther off he is.
    float3 rgb = color.rgb / color.a * u.tint.rgb;
    rgb = mix(rgb, u.tint.rgb * 0.92, pow(1.0 - u.nearness, 1.5) * 0.45);
    float a = color.a * shown * part.alpha;
    return float4(rgb * a, a);
}
"""
