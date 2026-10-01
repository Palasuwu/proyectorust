//! Skybox procedural: seis texturas de cubo generadas una sola vez y
//! muestreadas por dirección con interpolación bilineal.

use crate::noise::fbm_3d;
use crate::vec3::Vec3;

/// Ejes de cada cara: (eje principal, signo, eje u, eje v).
const FACES: [(usize, f32, usize, usize); 6] =
    [(0, 1.0, 2, 1), (0, -1.0, 2, 1), (1, 1.0, 0, 2), (1, -1.0, 0, 2), (2, 1.0, 0, 1), (2, -1.0, 0, 1)];

fn set_axis(v: &mut Vec3, i: usize, value: f32) {
    match i {
        0 => v.x = value,
        1 => v.y = value,
        _ => v.z = value,
    }
}

fn get_axis(v: Vec3, i: usize) -> f32 {
    match i {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

fn face_direction(face: usize, u: f32, v: f32) -> Vec3 {
    let (main, sign, ua, va) = FACES[face];
    let mut dir = Vec3::default();
    set_axis(&mut dir, main, sign);
    set_axis(&mut dir, ua, 2.0 * u - 1.0);
    set_axis(&mut dir, va, 2.0 * v - 1.0);
    dir.normalize()
}

pub struct Skybox {
    size: usize,
    faces: Vec<Vec<Vec3>>,
}

impl Skybox {
    /// Genera las seis caras a partir de la dirección 3D, así la textura queda
    /// continua en los bordes del cubo.
    pub fn generate(size: usize, sun_dir: Vec3) -> Self {
        let mut faces = Vec::with_capacity(6);
        for face in 0..6 {
            let mut pixels = vec![Vec3::default(); size * size];
            for y in 0..size {
                for x in 0..size {
                    let u = (x as f32 + 0.5) / size as f32;
                    let v = (y as f32 + 0.5) / size as f32;
                    pixels[y * size + x] = sky_color(face_direction(face, u, v), sun_dir);
                }
            }
            faces.push(pixels);
        }
        Self { size, faces }
    }

    pub fn sample(&self, dir: Vec3) -> Vec3 {
        let d = dir.normalize();
        // Cara dominante del cubo.
        let (mut main, mut best) = (0usize, d.x.abs());
        if d.y.abs() > best {
            main = 1;
            best = d.y.abs();
        }
        if d.z.abs() > best {
            main = 2;
            best = d.z.abs();
        }
        let sign = if get_axis(d, main) > 0.0 { 1.0 } else { -1.0 };
        let face = FACES
            .iter()
            .position(|&(m, s, _, _)| m == main && s == sign)
            .unwrap_or(0);
        let (_, _, ua, va) = FACES[face];

        let u = (get_axis(d, ua) / best + 1.0) * 0.5;
        let v = (get_axis(d, va) / best + 1.0) * 0.5;

        self.bilinear(face, u, v)
    }

    fn bilinear(&self, face: usize, u: f32, v: f32) -> Vec3 {
        let n = self.size as f32;
        let fx = (u * n - 0.5).clamp(0.0, n - 1.0);
        let fy = (v * n - 0.5).clamp(0.0, n - 1.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.size - 1), (y0 + 1).min(self.size - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);

        let px = &self.faces[face];
        let top = px[y0 * self.size + x0] * (1.0 - tx) + px[y0 * self.size + x1] * tx;
        let bot = px[y1 * self.size + x0] * (1.0 - tx) + px[y1 * self.size + x1] * tx;

        top * (1.0 - ty) + bot * ty
    }
}

/// Color del cielo en una dirección: degradado, nubes, sol y fondo neutro.
fn sky_color(dir: Vec3, sun_dir: Vec3) -> Vec3 {
    let d = dir.normalize();

    let up = d.y.max(0.0);
    let horizon = Vec3::new(0.86, 0.88, 0.93);
    let zenith = Vec3::new(0.36, 0.55, 0.86);
    let mut color = horizon + (zenith - horizon) * up.powf(0.55);

    // Nubes: fbm sobre la dirección, sólo por encima del horizonte.
    let wind = Vec3::new(0.7, 0.0, 0.3);
    let cloud = fbm_3d(d * 2.6 + wind, 5, 3);
    let mask = (up * 4.0).clamp(0.0, 1.0);
    let cover = (((cloud - 0.50) * 3.4).clamp(0.0, 1.0) * mask).clamp(0.0, 1.0);
    let cloud_color = Vec3::new(1.0, 0.99, 0.98);
    let shadowed = Vec3::new(0.72, 0.74, 0.80);
    let cloud_shade = shadowed + (cloud_color - shadowed) * cloud.clamp(0.0, 1.0);
    color = color + (cloud_shade - color) * cover;

    // Sol y halo.
    let s = d.dot(sun_dir).max(0.0);
    color += Vec3::new(1.0, 0.95, 0.80) * s.powf(1200.0) * 6.0;
    color += Vec3::new(1.0, 0.90, 0.70) * s.powf(40.0) * 0.22;

    // Bajo el horizonte: fondo neutro (el diorama flota sobre un gris suave).
    if d.y < 0.0 {
        let t = (-d.y * 3.0).clamp(0.0, 1.0);
        let ground = Vec3::new(0.58, 0.57, 0.62);
        color = color + (ground - color) * t;
    }

    color
}
