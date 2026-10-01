//! Visor interactivo en una ventana real.
//!
//! El raytracer sigue siendo 100% propio: lo único externo es `ffplay`, un
//! programa del sistema que aquí funciona sólo como pantalla. Se le envían los
//! pixeles en crudo (RGB de 8 bits por canal) por una tubería, igual que el
//! modo en vivo de la terminal usa `stty` sólo como teclado.
//!
//! Mientras la cámara se mueve se renderiza a media resolución y con la
//! oclusión ambiental reducida; al soltar las teclas se vuelve a dibujar el
//! cuadro a resolución completa y con toda la calidad.

use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::camera::Camera;
use crate::live::{Action, RawMode, ROTATE_STEP, ZOOM_STEP, read_actions, spawn_keyboard};
use crate::render::{self, AO_RAYS_FULL, Quality, Scene};
use crate::vec3::Vec3;

/// Cuánto esperar sin teclas antes de redibujar en alta calidad.
const IDLE: Duration = Duration::from_millis(280);
/// Velocidad del giro automático, en radianes por segundo.
const AUTO_SPIN: f32 = 0.30;
/// Tope de cuadros por segundo mientras se mueve la cámara (60 fps).
const TARGET_FRAME: Duration = Duration::from_millis(16);

/// Abre la ventana. `width`/`height` son el tamaño de ventana que ve el usuario
/// y `pixel_w`/`pixel_h` la resolución real del video: en pantallas Retina el
/// sistema estira la ventana al doble, así que se envía el doble de pixeles
/// para que cada pixel del render caiga sobre un pixel físico.
fn spawn_ffplay(width: usize, height: usize, pixel_w: usize, pixel_h: usize) -> io::Result<Child> {
    Command::new("ffplay")
        .args([
            "-hide_banner",
            "-loglevel", "error",
            "-f", "rawvideo",
            "-pixel_format", "rgb24",
            "-video_size", &format!("{pixel_w}x{pixel_h}"),
            "-x", &width.to_string(),
            "-y", &height.to_string(),
            "-framerate", "30",
            "-fflags", "nobuffer",
            "-flags", "low_delay",
            "-autoexit",
            "-window_title", "Diorama voxel - raytracer en Rust",
            "-i", "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
}

/// Escribe el cuadro en la ventana. Si se renderizó a menor resolución, lo
/// amplía con interpolación bilineal: repetir pixeles se ve escalonado.
fn send_frame(
    sink: &mut impl Write,
    pixels: &[Vec3],
    render_w: usize,
    render_h: usize,
    width: usize,
    height: usize,
) -> io::Result<()> {
    let mut buf = Vec::with_capacity(width * height * 3);
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    let push = |c: Vec3, buf: &mut Vec<u8>| {
        buf.extend_from_slice(&[byte(c.x), byte(c.y), byte(c.z)]);
    };

    // Misma resolución: copia directa.
    if render_w == width && render_h == height {
        for c in pixels {
            push(*c, &mut buf);
        }
        sink.write_all(&buf)?;
        return sink.flush();
    }

    let sx_ratio = render_w as f32 / width as f32;
    let sy_ratio = render_h as f32 / height as f32;

    // Índices y pesos por columna: se calculan una vez, no por pixel.
    let columns: Vec<(usize, usize, f32)> = (0..width)
        .map(|x| {
            let fx = ((x as f32 + 0.5) * sx_ratio - 0.5).max(0.0);
            let x0 = (fx as usize).min(render_w - 1);
            (x0, (x0 + 1).min(render_w - 1), fx - x0 as f32)
        })
        .collect();

    for y in 0..height {
        let fy = ((y as f32 + 0.5) * sy_ratio - 0.5).max(0.0);
        let y0 = (fy as usize).min(render_h - 1);
        let y1 = (y0 + 1).min(render_h - 1);
        let ty = fy - y0 as f32;

        let (top_row, bottom_row) =
            (&pixels[y0 * render_w..][..render_w], &pixels[y1 * render_w..][..render_w]);

        for &(x0, x1, tx) in &columns {
            let top = top_row[x0] * (1.0 - tx) + top_row[x1] * tx;
            let bottom = bottom_row[x0] * (1.0 - tx) + bottom_row[x1] * tx;
            push(top * (1.0 - ty) + bottom * ty, &mut buf);
        }
    }

    sink.write_all(&buf)?;
    sink.flush()
}

/// `motion` divide la resolución mientras la cámara se mueve: 1 = nitidez
/// completa pero pocos cuadros, 2 = equilibrado, 3 = muy fluido pero borroso.
pub fn run(
    scene: &mut Scene,
    camera: &mut Camera,
    width: usize,
    height: usize,
    scale: usize,
    motion: usize,
) -> io::Result<()> {
    let (pixel_w, pixel_h) = (width * scale, height * scale);
    let mut player = spawn_ffplay(width, height, pixel_w, pixel_h).map_err(|err| {
        io::Error::other(format!("no se pudo abrir la ventana con ffplay: {err}"))
    })?;
    let mut sink = player.stdin.take().expect("ffplay debe aceptar entrada");

    // El teclado se lee de la terminal; si no es interactiva, la cámara gira sola.
    let raw = RawMode::enable();
    let keys = spawn_keyboard();

    println!("Ventana abierta: la escena gira sola.\r");
    println!("Los controles se leen EN ESTA TERMINAL (si tecleas sobre la ventana de video no pasa nada):\r");
    println!("  flechas / WASD  girar      +/-  acercar y alejar\r");
    println!("  o  giro automático         espacio  pausar el agua\r");
    println!("  r  vista inicial           q  salir\r");
    println!("\r");

    let (start_yaw, start_pitch, start_distance) = (camera.yaw, camera.pitch, camera.distance);
    let mut water_paused = false;
    let mut auto_spin = true;
    let mut last_input = Instant::now();
    let mut refined = false;
    let mut fps = 0.0f32;
    let mut keys_seen = 0u32;

    let mut last_frame = Instant::now();

    loop {
        let frame_start = Instant::now();
        let frame_delta = last_frame.elapsed().as_secs_f32().min(0.2);
        last_frame = frame_start;
        let mut moved = false;
        let mut quit = false;

        for action in read_actions(&keys) {
            moved = true;
            keys_seen += 1;
            // Cualquier movimiento manual apaga el giro automático.
            match action {
                Action::Left => { auto_spin = false; camera.orbit(-ROTATE_STEP, 0.0) }
                Action::Right => { auto_spin = false; camera.orbit(ROTATE_STEP, 0.0) }
                Action::Up => { auto_spin = false; camera.orbit(0.0, ROTATE_STEP) }
                Action::Down => { auto_spin = false; camera.orbit(0.0, -ROTATE_STEP) }
                Action::ZoomIn => camera.zoom(-ZOOM_STEP),
                Action::ZoomOut => camera.zoom(ZOOM_STEP),
                Action::ToggleAuto => auto_spin = !auto_spin,
                Action::TogglePause => water_paused = !water_paused,
                Action::Reset => {
                    camera.yaw = start_yaw;
                    camera.pitch = start_pitch;
                    camera.distance = start_distance;
                }
                Action::Quit => quit = true,
            }
        }
        if quit {
            break;
        }
        if moved {
            last_input = Instant::now();
            refined = false;
        }
        if auto_spin {
            // Independiente de los fps: gira al mismo ritmo aunque cueste dibujar.
            camera.orbit(AUTO_SPIN * frame_delta, 0.0);
        }

        // Mientras gira solo nunca se queda quieto.
        let idle = !auto_spin && last_input.elapsed() > IDLE;

        // Al quedarse quieto: un solo cuadro a máxima calidad, y a esperar.
        if idle && refined {
            thread::sleep(Duration::from_millis(30));
            continue;
        }

        // La oclusión ambiental está cacheada por cara, así que no hace falta
        // bajarla al moverse: sólo se recorta la resolución y los rebotes.
        let (render_w, render_h, samples) = if idle {
            // Quieto: resolución física completa, nítido en pantalla Retina.
            scene.quality = Quality { ao_rays: AO_RAYS_FULL, max_depth: 3, fast_shadows: false };
            (pixel_w, pixel_h, 1)
        } else {
            // En movimiento: menos resolución y se amplía suavizado.
            scene.quality = Quality { ao_rays: AO_RAYS_FULL, max_depth: 1, fast_shadows: true };
            (pixel_w / motion, pixel_h / motion, 1)
        };

        if !water_paused && !idle {
            scene.time += frame_delta * 1.2;
        }

        print!(
            "\r giro {:3.0}°  altura {:3.0}°  distancia {:3.0}  |  {:4.1} fps  |  {}  |  teclas recibidas: {}\x1b[K",
            camera.yaw.to_degrees().rem_euclid(360.0),
            camera.pitch.to_degrees(),
            camera.distance,
            fps,
            if auto_spin { "giro automático" } else { "control manual  " },
            keys_seen,
        );
        let _ = io::stdout().flush();

        let pixels = render::render(scene, camera, render_w, render_h, samples);
        if send_frame(&mut sink, &pixels, render_w, render_h, pixel_w, pixel_h).is_err() {
            break; // la ventana se cerró
        }
        if idle {
            refined = true;
        }

        let elapsed = frame_start.elapsed();
        fps = 1.0 / elapsed.as_secs_f32().max(1e-3);
        if !idle && elapsed < TARGET_FRAME {
            thread::sleep(TARGET_FRAME - elapsed);
        }
    }

    drop(sink);
    let _ = player.kill();
    let _ = player.wait();
    drop(raw);
    Ok(())
}
