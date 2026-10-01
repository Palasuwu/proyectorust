# Diorama Voxel — Raytracer en Rust

Diorama de una laguna con una cabaña de madera, renderizado con un **raytracer escrito desde cero en Rust**, usando **únicamente la librería estándar** (sin crates externos). La escena está construida con cubos texturizados dentro de una rejilla de voxeles de 160×104×160 (≈2.7 millones de celdas) que se recorre con el algoritmo **DDA**.

![Diorama](docs/diorama.png)

## Video

`docs/diorama.mp4` — vuelta completa de la cámara con acercamiento y alejamiento, y el oleaje en movimiento.

https://github.com/Palasuwu/proyectorust/raw/main/docs/diorama.mp4

## Galería

| Laguna: refracción, reflejo y oleaje | Vista trasera: estratos de roca |
|---|---|
| ![Laguna](docs/laguna.png) | ![Atrás](docs/atras.png) |

## Cómo ejecutarlo

```bash
cargo run --release                          # imagen única -> render.bmp
cargo run --release -- --yaw 120 --distance 220 --samples 2 --out vista.bmp
cargo run --release -- --frames 150 --out frames    # secuencia para el video
```

Para armar el video a partir de los frames (ffmpeg es una herramienta externa, no una librería del programa):

```bash
ffmpeg -framerate 30 -i frames/frame_%04d.bmp -pix_fmt yuv420p -crf 20 docs/diorama.mp4
```

### Opciones

| Opción | Qué hace | Default |
|---|---|---|
| `--width`, `--height` | Tamaño de la imagen | 900×700 |
| `--yaw`, `--pitch` | Rotación de la cámara en grados | 30, 27 |
| `--distance` | Distancia de la cámara (zoom) | 270 |
| `--cx --cy --cz` | Punto al que mira la cámara | 0,0,0 |
| `--samples` | Muestras por pixel por eje (2 = 4× antialiasing) | 1 |
| `--sun-elev`, `--sun-azim` | Posición del sol en grados | 33, 110 |
| `--time` | Instante del oleaje | 0 |
| `--frames` | Renderiza N frames de una órbita completa | 0 |
| `--out` | Archivo de salida (o carpeta si hay `--frames`) | render.bmp |

## Cómo se cumple la rúbrica

### Rotación y zoom de la cámara (20 pts)

[`src/camera.rs`](src/camera.rs) implementa una cámara orbital: `orbit()` gira alrededor del diorama (con el cabeceo limitado para que no se voltee) y `zoom()` acerca y aleja. Se controla con `--yaw`, `--pitch` y `--distance`, y el modo `--frames` recorre los 360° variando también la distancia y el cabeceo.

### Materiales (5 pts c/u, máximo 25)

14 tipos de bloque, cada uno con **su propia textura procedural** y sus propios parámetros ([`src/material.rs`](src/material.rs), [`src/texture.rs`](src/texture.rs)):

| Material | Albedo (dif/esp) | Specular | Reflectividad | Transparencia | Índice refracción |
|---|---|---|---|---|---|
| Pasto | 0.95 / 0.05 | 8 | — | — | — |
| Tierra | 0.95 / 0.03 | 6 | — | — | — |
| Piedra | 0.85 / 0.25 | 40 | 0.08 | — | — |
| Arena | 0.95 / 0.10 | 12 | — | — | — |
| Tablón | 0.90 / 0.18 | 24 | 0.03 | — | — |
| Tronco | 0.92 / 0.10 | 14 | — | — | — |
| Hojas (verde, rosa, naranja) | 0.95 / 0.08 | 10 | — | — | — |
| **Agua** | 0.25 / 0.90 | 220 | 1.00 | 0.92 | 1.333 |
| **Vidrio** | 0.15 / 0.80 | 160 | 0.55 | 0.88 | 1.52 |
| Farol (emisivo) | 0.60 / 0.40 | 60 | — | 0.25 | — |
| Teja | 0.90 / 0.15 | 20 | 0.04 | — | — |
| Flor | 0.95 / 0.10 | 10 | — | — | — |

Las texturas no son imágenes cargadas de disco: se generan con hash y ruido de valor, dividiendo cada cara del cubo en 16×16 texels. Por eso cada material tiene su propio patrón: vetas y juntas en los tablones, anillos en las tapas del tronco, tejas escalonadas en el techo, marco en el vidrio, grietas en la piedra.

### Refracción (10 pts)

[`src/render.rs`](src/render.rs) aplica la **ley de Snell** en `refract()`, con reflexión total interna cuando corresponde. Tiene sentido en la escena porque el agua de la laguna deja ver el fondo desplazado, y las ventanas de la cabaña son de vidrio. Además el agua aplica **absorción de Beer-Lambert**: el rojo se apaga antes que el azul, así que lo poco profundo se ve claro y el centro de la laguna se pone azul-verde oscuro.

### Reflexión (5 pts)

También en `cast_ray()`: los rayos reflejados usan **Fresnel (aproximación de Schlick)**, de modo que el agua refleja el cielo y los árboles en ángulos rasantes y deja ver el fondo cuando se mira de frente. El vidrio, la piedra, la teja y los tablones tienen reflectividad propia.

### Skybox (20 pts)

[`src/sky.rs`](src/sky.rs) genera un cubo de seis texturas de 256×256 y lo muestrea por dirección con interpolación bilineal. Cada texel se calcula a partir de su dirección 3D (degradado del cielo, nubes con fbm, disco solar y halo), así que no hay costuras entre las caras. El skybox también sirve como **luz ambiental**: el color del cielo en la dirección de la normal ilumina las zonas en sombra.

### Complejidad y atractivo visual (50 pts)

- Terreno procedural con colina, acantilados, afloramientos de piedra y estratos en el zócalo
- Laguna cortada en el borde del diorama, con fondo oscuro, juncos, nenúfares, bote y muelle
- Cabaña con zócalo de ladrillo, muros de tablón, esquinas de tronco, ventanas, puerta, techo a dos aguas con alero, chimenea, terraza sobre pilotes con barandal y jardineras
- Bosque de coníferas y árboles frondosos en tres colores, arbustos, hierba alta, flores, peñascos, camino de piedra, banca y letrero
- **Oclusión ambiental** con 13 rayos por impacto, sombras duras del sol, iluminación Phong y tonemap filmico (ACES)

## Arquitectura

| Archivo | Responsabilidad |
|---|---|
| [`src/main.rs`](src/main.rs) | Argumentos, armado de la escena y modo animación |
| [`src/voxel.rs`](src/voxel.rs) | Rejilla de voxeles y recorrido DDA (Amanatides & Woo) |
| [`src/render.rs`](src/render.rs) | Phong, sombras, oclusión ambiental, reflexión, refracción, tonemap |
| [`src/scene.rs`](src/scene.rs) | Generación del diorama |
| [`src/material.rs`](src/material.rs) | Tipos de bloque y sus parámetros |
| [`src/texture.rs`](src/texture.rs) | Texturas procedurales por cara |
| [`src/sky.rs`](src/sky.rs) | Skybox procedural |
| [`src/noise.rs`](src/noise.rs) | Hash, ruido de valor, fbm y oleaje |
| [`src/camera.rs`](src/camera.rs) | Cámara orbital |
| [`src/bmp.rs`](src/bmp.rs) | Escritura de BMP de 24 bits |
| [`src/vec3.rs`](src/vec3.rs) | Álgebra de vectores |

El render usa todos los núcleos disponibles con `std::thread::scope`. Las pruebas del recorrido DDA se corren con `cargo test`.

## Créditos

El oleaje del agua (`sea_octave`, `water_height` en [`src/noise.rs`](src/noise.rs)) es una adaptación a Rust del shader [Seascape](https://www.shadertoy.com/view/Ms2SD1) de TDM.
