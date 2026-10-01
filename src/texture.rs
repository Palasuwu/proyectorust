//! Texturas procedurales estilo pixel-art: cada cara de bloque se divide en
//! una cuadrícula de 16x16 "texels" y el color se genera con hash + ruido.

use crate::material::{Kind, material_of};
use crate::noise::{fbm_2d, hash_2i, hash_3i, value_noise_2d};
use crate::vec3::Vec3;

/// Resolución de la textura por cara del bloque.
const TEX: f32 = 16.0;

/// Devuelve las coordenadas de textura (u, v) de la cara golpeada, el índice de
/// texel global y el eje dominante de la normal.
fn face_uv(point: Vec3, normal: Vec3) -> (f32, f32, u32) {
    if normal.x.abs() > 0.5 {
        (point.z, point.y, 0)
    } else if normal.y.abs() > 0.5 {
        (point.x, point.z, 1)
    } else {
        (point.x, point.y, 2)
    }
}

/// Color de la textura en el punto de impacto.
pub fn sample(id: u8, point: Vec3, normal: Vec3, cell: [i32; 3]) -> Vec3 {
    let kind = Kind::from_id(id);
    let base = material_of(id).base;
    let (u, v, axis) = face_uv(point, normal);

    // Texel global: continuo entre bloques vecinos de la misma superficie.
    let tu = (u * TEX).floor() as i32;
    let tv = (v * TEX).floor() as i32;
    // Coordenada dentro de la cara (0..15), para detalles por bloque.
    let lu = tu.rem_euclid(TEX as i32);
    let lv = tv.rem_euclid(TEX as i32);

    let seed = id as u32 * 977 + axis * 31;
    let grain = hash_2i(tu, tv, seed);

    match kind {
        Kind::Grass => {
            let blades = fbm_2d(tu as f32 * 0.22, tv as f32 * 0.22, 2, 7);
            let dark = if grain < 0.14 { -0.12 } else { 0.0 };
            let up = if normal.y > 0.5 { 0.0 } else { -0.18 };
            // Manchones amplios de pasto más claro y más seco.
            let patch = fbm_2d(point.x * 0.05, point.z * 0.05, 3, 19);
            let tint = Vec3::new(0.38, 0.46, 0.16);
            let color = base + (tint - base) * ((patch - 0.45) * 1.6).clamp(0.0, 0.7);
            color * (0.82 + blades * 0.38 + dark + up)
        }
        Kind::Dirt => {
            let speck = if grain < 0.10 { 0.22 } else if grain > 0.92 { -0.14 } else { 0.0 };
            base * (0.85 + grain * 0.22 + speck)
        }
        Kind::Stone => {
            let mottle = fbm_2d(tu as f32 * 0.18, tv as f32 * 0.18, 3, 11);
            let crack = if mottle < 0.30 { -0.24 } else { 0.0 };
            base * (0.80 + mottle * 0.40 + crack)
        }
        Kind::Sand => {
            let ripple = value_noise_2d(tu as f32 * 0.35, tv as f32 * 0.35, 13);
            base * (0.90 + ripple * 0.18 + grain * 0.06)
        }
        Kind::Plank => {
            // Tablas separadas cada 5 texels, con vetas a lo largo.
            let plank_id = tv.div_euclid(5);
            let joint = tv.rem_euclid(5) == 0;
            let vein = value_noise_2d(tu as f32 * 0.30, plank_id as f32 * 4.0, 17);
            let tone = 0.82 + hash_2i(plank_id, 0, 23) * 0.22 + vein * 0.16;
            base * if joint { tone * 0.62 } else { tone }
        }
        Kind::Log => {
            if normal.y.abs() > 0.5 {
                // Anillos en las tapas del tronco.
                let du = lu as f32 - 7.5;
                let dv = lv as f32 - 7.5;
                let r = (du * du + dv * dv).sqrt();
                let ring = (r * 1.6).sin() * 0.5 + 0.5;
                Vec3::new(0.62, 0.48, 0.32) * (0.78 + ring * 0.28)
            } else {
                // Corteza: franjas verticales irregulares.
                let bark = value_noise_2d(tu as f32 * 0.8, tv as f32 * 0.14, 29);
                base * (0.74 + bark * 0.50)
            }
        }
        Kind::LeafGreen | Kind::LeafPink | Kind::LeafOrange => {
            let clump = hash_3i(cell[0] * 16 + lu, cell[1] * 16 + lv, cell[2], seed);
            let shade = fbm_2d(tu as f32 * 0.4, tv as f32 * 0.4, 2, 31);
            base * (0.70 + shade * 0.45 + if clump < 0.18 { -0.22 } else { 0.06 })
        }
        Kind::Water => base * (0.9 + grain * 0.12),
        Kind::Glass => {
            // Marco del vidrio en el borde de la cara.
            let border = lu == 0 || lv == 0 || lu == 15 || lv == 15;
            if border { base * 0.55 } else { base }
        }
        Kind::Lantern => {
            let glow = ((lu + lv) % 4 == 0) as i32 as f32;
            base * (0.85 + glow * 0.15)
        }
        Kind::Roof => {
            // Tejas: filas escalonadas.
            let row = tv.div_euclid(4);
            let offset = if row % 2 == 0 { 0 } else { 2 };
            let tile = (tu + offset).rem_euclid(4);
            let edge = tv.rem_euclid(4) == 0 || tile == 0;
            base * if edge { 0.66 } else { 0.90 + grain * 0.16 }
        }
        Kind::Flower => {
            let petal = hash_2i(tu, tv, 37);
            if petal > 0.55 { base * (0.9 + petal * 0.2) } else { Vec3::new(0.35, 0.55, 0.28) }
        }
        Kind::Air => base,
    }
}
