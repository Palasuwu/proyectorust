//! Trazado de rayos: iluminación Phong, sombras, reflexión y refracción.

use std::f32::consts::PI;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
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
/// Alcance máximo de un rayo. El recorrido ya se corta al salir de la rejilla,
/// así que sólo tiene que ser mayor que la diagonal del mundo más la distancia
/// de la cámara.
const MAX_DIST: f32 = 10_000.0;
const FOV: f32 = PI / 6.0;
/// Color del agua acumulado por dispersión.
const WATER_TINT: Vec3 = Vec3::new(0.05, 0.28, 0.30);
/// Cuánta luz del cielo llega a las zonas en sombra.
const AMBIENT: f32 = 0.24;
/// Alcance de los rayos de oclusión ambiental, en voxeles.
const AO_DIST: f32 = 5.0;
/// Rayos de oclusión ambiental de una imagen final.
pub const AO_RAYS_FULL: usize = 13;

/// Ajustes que se bajan mientras la cámara se mueve y se suben al detenerse.
#[derive(Clone, Copy)]
pub struct Quality {
    /// Rayos de oclusión ambiental por impacto (0 la desactiva).
    pub ao_rays: usize,
    /// Rebotes de reflexión y refracción.
    pub max_depth: u32,
    /// Sombra cacheada por cara en vez de por pixel. Es mucho más rápida, pero
    /// el borde de la sombra se escalona bloque a bloque: sólo se usa mientras
    /// la cámara se mueve, nunca en las imágenes finales.
    pub fast_shadows: bool,
}

impl Default for Quality {
    fn default() -> Self {
        Self { ao_rays: AO_RAYS_FULL, max_depth: MAX_DEPTH, fast_shadows: false }
    }
}

/// Oclusión ambiental ya calculada, una entrada por cara de cubo.
///
/// La geometría no cambia, así que la oclusión de una cara es siempre la misma:
/// se calcula la primera vez que se ve y se reutiliza en todos los pixeles y
/// todos los cuadros. Se guarda con enteros atómicos para que los hilos puedan
/// llenarla sin bloquearse (0 = sin calcular).
pub struct AoCache {
    values: Vec<AtomicU8>,
}

impl AoCache {
    pub fn new(cells: usize) -> Self {
        Self { values: (0..cells * 6).map(|_| AtomicU8::new(0)).collect() }
    }

    fn slot(&self, index: usize) -> &AtomicU8 {
        &self.values[index]
    }
}

/// Índice de la cara golpeada dentro de una celda (0..5).
fn face_index(normal: Vec3) -> usize {
    if normal.x.abs() > 0.5 {
        (normal.x > 0.0) as usize
    } else if normal.y.abs() > 0.5 {
        2 + (normal.y > 0.0) as usize
    } else {
        4 + (normal.z > 0.0) as usize
    }
}

/// Farol: luz puntual con alcance limitado.
pub struct PointLight {
    pub position: Vec3,
    pub radius: f32,
}

/// Color cálido de los faroles.
const LANTERN_COLOR: Vec3 = Vec3::new(1.0, 0.72, 0.38);
/// Tope de intensidad que guarda la caché de luz.
const LIGHT_MAX: f32 = 6.0;

pub struct Scene {
    pub grid: Grid,
    pub sky: Skybox,
    /// Skybox nocturno, para la transición día->noche de la animación.
    pub sky_night: Option<Skybox>,
    /// 0 = día, 1 = noche.
    pub night_blend: f32,
    pub sun_dir: Vec3,
    pub sun_color: Vec3,
    /// Instante de la animación (mueve el oleaje).
    pub time: f32,
    pub quality: Quality,
    /// Cuánto rinden los faroles (de noche iluminan mucho más).
    pub lantern_power: f32,
    /// Faroles de la escena (posición de cada bloque emisivo).
    pub lights: Vec<PointLight>,
    pub ao_cache: AoCache,
    /// Luz de los faroles ya acumulada por cara.
    pub light_cache: AoCache,
    /// Sombras ya calculadas por cara (0 = sin calcular, 1 = al sol, 2 = en sombra).
    pub shadow_cache: AoCache,
}

impl Scene {
    /// Color del cielo, mezclando el skybox de día y el de noche.
    pub fn sky_at(&self, dir: Vec3) -> Vec3 {
        let day = self.sky.sample(dir);
        match &self.sky_night {
            Some(night) if self.night_blend > 0.0 => {
                let n = night.sample(dir);
                day + (n - day) * self.night_blend
            }
            _ => day,
        }
    }
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
/// Devuelve la oclusión de la cara golpeada, calculándola sólo si hace falta.
fn ambient_occlusion(scene: &Scene, cell: [i32; 3], normal: Vec3) -> f32 {
    let rays = scene.quality.ao_rays;
    if rays == 0 {
        return 1.0;
    }

    let face = face_index(normal);
    let slot = scene.ao_cache.slot(scene.grid.cell_index(cell[0], cell[1], cell[2]) * 6 + face);

    let cached = slot.load(Ordering::Relaxed);
    if cached != 0 {
        return (cached - 1) as f32 / 254.0;
    }

    // Se mide en el centro de la cara: una cara de cubo ocupa pocos pixeles.
    let center = scene.grid.origin
        + Vec3::new(cell[0] as f32 + 0.5, cell[1] as f32 + 0.5, cell[2] as f32 + 0.5)
        + normal * 0.5;
    let value = trace_occlusion(scene, center, normal, rays);
    slot.store(1 + (value * 254.0) as u8, Ordering::Relaxed);
    value
}

fn trace_occlusion(scene: &Scene, point: Vec3, normal: Vec3, rays: usize) -> f32 {
    // Base tangente a partir de la normal (las caras están alineadas a los ejes).
    let helper = if normal.y.abs() > 0.9 { Vec3::new(1.0, 0.0, 0.0) } else { Vec3::new(0.0, 1.0, 0.0) };
    let tangent = normal.cross(helper).normalize();
    let bitangent = normal.cross(tangent);

    let origin = point + normal * (BIAS * 4.0);
    let mut open = 0.0;

    for i in 0..rays {
        let (dir, weight) = if i == 0 {
            (normal, 1.0)
        } else {
            let angle = (i as f32 - 1.0) * std::f32::consts::TAU / (rays - 1) as f32;
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

    let total: f32 = 1.0 + (1..rays).map(|i| if i % 2 == 0 { 0.80 } else { 0.50 }).sum::<f32>();
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

/// Luz que los faroles aportan a una cara. Como ni las luces ni la geometría se
/// mueven, se calcula la primera vez que se ve la cara y se reutiliza siempre.
fn lantern_light(scene: &Scene, cell: [i32; 3], normal: Vec3) -> f32 {
    if scene.lights.is_empty() {
        return 0.0;
    }

    let face = face_index(normal);
    let slot = scene.light_cache.slot(scene.grid.cell_index(cell[0], cell[1], cell[2]) * 6 + face);
    let cached = slot.load(Ordering::Relaxed);
    if cached != 0 {
        return (cached - 1) as f32 / 254.0 * LIGHT_MAX;
    }

    let center = scene.grid.origin
        + Vec3::new(cell[0] as f32 + 0.5, cell[1] as f32 + 0.5, cell[2] as f32 + 0.5)
        + normal * 0.5;

    let mut total = 0.0;
    for light in &scene.lights {
        let to_light = light.position - center;
        let distance = to_light.length();
        if distance > light.radius || distance < 1e-3 {
            continue;
        }
        let dir = to_light * (1.0 / distance);
        let facing = normal.dot(dir);
        if facing <= 0.0 {
            continue;
        }
        // Atenuación suave: llega a cero justo en el radio.
        let falloff = (1.0 - distance / light.radius).powi(2);
        if scene
            .grid
            .traverse(center + normal * (BIAS * 4.0), dir, distance - 1.2, transparent)
            .is_some()
        {
            continue;
        }
        total += facing * falloff;
    }

    let stored = (total / LIGHT_MAX).clamp(0.0, 1.0);
    slot.store(1 + (stored * 254.0) as u8, Ordering::Relaxed);
    total.min(LIGHT_MAX)
}

/// Sombra de la cara completa, calculada una sola vez y reutilizada.
fn shadow_cached(scene: &Scene, cell: [i32; 3], normal: Vec3) -> f32 {
    let face = face_index(normal);
    let slot = scene.shadow_cache.slot(scene.grid.cell_index(cell[0], cell[1], cell[2]) * 6 + face);

    match slot.load(Ordering::Relaxed) {
        1 => return 1.0,
        2 => return 0.0,
        _ => {}
    }

    let center = scene.grid.origin
        + Vec3::new(cell[0] as f32 + 0.5, cell[1] as f32 + 0.5, cell[2] as f32 + 0.5)
        + normal * 0.5;
    let lit = shadow_factor(scene, center, normal);
    slot.store(if lit > 0.5 { 1 } else { 2 }, Ordering::Relaxed);
    lit
}

pub fn cast_ray(scene: &Scene, origin: Vec3, dir: Vec3, depth: u32, ignore: u8) -> Vec3 {
    trace(scene, origin, dir, depth, ignore).0
}

/// Igual que `cast_ray`, pero devuelve también la distancia recorrida hasta el
/// impacto: la necesita el agua para absorber la luz según la profundidad.
fn trace(scene: &Scene, origin: Vec3, dir: Vec3, depth: u32, ignore: u8) -> (Vec3, f32) {
    let Some(hit) = scene.grid.traverse(origin, dir, MAX_DIST, |id| id == ignore) else {
        return (scene.sky_at(dir), MAX_DIST);
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
    let shade = if scene.quality.fast_shadows {
        shadow_cached(scene, hit.cell, hit.normal)
    } else {
        shadow_factor(scene, hit.point, hit.normal)
    };

    let diffuse = normal.dot(light).max(0.0) * shade;
    let reflect_dir = (-light).reflect(normal);
    let specular = view.dot(reflect_dir).max(0.0).powf(material.specular) * shade;

    // Luz ambiental tomada del propio skybox (cielo en la dirección de la normal).
    let occlusion = ambient_occlusion(scene, hit.cell, hit.normal);
    let ambient = scene.sky_at(normal) * (AMBIENT * (0.04 + 0.96 * occlusion.powf(1.4)));

    let lantern = lantern_light(scene, hit.cell, hit.normal);

    let mut color = tex
        * (ambient
            + scene.sun_color * (material.albedo[0] * diffuse)
            + LANTERN_COLOR * (material.albedo[0] * lantern * scene.lantern_power))
        + scene.sun_color * (material.albedo[1] * specular)
        + material.emissive * tex;

    if depth >= scene.quality.max_depth || (material.reflectivity <= 0.0 && material.transparency <= 0.0) {
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

    // Cola de filas: cada hilo toma la siguiente que quede libre. Repartir
    // bloques fijos desperdicia núcleos, porque las filas de cielo son mucho
    // más baratas que las del bosque.
    let queue = Mutex::new(pixels.chunks_mut(width).enumerate().collect::<Vec<_>>());

    thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let Some((y, row)) = queue.lock().expect("cola de filas").pop() else {
                        break;
                    };

                    for (x, pixel) in row.iter_mut().enumerate() {
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
                }
            });
        }
    });

    pixels
}
