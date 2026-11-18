// Depth Anything V2 Small's forward pass, after candle-transformers' dinov2 and depth_anything_v2 (MIT OR Apache-2.0)
// with what the published network does that candle's doesn't: an input that isn't square, its position embedding
// resampled bicubically, bilinear upsampling with aligned corners, layer norms at 1e-6 and the exact GELU.

use candle_core::{D, IndexOp, Module, Result, Tensor};
use candle_nn::{
    Conv2d, Conv2dConfig, ConvTranspose2d, ConvTranspose2dConfig, LayerNorm, Linear, VarBuilder, conv2d, conv2d_no_bias, layer_norm, linear,
};

const PATCH: usize = 14;
const DIM: usize = 384;
const HEADS: usize = 6;
const DEPTH: usize = 12;
/// The blocks whose output the depth head is built from.
const TAKEN: [usize; 4] = [2, 5, 8, 11];
const FEATURES: usize = 64;
const CHANNELS: [usize; 4] = [48, 96, 192, 384];
/// The patches the position embedding was learned for, across and down.
const LEARNED: usize = 37;
/// Rows the last convolutions are done in at a time: candle unfolds a convolution's whole input first, which for these,
/// at the picture's full size, would take hundreds of megabytes.
const STRIP: usize = 64;

struct Attention {
    qkv: Linear,
    proj: Linear,
}

impl Attention {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let (b, n, c) = xs.dims3()?;
        let qkv = self.qkv.forward(xs)?.reshape((b, n, 3, HEADS, c / HEADS))?.permute((2, 0, 3, 1, 4))?;
        let q = (qkv.i(0)? * (1.0 / ((c / HEADS) as f64).sqrt()))?;
        let k = qkv.i(1)?.contiguous()?;
        let v = qkv.i(2)?.contiguous()?;
        let attention = candle_nn::ops::softmax_last_dim(&q.contiguous()?.matmul(&k.t()?)?)?;
        self.proj.forward(&attention.matmul(&v)?.transpose(1, 2)?.reshape((b, n, c))?)
    }
}

struct Block {
    norm1: LayerNorm,
    attention: Attention,
    scale1: Tensor,
    norm2: LayerNorm,
    fc1: Linear,
    fc2: Linear,
    scale2: Tensor,
}

impl Block {
    fn new(vb: VarBuilder) -> Result<Block> {
        Ok(Block {
            norm1: layer_norm(DIM, 1e-6, vb.pp("norm1"))?,
            attention: Attention { qkv: linear(DIM, DIM * 3, vb.pp("attn.qkv"))?, proj: linear(DIM, DIM, vb.pp("attn.proj"))? },
            scale1: vb.get(DIM, "ls1.gamma")?,
            norm2: layer_norm(DIM, 1e-6, vb.pp("norm2"))?,
            fc1: linear(DIM, DIM * 4, vb.pp("mlp.fc1"))?,
            fc2: linear(DIM * 4, DIM, vb.pp("mlp.fc2"))?,
            scale2: vb.get(DIM, "ls2.gamma")?,
        })
    }

    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let xs = (self.attention.forward(&self.norm1.forward(xs)?)?.broadcast_mul(&self.scale1)? + xs)?;
        let mlp = self.fc2.forward(&self.fc1.forward(&self.norm2.forward(&xs)?)?.gelu_erf()?)?;
        mlp.broadcast_mul(&self.scale2)? + xs
    }
}

struct Unit {
    conv1: Conv2d,
    conv2: Conv2d,
}

impl Unit {
    fn new(vb: VarBuilder) -> Result<Unit> {
        let same = Conv2dConfig { padding: 1, ..Default::default() };
        Ok(Unit { conv1: conv2d(FEATURES, FEATURES, 3, same, vb.pp("conv1"))?, conv2: conv2d(FEATURES, FEATURES, 3, same, vb.pp("conv2"))? })
    }

    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        self.conv2.forward(&self.conv1.forward(&xs.relu()?)?.relu()?)? + xs
    }
}

struct Fusion {
    unit1: Unit,
    unit2: Unit,
    out: Conv2d,
}

impl Fusion {
    fn new(vb: VarBuilder) -> Result<Fusion> {
        Ok(Fusion {
            unit1: Unit::new(vb.pp("resConfUnit1"))?,
            unit2: Unit::new(vb.pp("resConfUnit2"))?,
            out: conv2d(FEATURES, FEATURES, 1, Default::default(), vb.pp("out_conv"))?,
        })
    }

    /// `skip` added in first when there is one, and the result brought to `size`, height and width.
    fn forward(&self, xs: &Tensor, skip: Option<&Tensor>, size: (usize, usize)) -> Result<Tensor> {
        let xs = match skip {
            Some(skip) => (xs + self.unit1.forward(skip)?)?,
            None => xs.clone(),
        };
        let xs = bilinear(&self.unit2.forward(&xs)?, size.0, size.1)?;
        self.out.forward(&xs)
    }
}

pub struct Network {
    patch: Conv2d,
    class: Tensor,
    /// The class token's position and the patches' as learned, 37 by 37, each `DIM` long.
    position: (Tensor, Vec<f32>),
    blocks: Vec<Block>,
    norm: LayerNorm,
    projections: Vec<Conv2d>,
    grow4: ConvTranspose2d,
    grow2: ConvTranspose2d,
    shrink: Conv2d,
    layers: Vec<Conv2d>,
    fusions: Vec<Fusion>,
    output1: Conv2d,
    output2: (Conv2d, Conv2d),
}

impl Network {
    pub fn new(vb: VarBuilder) -> Result<Network> {
        let encoder = vb.pp("pretrained");
        let position = encoder.get((1, 1 + LEARNED * LEARNED, DIM), "pos_embed")?;
        let patches: Vec<f32> = position.i((0, 1..))?.flatten_all()?.to_vec1()?;
        let head = vb.pp("depth_head");
        let same = Conv2dConfig { padding: 1, ..Default::default() };
        let grow = |factor: usize, k: usize| {
            let config = ConvTranspose2dConfig { padding: 0, stride: factor, dilation: 1, output_padding: 0 };
            candle_nn::conv_transpose2d(CHANNELS[k], CHANNELS[k], factor, config, head.pp(format!("resize_layers.{k}")))
        };
        let scratch = head.pp("scratch");
        Ok(Network {
            patch: conv2d(3, DIM, PATCH, Conv2dConfig { stride: PATCH, ..Default::default() }, encoder.pp("patch_embed.proj"))?,
            class: encoder.get((1, 1, DIM), "cls_token")?,
            position: (position.i((.., ..1))?, patches),
            blocks: (0..DEPTH).map(|k| Block::new(encoder.pp(format!("blocks.{k}")))).collect::<Result<_>>()?,
            norm: layer_norm(DIM, 1e-6, encoder.pp("norm"))?,
            projections: (0..4).map(|k| conv2d(DIM, CHANNELS[k], 1, Default::default(), head.pp(format!("projects.{k}")))).collect::<Result<_>>()?,
            grow4: grow(4, 0)?,
            grow2: grow(2, 1)?,
            shrink: conv2d(CHANNELS[3], CHANNELS[3], 3, Conv2dConfig { padding: 1, stride: 2, ..Default::default() }, head.pp("resize_layers.3"))?,
            layers: (0..4)
                .map(|k| conv2d_no_bias(CHANNELS[k], FEATURES, 3, same, scratch.pp(format!("layer{}_rn", k + 1))))
                .collect::<Result<_>>()?,
            fusions: (0..4).map(|k| Fusion::new(scratch.pp(format!("refinenet{}", k + 1)))).collect::<Result<_>>()?,
            output1: conv2d(FEATURES, FEATURES / 2, 3, same, scratch.pp("output_conv1"))?,
            output2: (
                conv2d(FEATURES / 2, 32, 3, same, scratch.pp("output_conv2.0"))?,
                conv2d(32, 1, 1, Default::default(), scratch.pp("output_conv2.2"))?,
            ),
        })
    }

    /// The position embedding for a grid `down` by `across` patches: the learned one resampled bicubically, as the
    /// published network does, by the grid's size and a tenth of a patch over the learned one.
    fn positions(&self, down: usize, across: usize, device: &candle_core::Device) -> Result<Tensor> {
        let patches = if down == LEARNED && across == LEARNED {
            self.position.1.clone()
        } else {
            bicubic(&self.position.1, LEARNED, DIM, (down as f64 + 0.1) / LEARNED as f64, (across as f64 + 0.1) / LEARNED as f64, down, across)
        };
        let patches = Tensor::from_vec(patches, (1, down * across, DIM), device)?;
        Tensor::cat(&[&self.position.0, &patches], 1)
    }

    /// The depth of a picture as RGB planes, `height` by `width` pixels, both whole patches: the larger, the nearer.
    pub fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let (b, _, height, width) = xs.dims4()?;
        let (down, across) = (height / PATCH, width / PATCH);
        let tokens = self.patch.forward(xs)?.flatten_from(2)?.transpose(1, 2)?;
        let mut tokens = (Tensor::cat(&[&self.class.expand((b, 1, DIM))?, &tokens], 1)? + self.positions(down, across, xs.device())?)?;
        let mut taken = Vec::with_capacity(4);
        for (k, block) in self.blocks.iter().enumerate() {
            tokens = block.forward(&tokens)?;
            if TAKEN.contains(&k) {
                taken.push(self.norm.forward(&tokens)?.i((.., 1..))?);
            }
        }
        let mut layers = Vec::with_capacity(4);
        for (k, features) in taken.iter().enumerate() {
            let features = features.transpose(1, 2)?.reshape((b, DIM, down, across))?;
            let projected = self.projections[k].forward(&features)?;
            let resized = match k {
                0 => self.grow4.forward(&projected)?,
                1 => self.grow2.forward(&projected)?,
                2 => projected,
                _ => self.shrink.forward(&projected)?,
            };
            layers.push(self.layers[k].forward(&resized)?);
        }
        let size = |layer: &Tensor| -> Result<(usize, usize)> { Ok((layer.dim(D::Minus2)?, layer.dim(D::Minus1)?)) };
        let path = self.fusions[3].forward(&layers[3], None, size(&layers[2])?)?;
        let path = self.fusions[2].forward(&path, Some(&layers[2]), size(&layers[1])?)?;
        let path = self.fusions[1].forward(&path, Some(&layers[1]), size(&layers[0])?)?;
        let (h, w) = size(&layers[0])?;
        let path = self.fusions[0].forward(&path, Some(&layers[0]), (h * 2, w * 2))?;
        let out = bilinear(&in_strips(&path, |xs| self.output1.forward(xs))?, height, width)?;
        in_strips(&out, |xs| self.output2.1.forward(&self.output2.0.forward(xs)?.relu()?)?.relu())
    }
}

/// `convolve`, a 3 by 3 convolution padded by one and what follows it pixel by pixel, done a strip of rows at a time,
/// each given the row either side of it.
fn in_strips(xs: &Tensor, convolve: impl Fn(&Tensor) -> Result<Tensor>) -> Result<Tensor> {
    let height = xs.dim(2)?;
    let mut strips = Vec::with_capacity(height.div_ceil(STRIP));
    for top in (0..height).step_by(STRIP) {
        let bottom = (top + STRIP).min(height);
        let (from, to) = (top.saturating_sub(1), (bottom + 1).min(height));
        let out = convolve(&xs.narrow(2, from, to - from)?.contiguous()?)?;
        strips.push(out.narrow(2, top - from, bottom - top)?);
    }
    Tensor::cat(&strips, 2)
}

/// Bilinear resizing of each plane to `height` by `width`, corners aligned, as PyTorch's: done on the CPU, which candle
/// 0.9 can't, and later versions only with a tokenizer library built in.
fn bilinear(xs: &Tensor, height: usize, width: usize) -> Result<Tensor> {
    let (b, c, h, w) = xs.dims4()?;
    let values: Vec<f32> = xs.flatten_all()?.to_vec1()?;
    let along = |from: usize, to: usize| -> Vec<(usize, usize, f32)> {
        let scale = if to > 1 { (from - 1) as f32 / (to - 1) as f32 } else { 0.0 };
        (0..to)
            .map(|k| {
                let at = k as f32 * scale;
                let first = (at as usize).min(from - 1);
                (first, (first + 1).min(from - 1), at - first as f32)
            })
            .collect()
    };
    let (rows, columns) = (along(h, height), along(w, width));
    let mut out = vec![0f32; b * c * height * width];
    for (plane, target) in values.chunks_exact(h * w).zip(out.chunks_exact_mut(height * width)) {
        for (y, &(y0, y1, ty)) in rows.iter().enumerate() {
            let (top, bottom) = (&plane[y0 * w..(y0 + 1) * w], &plane[y1 * w..(y1 + 1) * w]);
            for (x, &(x0, x1, tx)) in columns.iter().enumerate() {
                let upper = top[x0] + (top[x1] - top[x0]) * tx;
                let lower = bottom[x0] + (bottom[x1] - bottom[x0]) * tx;
                target[y * width + x] = upper + (lower - upper) * ty;
            }
        }
    }
    Tensor::from_vec(out, (b, c, height, width), xs.device())
}

/// PyTorch's bicubic resampling of a `side` by `side` grid of `dim`-long values, row by row, by `scale_down` and
/// `scale_across` to `down` by `across`: corners not aligned, the cubic's A at -0.75, edges clamped.
fn bicubic(grid: &[f32], side: usize, dim: usize, scale_down: f64, scale_across: f64, down: usize, across: usize) -> Vec<f32> {
    let a = -0.75f64;
    let near = |x: f64| ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0;
    let far = |x: f64| ((a * x - 5.0 * a) * x + 8.0 * a) * x - 4.0 * a;
    let weights = |out: usize, scale: f64| -> (isize, [f64; 4]) {
        let real = (out as f64 + 0.5) / scale - 0.5;
        let first = real.floor();
        let t = real - first;
        (first as isize, [far(t + 1.0), near(t), near(1.0 - t), far(2.0 - t)])
    };
    let clamp = |i: isize| i.clamp(0, side as isize - 1) as usize;
    let mut out = vec![0f32; down * across * dim];
    for y in 0..down {
        let (y0, wy) = weights(y, scale_down);
        for x in 0..across {
            let (x0, wx) = weights(x, scale_across);
            let target = &mut out[(y * across + x) * dim..(y * across + x + 1) * dim];
            for (j, wy) in wy.iter().enumerate() {
                let row = clamp(y0 - 1 + j as isize);
                for (i, wx) in wx.iter().enumerate() {
                    let column = clamp(x0 - 1 + i as isize);
                    let weight = (wy * wx) as f32;
                    let source = &grid[(row * side + column) * dim..(row * side + column + 1) * dim];
                    for (value, from) in target.iter_mut().zip(source) {
                        *value += weight * from;
                    }
                }
            }
        }
    }
    out
}
