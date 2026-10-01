mod bmp;
mod camera;
mod material;
mod noise;
mod render;
mod scene;
mod sky;
mod texture;
mod vec3;
mod voxel;

use std::f32::consts::PI;
use std::path::PathBuf;
use std::time::Instant;
use std::{env, fs, process};

use camera::Camera;
use render::Scene;
use sky::Skybox;
use vec3::Vec3;

struct Options {
    width: usize,
    height: usize,
    yaw: f32,
    pitch: f32,
    distance: f32,
    frames: usize,
    samples: usize,
    time: f32,
    sun_elev: f32,
    sun_azim: f32,
    target: Vec3,
    output: PathBuf,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        width: 900,
        height: 700,
        yaw: 30.0,
        pitch: 27.0,
        distance: 270.0,
        frames: 0,
        samples: 1,
        time: 0.0,
        sun_elev: 33.0,
        sun_azim: 110.0,
        target: Vec3::new(0.0, 0.0, 0.0),
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
            "--samples" => { let v = value()?; opts.samples = v.parse().map_err(|_| bad(&v))?; }
            "--cx" => { let v = value()?; opts.target.x = v.parse().map_err(|_| bad(&v))?; }
            "--cy" => { let v = value()?; opts.target.y = v.parse().map_err(|_| bad(&v))?; }
            "--cz" => { let v = value()?; opts.target.z = v.parse().map_err(|_| bad(&v))?; }
            "--sun-elev" => { let v = value()?; opts.sun_elev = v.parse().map_err(|_| bad(&v))?; }
            "--sun-azim" => { let v = value()?; opts.sun_azim = v.parse().map_err(|_| bad(&v))?; }
            "--time" => { let v = value()?; opts.time = v.parse().map_err(|_| bad(&v))?; }
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
             [--distance D] [--samples N] [--time T] [--sun-elev G] [--sun-azim G] [--cx X --cy Y --cz Z] [--frames N] [--out RUTA]"
        );
        process::exit(1);
    });

    let start = Instant::now();
    // Dirección del sol a partir de su elevación y azimut (en grados).
    let (elev, azim) = (opts.sun_elev.to_radians(), opts.sun_azim.to_radians());
    let sun_dir =
        Vec3::new(elev.cos() * azim.sin(), elev.sin(), elev.cos() * azim.cos()).normalize();
    let mut scene = Scene {
        grid: scene::build(),
        sky: Skybox::generate(256, sun_dir),
        sun_dir,
        sun_color: Vec3::new(1.28, 1.20, 1.02),
        time: opts.time,
    };
    println!("escena y skybox listos en {:.2?}", start.elapsed());

    let mut camera = Camera::new(
        opts.target,
        opts.yaw.to_radians(),
        opts.pitch.to_radians(),
        opts.distance,
    );

    if opts.frames == 0 {
        let t = Instant::now();
        let pixels = render::render(&scene, &camera, opts.width, opts.height, opts.samples);
        save(&opts.output, opts.width, opts.height, &pixels);
        println!("imagen guardada en {} ({:.2?})", opts.output.display(), t.elapsed());
        return;
    }

    // Animación: vuelta completa de cámara, con acercamiento y oleaje en marcha.
    fs::create_dir_all(&opts.output).unwrap_or_else(|err| {
        eprintln!("no se pudo crear {}: {err}", opts.output.display());
        process::exit(1);
    });
    let base_distance = opts.distance;
    let base_pitch = opts.pitch.to_radians();

    for frame in 0..opts.frames {
        let t = frame as f32 / opts.frames as f32;
        let angle = 2.0 * PI * t;

        // El oleaje avanza y la cámara se acerca y se aleja mientras gira.
        scene.time = opts.time + t * 16.0;
        camera.yaw = opts.yaw.to_radians() + angle;
        camera.pitch = base_pitch + (angle.sin() * 9.0).to_radians();
        camera.distance = base_distance - 70.0 * (angle * 2.0).sin();

        let pixels = render::render(&scene, &camera, opts.width, opts.height, opts.samples);
        let path = opts.output.join(format!("frame_{frame:04}.bmp"));
        save(&path, opts.width, opts.height, &pixels);
        println!("frame {}/{}", frame + 1, opts.frames);
    }
    println!("listo en {:.2?}", start.elapsed());
}

fn save(path: &std::path::Path, width: usize, height: usize, pixels: &[Vec3]) {
    bmp::write_bmp(path, width, height, pixels).unwrap_or_else(|err| {
        eprintln!("no se pudo guardar {}: {err}", path.display());
        process::exit(1);
    });
}
