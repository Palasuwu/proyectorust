//! Trazado de rayos: iluminación Phong, sombras, reflexión y refracción.

use std::f32::consts::PI;
use std::thread;

use crate::camera::Camera;
use crate::material::{Kind, material_of};
use crate::noise::water_normal;
use crate::sky::Skybox;
use crate::texture;
use crate::vec3::Vec3;
use crate::voxel::Grid;

const MAX_DEPTH: u32 = 3;
const BIAS: f32 = 1e-3;
const MAX_DIST: f32 = 400.0;
const FOV: f32 = PI / 6.0;
/// Color del agua acumulado por dispersión.
const WATER_TINT: Vec3 = Vec3::new(0.05, 0.28, 0.30);
/// Cuánta luz del cielo llega a las zonas en sombra.
const AMBIENT: f32 = 0.24;
/// Rayos de oclusión ambiental por impacto y su alcance en voxeles.
const AO_RAYS: usize = 13;
const AO_DIST: f32 = 5.0;

pub struct Scene {
    pub grid: Grid,
    pub sky: Skybox,
    pub sun_dir: Vec3,
    pub sun_color: Vec3,
    /// Instante de la animación (mueve el oleaje).
    pub time: f32,
}

fn transparent(id: u8) -> bool {
    material_of(id).transparency > 0.4
}

/// Ley de Snell. Devuelve `None` en reflexión total interna.
fn refract(incident: Vec3, normal: Vec3, ior: f32) -> Option<Vec3> {
    let mut cos_i = incident.dot(normal).clamp(-1.0, 1.0);
    let (mut eta_i, mut eta_t) = (1.0, ior);
    let mut n = normal;

    if cos_i < 0.0 {
        cos_i = -cos_i;
    } else {
        // El rayo sale del material.
        std::mem::swap(&mut eta_i, &mut eta_t);
        n = -normal;
    }

    let eta = eta_i / eta_t;
    let k = 1.0 - eta * eta * (1.0 - cos_i * cos_i);
    if k < 0.0 {
        None
    } else {
        Some(incident * eta + n * (eta * cos_i - k.sqrt()))
    }
}

/// Reflectancia de Fresnel (aproximación de Schlick).
fn fresnel(cos_i: f32, ior: f32) -> f32 {
    let f0 = ((1.0 - ior) / (1.0 + ior)).powi(2);
    f0 + (1.0 - f0) * (1.0 - cos_i).clamp(0.0, 1.0).powi(5)
}

/// Oclusión ambiental: lanza rayos cortos en el hemisferio de la normal para
/// oscurecer rincones y grietas entre bloques.
fn ambient_occlusion(scene: &Scene, point: Vec3, normal: Vec3) -> f32 {
    // Base tangente a partir de la normal (las caras están alineadas a los ejes).
    let helper = if normal.y.abs() > 0.9 { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(0.0, 1.0, 0.0) };
    let tangent = normal.cross(helper).normalize();
    let bitangent = normal.cross(tangent);

    let origin = point + normal * (BIAS * 4.0);
    let mut open = 0.0;

    for i in 0..AO_RAYS {
        let (dir, weight) = if i == 0 {
            (normal, 1.0)
        } else {
            let angle = (i as f32 - 1.0) * std::f32::consts::TAU / (AO_RAYS - 1) as f32;
            let tilt = if i % 2 == 0 { 0.80 } else { 0.50 };
            let side = tangent * angle.cos() + bitangent * angle.sin();
            ((normal * tilt + side * (1.0 - tilt * tilt).sqrt()).normalize(), tilt)
        };

        match scene.grid.traverse(origin, dir, AO_DIST, transparent) {
            // Lo cercano tapa más que lo lejano.
            Some(hit) => open += weight * (hit.distance / AO_DIST).clamp(0.0, 1.0),
            None => open += weight,
        }
    }

    let total: f32 = 1.0 + (1..AO_RAYS).map(|i| if i % 2 == 0 { 0.80 } else { 0.50 }).sum::<f32>();
    (open / total).clamp(0.0, 1.0)
}

fn shadow_factor(scene: &Scene, point: Vec3, normal: Vec3) -> f32 {
    let origin = point + normal * BIAS * 2.0;
    // Los materiales transparentes dejan pasar la luz.
    match scene.grid.traverse(origin, scene.sun_dir, MAX_DIST, transparent) {
        Some(_) => 0.0,
        None => 1.0,
    }
}

pub fn cast_ray(scene: &Scene, origin: Vec3, dir: Vec3, depth: u32, ignore: u8) -> Vec3 {
    trace(scene, origin, dir, depth, ignore).0
}

/// Igual que `cast_ray`, pero devuelve también la distancia recorrida hasta el
/// impacto: la necesita el agua para absorber la luz según la profundidad.
fn trace(scene: &Scene, origin: Vec3, dir: Vec3, depth: u32, ignore: u8) -> (Vec3, f32) {
    let Some(hit) = scene.grid.traverse(origin, dir, MAX_DIST, |id| id == ignore) else {
        return (scene.sky.sample(dir), MAX_DIST);
    };
    let distance = hit.distance;

    let material = material_of(hit.id);
    let kind = Kind::from_id(hit.id);
    let tex = texture::sample(hit.id, hit.point, hit.normal, hit.cell);

    // La cara superior del agua usa las olas del shader como normal.
    let mut normal = hit.normal;
    if kind == Kind::Water && hit.normal.y > 0.5 {
        let wave = water_normal(hit.point.x, hit.point.z, scene.time);
        normal = (hit.normal * 0.45 + wave * 0.55).normalize();
    }

    // --- Iluminación local (Phong) ---
    let view = -dir;
    let light = scene.sun_dir;
    let shade = shadow_factor(scene, hit.point, hit.normal);

    let diffuse = normal.dot(light).max(0.0) * shade;
    let reflect_dir = (-light).reflect(normal);
    let specular = view.dot(reflect_dir).max(0.0).powf(material.specular) * shade;

    // Luz ambiental tomada del propio skybox (cielo en la dirección de la normal).
    let occlusion = if depth == 0 { ambient_occlusion(scene, hit.point, hit.normal) } else { 1.0 };
    let ambient = scene.sky.sample(normal) * (AMBIENT * (0.04 + 0.96 * occlusion.powf(1.4)));

    let mut color = tex * (ambient + scene.sun_color * (material.albedo[0] * diffuse))
        + scene.sun_color * (material.albedo[1] * specular)
        + material.emissive * tex;

    if depth >= MAX_DEPTH || (material.reflectivity <= 0.0 && material.transparency <= 0.0) {
        return (color, distance);
    }

    // --- Reflexión y refracción ---
    let cos_i = view.dot(normal).clamp(0.0, 1.0);
    let f = fresnel(cos_i, material.ior);
    let kr = (material.reflectivity * f).clamp(0.0, 1.0);
    let kt = (material.transparency * (1.0 - kr)).clamp(0.0, 1.0);
    let local = (1.0 - kr - kt).max(0.0);

    color = color * local;

    if kr > 0.001 {
        let dir_r = dir.reflect(normal);
        let origin_r = hit.point + normal * BIAS;
        color += cast_ray(scene, origin_r, dir_r, depth + 1, 0) * kr;
    }

    if kt > 0.001 {
        if let Some(dir_t) = refract(dir, normal, material.ior) {
            let origin_t = hit.point - normal * BIAS;
            // Ignora el propio material para atravesar el bloque de agua/vidrio.
            let (mut through, travelled) =
                trace(scene, origin_t, dir_t.normalize(), depth + 1, hit.id);
            if kind == Kind::Water {
                // Ley de Beer-Lambert: el rojo se apaga mucho antes que el azul.
                let d = travelled.min(60.0);
                let absorb = Vec3::new(
                    (-0.130 * d).exp(),
                    (-0.042 * d).exp(),
                    (-0.028 * d).exp(),
                );
                through = through * absorb + WATER_TINT * (1.0 - absorb.y) * 0.35;
            }
            color += through * kt;
        } else {
            // Reflexión total interna.
            let dir_r = dir.reflect(normal);
            color += cast_ray(scene, hit.point + normal * BIAS, dir_r, depth + 1, 0) * kt;
        }
    }

    (color, distance)
}

/// Curva filmica (aproximación ACES), realce de saturación y gamma.
fn tonemap(c: Vec3) -> Vec3 {
    let f = |v: f32| {
        let v = v * 0.90;
        let mapped = (v * (2.51 * v + 0.03)) / (v * (2.43 * v + 0.59) + 0.14);
        mapped.clamp(0.0, 1.0).powf(1.0 / 2.2)
    };
    let mapped = Vec3::new(f(c.x), f(c.y), f(c.z));

    // Un poco más de saturación, como en los renders de referencia.
    let luma = mapped.x * 0.2126 + mapped.y * 0.7152 + mapped.z * 0.0722;
    let gray = Vec3::new(luma, luma, luma);
    let saturated = gray + (mapped - gray) * 1.22;

    Vec3::new(saturated.x.clamp(0.0, 1.0), saturated.y.clamp(0.0, 1.0), saturated.z.clamp(0.0, 1.0))
}

pub fn render(
    scene: &Scene,
    camera: &Camera,
    width: usize,
    height: usize,
    samples: usize,
) -> Vec<Vec3> {
    let mut pixels = vec![Vec3::default(); width * height];
    let aspect = width as f32 / height as f32;
    let scale = (FOV * 0.5).tan();
    let eye = camera.eye();
    let samples = samples.max(1);
    let inv_samples = 1.0 / (samples * samples) as f32;

    let workers = thread::available_parallelism().map_or(4, |n| n.get());
    let rows = height.div_ceil(workers).max(1);

    thread::scope(|s| {
        for (chunk_index, chunk) in pixels.chunks_mut(width * rows).enumerate() {
            s.spawn(move || {
                for (i, pixel) in chunk.iter_mut().enumerate() {
                    let x = i % width;
                    let y = chunk_index * rows + i / width;

                    let mut color = Vec3::default();
                    for sy in 0..samples {
                        for sx in 0..samples {
                            let ox = (sx as f32 + 0.5) / samples as f32;
                            let oy = (sy as f32 + 0.5) / samples as f32;
                            let px = ((2.0 * (x as f32 + ox)) / width as f32 - 1.0) * aspect * scale;
                            let py = (1.0 - (2.0 * (y as f32 + oy)) / height as f32) * scale;
                            let dir = camera.basis_change(Vec3::new(px, py, -1.0));
                            color += cast_ray(scene, eye, dir, 0, 0);
                        }
                    }
                    *pixel = tonemap(color * inv_samples);
                }
            });
        }
    });

    pixels
}
