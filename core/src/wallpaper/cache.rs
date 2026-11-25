//! What was found in a wallpaper, kept on disk so that a picture is read once: a few screens' worth of planes,
//! deflated, and the ridges and numbers as they are.

use super::{Light, Relief, Ridge, Scene, Terrain};

/// Raise whenever what is found changes, so pictures already done are looked at again.
pub const VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"SNOWWALL";

/// A file name for what was found in a picture, from all that tells it apart: its path, its size and date, how it is
/// laid out, the column width.
pub fn name(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in parts.join("|").bytes().chain(VERSION.to_le_bytes()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}.scene")
}

impl Scene {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer(MAGIC.to_vec());
        out.u32(VERSION);
        let relief = &self.relief;
        out.sizes(relief.width, relief.height, relief.cell);
        for plane in [&relief.lightness, &relief.tops, &relief.hold, &relief.near] {
            out.plane(plane);
        }
        let terrain = &self.terrain;
        out.sizes(terrain.width, terrain.height, terrain.cell);
        out.plane(&terrain.near);
        out.f32(terrain.far);
        out.f32(terrain.close);
        let light = &self.light;
        out.sizes(light.width, light.height, light.cell);
        out.plane(&light.pixels);
        light.haze.iter().for_each(|value| out.f32(*value));
        out.u32(self.ridges.len() as u32);
        for ridge in &self.ridges {
            out.f32(ridge.left);
            out.f32(ridge.nearness);
            for values in [&ridge.tops, &ridge.room, &ridge.mass] {
                out.u32(values.len() as u32);
                values.iter().for_each(|value| out.f32(*value));
            }
        }
        out.0
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Scene> {
        let mut input = Reader(bytes.strip_prefix(MAGIC.as_slice())?);
        if input.u32()? != VERSION {
            return None;
        }
        let (width, height, cell) = input.sizes()?;
        let count = width * height;
        let relief = Relief {
            width,
            height,
            cell,
            lightness: input.plane(count)?,
            tops: input.plane(count)?,
            hold: input.plane(count)?,
            near: input.plane(count)?,
        };
        let (width, height, cell) = input.sizes()?;
        let terrain = Terrain { width, height, cell, near: input.plane(width * height)?, far: input.f32()?, close: input.f32()? };
        let (width, height, cell) = input.sizes()?;
        let light = Light { width, height, cell, pixels: input.plane(width * height * 4)?, haze: [input.f32()?, input.f32()?, input.f32()?] };
        let mut ridges = Vec::new();
        for _ in 0..input.u32()? {
            let (left, nearness) = (input.f32()?, input.f32()?);
            let mut lists = Vec::new();
            for _ in 0..3 {
                let count = input.u32()? as usize;
                lists.push((0..count).map(|_| input.f32()).collect::<Option<Vec<f32>>>()?);
            }
            let mass = lists.pop()?;
            let room = lists.pop()?;
            let tops = lists.pop()?;
            ridges.push(Ridge { left, tops, nearness, room, mass });
        }
        Some(Scene { ridges, light, terrain, relief })
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, value: u32) {
        self.0.extend(value.to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.0.extend(value.to_le_bytes());
    }

    fn sizes(&mut self, width: usize, height: usize, cell: usize) {
        [width, height, cell].iter().for_each(|value| self.u32(*value as u32));
    }

    fn plane(&mut self, plane: &[u8]) {
        let packed = miniz_oxide::deflate::compress_to_vec(plane, 6);
        self.u32(packed.len() as u32);
        self.0.extend(packed);
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Option<&[u8]> {
        if self.0.len() < count {
            return None;
        }
        let (taken, rest) = self.0.split_at(count);
        self.0 = rest;
        Some(taken)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn sizes(&mut self) -> Option<(usize, usize, usize)> {
        Some((self.u32()? as usize, self.u32()? as usize, self.u32()? as usize))
    }

    fn plane(&mut self, count: usize) -> Option<Vec<u8>> {
        let length = self.u32()? as usize;
        let plane = miniz_oxide::inflate::decompress_to_vec_with_limit(self.take(length)?, count).ok()?;
        (plane.len() == count).then_some(plane)
    }
}
