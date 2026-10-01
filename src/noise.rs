//! Ruido procedural: texturas, terreno y oleaje.
//!
//! Las funciones de agua (`sea_octave`, `water_height`) son una adaptación a Rust
//! del shader "Seascape" de TDM, usado como referencia para el oleaje.

use crate::vec3::Vec3;

/// Hash entero determinista (mezcla tipo xorshift-multiply).
pub fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Valor pseudoaleatorio en 0..1 a partir de dos enteros y una semilla.
pub fn hash_2i(x: i32, y: i32, seed: u32) -> f32 {
    let h = hash_u32(
        (x as u32)
            .wrapping_mul(0x9e37_79b9)
            .wrapping_add((y as u32).wrapping_mul(0x85eb_ca6b))
            .wrapping_add(seed.wrapping_mul(0xc2b2_ae35)),
    );
    (h >> 8) as f32 / 16_777_216.0
}

/// Valor pseudoaleatorio en 0..1 a partir de tres enteros y una semilla.
pub fn hash_3i(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let h = hash_u32(
        (x as u32)
            .wrapping_mul(0x9e37_79b9)
            .wrapping_add((y as u32).wrapping_mul(0x85eb_ca6b))
            .wrapping_add((z as u32).wrapping_mul(0xc2b2_ae35))
            .wrapping_add(seed.wrapping_mul(0x27d4_eb2f)),
    );
    (h >> 8) as f32 / 16_777_216.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Ruido de valor suavizado en 0..1.
pub fn value_noise_2d(x: f32, y: f32, seed: u32) -> f32 {
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let (ux, uy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (i, j) = (fx as i32, fy as i32);

    let a = hash_2i(i, j, seed);
    let b = hash_2i(i + 1, j, seed);
    let c = hash_2i(i, j + 1, seed);
    let d = hash_2i(i + 1, j + 1, seed);

    lerp(lerp(a, b, ux), lerp(c, d, ux), uy)
}

pub fn value_noise_3d(p: Vec3, seed: u32) -> f32 {
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (tx, ty, tz) = (p.x - fx, p.y - fy, p.z - fz);
    let ux = tx * tx * (3.0 - 2.0 * tx);
    let uy = ty * ty * (3.0 - 2.0 * ty);
    let uz = tz * tz * (3.0 - 2.0 * tz);
    let (i, j, k) = (fx as i32, fy as i32, fz as i32);

    let c = |dx: i32, dy: i32, dz: i32| hash_3i(i + dx, j + dy, k + dz, seed);
    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), ux);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), ux);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), ux);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), ux);

    lerp(lerp(x00, x10, uy), lerp(x01, x11, uy), uz)
}

/// Suma de octavas (fractal brownian motion) en 0..1.
pub fn fbm_2d(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut freq, mut amp, mut sum, mut norm) = (1.0, 0.5, 0.0, 0.0);
    for _ in 0..octaves {
        sum += value_noise_2d(x * freq, y * freq, seed) * amp;
        norm += amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    sum / norm
}

pub fn fbm_3d(p: Vec3, octaves: u32, seed: u32) -> f32 {
    let (mut freq, mut amp, mut sum, mut norm) = (1.0, 0.5, 0.0, 0.0);
    for _ in 0..octaves {
        sum += value_noise_3d(p * freq, seed) * amp;
        norm += amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    sum / norm
}

// ---------------------------------------------------------------------------
// Oleaje (adaptado del shader Seascape)
// ---------------------------------------------------------------------------

const SEA_CHOPPY: f32 = 4.0;
const SEA_FREQ: f32 = 0.30;
const SEA_HEIGHT: f32 = 0.11;
const SEA_SPEED: f32 = 0.8;
const ITER_GEOMETRY: u32 = 4;

fn sea_hash(x: f32, y: f32) -> f32 {
    let h = x * 127.1 + y * 311.7;
    (h.sin() * 43758.5453123).fract().abs()
}

/// Ruido del shader original, en el rango -1..1.
fn sea_noise(x: f32, y: f32) -> f32 {
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let (ux, uy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));

    let a = sea_hash(fx, fy);
    let b = sea_hash(fx + 1.0, fy);
    let c = sea_hash(fx, fy + 1.0);
    let d = sea_hash(fx + 1.0, fy + 1.0);

    -1.0 + 2.0 * lerp(lerp(a, b, ux), lerp(c, d, ux), uy)
}

fn sea_octave(mut ux: f32, mut uy: f32, choppy: f32) -> f32 {
    let n = sea_noise(ux, uy);
    ux += n;
    uy += n;

    let (wx, wy) = (1.0 - ux.sin().abs(), 1.0 - uy.sin().abs());
    let (sx, sy) = (ux.cos().abs(), uy.cos().abs());
    // mix(wv, swv, wv)
    let wx = wx + (sx - wx) * wx;
    let wy = wy + (sy - wy) * wy;

    (1.0 - (wx * wy).powf(0.65)).powf(choppy)
}

/// Altura de la ola en el punto (x, z) para un instante dado.
pub fn water_height(x: f32, z: f32, time: f32) -> f32 {
    let (mut freq, mut amp, mut choppy) = (SEA_FREQ, SEA_HEIGHT, SEA_CHOPPY);
    let (mut ux, mut uy) = (x * 0.75, z);
    let t = 1.0 + time * SEA_SPEED;
    let mut height = 0.0;

    for _ in 0..ITER_GEOMETRY {
        let d = sea_octave((ux + t) * freq, (uy + t) * freq, choppy)
            + sea_octave((ux - t) * freq, (uy - t) * freq, choppy);
        height += d * amp;

        // uv *= octave_m
        let (nx, ny) = (ux * 1.6 + uy * 1.2, ux * -1.2 + uy * 1.6);
        ux = nx;
        uy = ny;
        freq *= 1.9;
        amp *= 0.22;
        choppy = choppy + (1.0 - choppy) * 0.2;
    }

    height
}

/// Normal de la superficie del agua por diferencias finitas.
pub fn water_normal(x: f32, z: f32, time: f32) -> Vec3 {
    const EPS: f32 = 0.08;
    let h = water_height(x, z, time);
    let dx = water_height(x + EPS, z, time) - h;
    let dz = water_height(x, z + EPS, time) - h;

    Vec3::new(-dx / EPS, 1.0, -dz / EPS).normalize()
}
