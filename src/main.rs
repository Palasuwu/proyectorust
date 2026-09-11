mod bmp;
mod camera;
mod cube;
mod material;
mod ray_intersect;
mod vec3;

use std::f32::consts::PI;
use std::path::PathBuf;
use std::{env, fs, process, thread};

use camera::Camera;
use cube::Cube;
use material::Material;
use ray_intersect::{Intersect, RayIntersect};
use vec3::Vec3;

const BACKGROUND: Vec3 = Vec3::new(0.78, 0.90, 0.88);
const AMBIENT: f32 = 0.15;
const SHADOW_BIAS: f32 = 1e-3;
const FOV: f32 = PI / 3.0;

struct Light {
    position: Vec3,
    color: Vec3,
    intensity: f32,
}

struct Options {
    width: usize,
    height: usize,
    yaw: f32,
    pitch: f32,
    distance: f32,
    frames: usize,
    output: PathBuf,
}

fn closest_intersection(origin: Vec3, dir: Vec3, objects: &[Cube]) -> Option<Intersect> {
    objects
        .iter()
        .filter_map(|object| object.ray_intersect(origin, dir))
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

fn in_shadow(hit: &Intersect, light: &Light, objects: &[Cube]) -> bool {
    let to_light = light.position - hit.point;
    let light_distance = to_light.length();
    let origin = hit.point + hit.normal * SHADOW_BIAS;

    closest_intersection(origin, to_light.normalize(), objects)
        .is_some_and(|shadow_hit| shadow_hit.distance < light_distance)
}

fn cast_ray(origin: Vec3, dir: Vec3, objects: &[Cube], light: &Light) -> Vec3 {
    let Some(hit) = closest_intersection(origin, dir, objects) else {
        return BACKGROUND;
    };
    let material = hit.material;

    let light_dir = (light.position - hit.point).normalize();
    let view_dir = -dir;
    let reflect_dir = (-light_dir).reflect(hit.normal);

    let light_intensity = if in_shadow(&hit, light, objects) { 0.0 } else { light.intensity };

    let diffuse_intensity = hit.normal.dot(light_dir).max(0.0);
    let diffuse = material.diffuse * light.color * (material.albedo[0] * diffuse_intensity * light_intensity);

    let specular_intensity = view_dir.dot(reflect_dir).max(0.0).powf(material.specular);
    let specular = light.color * (material.albedo[1] * specular_intensity * light_intensity);

    let ambient = material.diffuse * AMBIENT;

    ambient + diffuse + specular
}

fn render(width: usize, height: usize, objects: &[Cube], camera: &Camera, light: &Light) -> Vec<Vec3> {
    let mut pixels = vec![Vec3::default(); width * height];
    let aspect = width as f32 / height as f32;
    let scale = (FOV * 0.5).tan();
    let eye = camera.eye();

    let workers = thread::available_parallelism().map_or(4, |n| n.get());
    let rows_per_chunk = height.div_ceil(workers).max(1);

    thread::scope(|s| {
        for (chunk_index, chunk) in pixels.chunks_mut(width * rows_per_chunk).enumerate() {
            s.spawn(move || {
                for (i, pixel) in chunk.iter_mut().enumerate() {
                    let x = i % width;
                    let y = chunk_index * rows_per_chunk + i / width;

                    let screen_x = ((2.0 * (x as f32 + 0.5)) / width as f32 - 1.0) * aspect * scale;
                    let screen_y = (1.0 - (2.0 * (y as f32 + 0.5)) / height as f32) * scale;

                    let dir = camera.basis_change(Vec3::new(screen_x, screen_y, -1.0));
                    *pixel = cast_ray(eye, dir, objects, light);
                }
            });
        }
    });

    pixels
}

fn build_scene() -> Vec<Cube> {
    let grass = Material::new(Vec3::new(0.30, 0.65, 0.30), [0.9, 0.1], 10.0);
    let stone = Material::new(Vec3::new(0.60, 0.60, 0.62), [0.8, 0.3], 30.0);

    vec![
        Cube::new(Vec3::new(0.0, 0.0, 0.0), 1.0, stone), // abajo
        Cube::new(Vec3::new(0.0, 1.0, 0.0), 1.0, grass), // encima
    ]
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        width: 800,
        height: 600,
        yaw: 35.0,
        pitch: 25.0,
        distance: 5.0,
        frames: 0,
        output: PathBuf::from("render.bmp"),
    };

    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("falta el valor para {flag}"));
        let bad = |v: &str| format!("valor inválido para {flag}: {v}");
        match flag.as_str() {
            "--width" => { let v = value()?; opts.width = v.parse().map_err(|_| bad(&v))?; }
            "--height" => { let v = value()?; opts.height = v.parse().map_err(|_| bad(&v))?; }
            "--yaw" => { let v = value()?; opts.yaw = v.parse().map_err(|_| bad(&v))?; }
            "--pitch" => { let v = value()?; opts.pitch = v.parse().map_err(|_| bad(&v))?; }
            "--distance" => { let v = value()?; opts.distance = v.parse().map_err(|_| bad(&v))?; }
            "--frames" => { let v = value()?; opts.frames = v.parse().map_err(|_| bad(&v))?; }
            "--out" => opts.output = PathBuf::from(value()?),
            _ => return Err(format!("opción desconocida: {flag}")),
        }
    }

    if opts.width == 0 || opts.height == 0 {
        return Err("el tamaño de la imagen debe ser mayor que 0".into());
    }
    Ok(opts)
}

fn main() {
    let opts = parse_args().unwrap_or_else(|err| {
        eprintln!("error: {err}");
        eprintln!(
            "uso: diorama [--width N] [--height N] [--yaw GRADOS] [--pitch GRADOS] \
             [--distance D] [--frames N] [--out RUTA]"
        );
        process::exit(1);
    });

    let objects = build_scene();
    let light = Light {
        position: Vec3::new(4.0, 6.0, 5.0),
        color: Vec3::new(1.0, 1.0, 1.0),
        intensity: 1.0,
    };
    let mut camera = Camera::new(
        Vec3::new(0.0, 0.5, 0.0),
        opts.yaw.to_radians(),
        opts.pitch.to_radians(),
        opts.distance,
    );

    if opts.frames == 0 {
        let pixels = render(opts.width, opts.height, &objects, &camera, &light);
        bmp::write_bmp(&opts.output, opts.width, opts.height, &pixels).unwrap_or_else(|err| {
            eprintln!("no se pudo guardar {}: {err}", opts.output.display());
            process::exit(1);
        });
        println!("imagen guardada en {}", opts.output.display());
        return;
    }

    // Animación: una vuelta completa mientras la cámara se acerca y aleja.
    fs::create_dir_all(&opts.output).unwrap_or_else(|err| {
        eprintln!("no se pudo crear {}: {err}", opts.output.display());
        process::exit(1);
    });
    let base_distance = opts.distance;
    let step = 2.0 * PI / opts.frames as f32;

    for frame in 0..opts.frames {
        let t = frame as f32 / opts.frames as f32;
        let target_distance = base_distance + 1.5 * (2.0 * PI * t).sin();
        camera.zoom(target_distance - camera.distance);

        let pixels = render(opts.width, opts.height, &objects, &camera, &light);
        let path = opts.output.join(format!("frame_{frame:04}.bmp"));
        bmp::write_bmp(&path, opts.width, opts.height, &pixels).unwrap_or_else(|err| {
            eprintln!("no se pudo guardar {}: {err}", path.display());
            process::exit(1);
        });
        println!("frame {}/{}", frame + 1, opts.frames);

        camera.orbit(step, 0.0);
    }
}
