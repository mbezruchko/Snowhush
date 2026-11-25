import Foundation

/// All distances are screen points. A bounded time step keeps throws stable after a stalled frame.
struct SnowmanMotion {
    var velocity = CGPoint.zero
    var spring: CGFloat = 0
    var springVelocity: CGFloat = 0
    var rotation: CGFloat = 0
    var support: SnowmanSupport?
    var greeting: CGFloat = 0
    var moving: Bool { hypot(velocity.x, velocity.y) > 1 || abs(spring) > 0.001 || abs(springVelocity) > 0.01 || greeting > 0 }

    mutating func advance(position: inout CGPoint, radius: CGFloat, floor: CGFloat, width: CGFloat, dt elapsed: CGFloat) {
        guard elapsed.isFinite, elapsed > 0 else { return }
        let duration = min(elapsed, 0.1)
        let steps = max(1, Int(ceil(duration * 120)))
        let dt = duration / CGFloat(steps)
        greeting = max(0, greeting - duration)
        for _ in 0..<steps {
            integrate(position: &position, radius: radius, floor: floor, width: width, dt: dt)
        }
    }

    private mutating func integrate(position: inout CGPoint, radius: CGFloat, floor: CGFloat, width: CGFloat, dt: CGFloat) {
        let grounded = position.y <= floor + 0.5 && velocity.y <= 0
        if grounded {
            position.y = floor
            velocity.y = 0
            velocity.x *= exp(-7 * dt)
            if abs(velocity.x) < 1 { velocity.x = 0 }
        } else {
            velocity.y -= 980 * dt
        }
        position.x += velocity.x * dt
        position.y += velocity.y * dt
        if position.y < floor {
            position.y = floor
            let impact = abs(velocity.y)
            velocity.y = impact > 70 ? impact * 0.16 : 0
            springVelocity -= min(2.2, impact / 350)
        }
        if position.x < radius + 4 {
            position.x = radius + 4; velocity.x = abs(velocity.x) * 0.3
        } else if position.x > width - radius - 4 {
            position.x = max(radius + 4, width - radius - 4); velocity.x = -abs(velocity.x) * 0.3
        }
        rotation += velocity.x * dt / max(radius, 1)
        springVelocity += (-150 * spring - 17 * springVelocity) * dt
        spring += springVelocity * dt
        spring = min(0.14, max(-0.14, spring))
        if abs(spring) < 0.001 && abs(springVelocity) < 0.01 { spring = 0; springVelocity = 0 }
    }
}

struct SnowmanSupport {
    let id: ObjectIdentifier
    let origin: CGPoint
    let height: CGFloat
}

/// Estimate a flick over a short stroke, rather than amplifying the final trackpad event.
struct SnowmanThrow {
    private var samples: [(point: CGPoint, time: TimeInterval)] = []
    mutating func record(_ point: CGPoint, at time: TimeInterval) {
        guard time.isFinite, point.x.isFinite, point.y.isFinite else { return }
        if let last = samples.last, time <= last.time { return }
        samples.append((point, time))
        samples.removeAll { time - $0.time > 0.1 }
        if samples.count > 32 { samples.removeFirst(samples.count - 32) }
    }
    func velocity(at time: TimeInterval) -> CGPoint {
        guard let first = samples.first, let last = samples.last, samples.count >= 2,
              time >= last.time, time - last.time < 0.12 else { return .zero }
        let interval = last.time - first.time
        guard interval >= 1.0 / 120 else { return .zero }
        let fade = max(0, 1 - (time - last.time) / 0.12)
        var result = CGPoint(x: (last.point.x - first.point.x) / interval * fade,
                             y: (last.point.y - first.point.y) / interval * fade)
        let speed = hypot(result.x, result.y)
        if speed > 900 { result.x *= 900 / speed; result.y *= 900 / speed }
        return result
    }
}

/// Circle contact with mass-weighted separation and a damped impulse. Stacks share one body mass.
enum SnowmanContact {
    struct Body {
        var center: CGPoint
        var velocity: CGPoint
        var radius: CGFloat
        var mass: CGFloat
        var held = false
    }
    @discardableResult
    static func resolve(_ a: inout Body, _ b: inout Body) -> CGFloat? {
        let dx = b.center.x - a.center.x, dy = b.center.y - a.center.y
        let distance = hypot(dx, dy), reach = a.radius + b.radius
        guard distance < reach else { return nil }
        let normal = distance > 0.0001 ? CGPoint(x: dx / distance, y: dy / distance) : CGPoint(x: 1, y: 0)
        let inverseA: CGFloat = a.held ? 0 : 1 / max(1, a.mass)
        let inverseB: CGFloat = b.held ? 0 : 1 / max(1, b.mass)
        let inverse = inverseA + inverseB
        guard inverse > 0 else { return nil }
        let correction = max(0, reach - distance - 0.05) / inverse
        a.center.x -= normal.x * correction * inverseA
        a.center.y -= normal.y * correction * inverseA
        b.center.x += normal.x * correction * inverseB
        b.center.y += normal.y * correction * inverseB
        let closing = (b.velocity.x - a.velocity.x) * normal.x + (b.velocity.y - a.velocity.y) * normal.y
        guard closing < 0 else { return 0 }
        let impulse = -closing * 1.22 / inverse
        a.velocity.x -= normal.x * impulse * inverseA
        a.velocity.y -= normal.y * impulse * inverseA
        b.velocity.x += normal.x * impulse * inverseB
        b.velocity.y += normal.y * impulse * inverseB
        return -closing
    }
}
