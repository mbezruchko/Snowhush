use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

impl V2 {
    pub const ZERO: V2 = V2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> V2 {
        V2 { x, y }
    }

    pub fn dot(self, other: V2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, other: V2) -> V2 {
        V2::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, other: V2) -> V2 {
        V2::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for V2 {
    type Output = V2;
    fn mul(self, k: f32) -> V2 {
        V2::new(self.x * k, self.y * k)
    }
}

impl Div<f32> for V2 {
    type Output = V2;
    fn div(self, k: f32) -> V2 {
        V2::new(self.x / k, self.y / k)
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        V2::new(-self.x, -self.y)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, other: V2) {
        *self = *self + other;
    }
}

impl SubAssign for V2 {
    fn sub_assign(&mut self, other: V2) {
        *self = *self - other;
    }
}

impl MulAssign<f32> for V2 {
    fn mul_assign(&mut self, k: f32) {
        *self = *self * k;
    }
}

/// A rectangle in overlay coordinates: y points up from the bottom of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn min_x(&self) -> f32 {
        self.x
    }

    pub fn max_x(&self) -> f32 {
        self.x + self.width
    }

    pub fn min_y(&self) -> f32 {
        self.y
    }

    pub fn max_y(&self) -> f32 {
        self.y + self.height
    }
}

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
