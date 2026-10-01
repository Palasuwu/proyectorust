//! Visor interactivo en la terminal.
//!
//! No usa ninguna librería externa: dibuja con secuencias ANSI de color de 24
//! bits y el carácter de medio bloque `▀`, de modo que cada celda de la
//! terminal muestra dos pixeles (arriba = color de texto, abajo = fondo).
//! El modo crudo del teclado se activa llamando al programa `stty` del sistema.

use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crate::camera::Camera;
use crate::render::{self, Scene};

const TARGET_FRAME: Duration = Duration::from_millis(40); // ~25 fps
pub const ROTATE_STEP: f32 = 0.06;
pub const ZOOM_STEP: f32 = 8.0;

/// Deja la terminal en modo crudo y la restaura al salir.
pub struct RawMode;

impl RawMode {
    pub fn enable() -> Option<RawMode> {
        let ok = Command::new("stty")
            .args(["raw", "-echo"])
            .stdin(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        ok.then_some(RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = Command::new("stty").arg("sane").stdin(Stdio::inherit()).status();
        let mut out = io::stdout();
        // Muestra el cursor y limpia la pantalla.
        let _ = out.write_all(b"\x1b[?25h\x1b[0m\x1b[2J\x1b[H");
        let _ = out.flush();
    }
}

/// Tamaño de la terminal en (columnas, filas).
fn terminal_size() -> Option<(usize, usize)> {
    let output = Command::new("stty").arg("size").stdin(Stdio::inherit()).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let mut parts = text.split_whitespace();
    let rows: usize = parts.next()?.parse().ok()?;
    let cols: usize = parts.next()?.parse().ok()?;
    (rows > 4 && cols > 10).then_some((cols, rows))
}

/// Lee el teclado en un hilo aparte para no bloquear el render.
pub fn spawn_keyboard() -> Receiver<u8> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for byte in io::stdin().lock().bytes() {
            match byte {
                Ok(b) if tx.send(b).is_ok() => {}
                _ => break,
            }
        }
    });
    rx
}

pub enum Action {
    Left,
    Right,
    Up,
    Down,
    ZoomIn,
    ZoomOut,
    TogglePause,
    ToggleAuto,
    Reset,
    Quit,
}

pub fn read_actions(keys: &Receiver<u8>) -> Vec<Action> {
    let mut bytes = Vec::new();
    loop {
        match keys.try_recv() {
            Ok(b) => bytes.push(b),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    let mut actions = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Flechas: ESC [ A/B/C/D
        if b == 0x1b && i + 2 < bytes.len() && bytes[i + 1] == b'[' {
            actions.push(match bytes[i + 2] {
                b'A' => Action::Up,
                b'B' => Action::Down,
                b'C' => Action::Right,
                _ => Action::Left,
            });
            i += 3;
            continue;
        }
        match b {
            b'a' | b'A' => actions.push(Action::Left),
            b'd' | b'D' => actions.push(Action::Right),
            b'w' | b'W' => actions.push(Action::Up),
            b's' | b'S' => actions.push(Action::Down),
            b'+' | b'=' | b'z' => actions.push(Action::ZoomIn),
            b'-' | b'_' | b'x' => actions.push(Action::ZoomOut),
            b' ' => actions.push(Action::TogglePause),
            b'o' | b'O' => actions.push(Action::ToggleAuto),
            b'r' | b'R' => actions.push(Action::Reset),
            b'q' | b'Q' | 0x03 | 0x1b => actions.push(Action::Quit),
            _ => {}
        }
        i += 1;
    }
    actions
}

/// Convierte el framebuffer en texto con color y lo manda a la terminal.
fn draw(out: &mut impl Write, pixels: &[crate::vec3::Vec3], width: usize, height: usize, status: &str) -> io::Result<()> {
    let mut buf = String::with_capacity(width * height * 20);
    buf.push_str("\x1b[H");

    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;

    for row in (0..height).step_by(2) {
        let (mut last_top, mut last_bottom) = ((300u16, 0u8, 0u8), (300u16, 0u8, 0u8));
        for x in 0..width {
            let top = pixels[row * width + x];
            let bottom = if row + 1 < height { pixels[(row + 1) * width + x] } else { top };
            let t = (byte(top.x) as u16, byte(top.y), byte(top.z));
            let b = (byte(bottom.x) as u16, byte(bottom.y), byte(bottom.z));

            // Sólo reemite el código de color cuando cambia.
            if t != last_top {
                buf.push_str(&format!("\x1b[38;2;{};{};{}m", t.0, t.1, t.2));
                last_top = t;
            }
            if b != last_bottom {
                buf.push_str(&format!("\x1b[48;2;{};{};{}m", b.0, b.1, b.2));
                last_bottom = b;
            }
            buf.push('▀');
        }
        buf.push_str("\x1b[0m\r\n");
    }

    buf.push_str("\x1b[0m");
    buf.push_str(status);
    buf.push_str("\x1b[K");

    out.write_all(buf.as_bytes())?;
    out.flush()
}

/// Bucle interactivo: gira, acerca y aleja la cámara mientras el agua se mueve.
pub fn run(scene: &mut Scene, camera: &mut Camera, animate_water: bool) -> io::Result<()> {
    let Some(_raw) = RawMode::enable() else {
        eprintln!("El modo en vivo necesita una terminal interactiva (no funciona con la salida redirigida).");
        return Ok(());
    };

    let keys = spawn_keyboard();
    let mut out = io::stdout();
    out.write_all(b"\x1b[?25l\x1b[2J")?; // esconde el cursor y limpia

    let (start_yaw, start_pitch, start_distance) = (camera.yaw, camera.pitch, camera.distance);
    let mut paused = !animate_water;
    let mut size = terminal_size().unwrap_or((120, 40));
    let mut frames_since_resize = 0;
    let mut fps = 0.0f32;

    loop {
        let frame_start = Instant::now();

        let mut quit = false;
        for action in read_actions(&keys) {
            match action {
                Action::Left => camera.orbit(-ROTATE_STEP, 0.0),
                Action::Right => camera.orbit(ROTATE_STEP, 0.0),
                Action::Up => camera.orbit(0.0, ROTATE_STEP),
                Action::Down => camera.orbit(0.0, -ROTATE_STEP),
                Action::ZoomIn => camera.zoom(-ZOOM_STEP),
                Action::ZoomOut => camera.zoom(ZOOM_STEP),
                Action::TogglePause => paused = !paused,
                Action::ToggleAuto => {}
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

        // Relee el tamaño de la terminal de vez en cuando por si cambió.
        frames_since_resize += 1;
        if frames_since_resize > 25 {
            frames_since_resize = 0;
            if let Some(new_size) = terminal_size() {
                if new_size != size {
                    size = new_size;
                    out.write_all(b"\x1b[2J")?;
                }
            }
        }

        let (cols, rows) = size;
        let (width, height) = (cols, (rows - 1) * 2);

        if !paused {
            scene.time += 0.05;
        }

        let pixels = render::render(scene, camera, width, height, 1);
        let status = format!(
            "\x1b[1m flechas/WASD\x1b[0m girar  \x1b[1m+/-\x1b[0m zoom  \x1b[1mespacio\x1b[0m agua  \x1b[1mr\x1b[0m reiniciar  \x1b[1mq\x1b[0m salir   |  \
             giro {:.0}°  altura {:.0}°  dist {:.0}  {:.1} fps ",
            camera.yaw.to_degrees().rem_euclid(360.0),
            camera.pitch.to_degrees(),
            camera.distance,
            fps,
        );
        draw(&mut out, &pixels, width, height, &status)?;

        let elapsed = frame_start.elapsed();
        fps = 1.0 / elapsed.as_secs_f32().max(1e-3);
        if elapsed < TARGET_FRAME {
            thread::sleep(TARGET_FRAME - elapsed);
        }
    }

    Ok(())
}
