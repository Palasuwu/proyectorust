# Diorama – Raytracer en Rust

Raytracer escrito en Rust **sin librerías externas** (solo `std`). La escena se construye con cubos y se guarda como imagen BMP.

> Estado actual: dos cubos apilados, sin texturas, con iluminación Phong y sombras.

## Video

_(pendiente)_

## Uso

```bash
cargo run --release                                   # genera render.bmp
cargo run --release -- --yaw 120 --pitch 10 --distance 3 --out vista.bmp
```

| Opción       | Descripción                                   | Default      |
|--------------|-----------------------------------------------|--------------|
| `--width`    | Ancho en píxeles                              | 800          |
| `--height`   | Alto en píxeles                               | 600          |
| `--yaw`      | Rotación horizontal de la cámara (grados)     | 35           |
| `--pitch`    | Rotación vertical de la cámara (grados)       | 25           |
| `--distance` | Distancia de la cámara (zoom)                 | 5            |
| `--frames`   | Renderiza N frames de una vuelta completa     | 0 (una imagen) |
| `--out`      | Archivo de salida (o carpeta si hay `--frames`) | render.bmp |

### Generar el video

```bash
cargo run --release -- --frames 120 --out frames
ffmpeg -framerate 30 -i frames/frame_%04d.bmp -pix_fmt yuv420p diorama.mp4
```

## Estructura

- `src/main.rs` – escena, `cast_ray`, render en paralelo y argumentos
- `src/camera.rs` – cámara orbital (rotación y zoom)
- `src/cube.rs` – intersección rayo-cubo (AABB)
- `src/material.rs` – parámetros del material
- `src/bmp.rs` – escritura de imágenes BMP
- `src/vec3.rs` – álgebra de vectores
