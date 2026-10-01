//! Construcción del diorama: terreno, laguna, cabaña, vegetación y detalles.

use crate::material::Kind;
use crate::noise::{fbm_2d, fbm_3d, hash_2i, hash_3i};
use crate::vec3::Vec3;
use crate::voxel::Grid;

pub const SIZE_X: i32 = 160;
pub const SIZE_Y: i32 = 104;
pub const SIZE_Z: i32 = 160;

/// Superficie del agua (en voxeles).
pub const WATER_LEVEL: i32 = 30;
/// Roca maciza sobre la que se apoya el diorama.
const BEDROCK: i32 = 10;

/// Qué tanto entra la laguna en una columna (positivo = agua).
fn lagoon_field(x: f32, z: f32) -> f32 {
    let wobble = (fbm_2d(x * 0.038, z * 0.038, 3, 21) - 0.5) * 0.55;
    // Elipse abierta hacia el borde frontal del diorama.
    let d = (((x - 62.0) / 58.0).powi(2) + ((z - 126.0) / 54.0).powi(2)).sqrt();
    (1.0 - d) + wobble
}

fn terrain_height(x: f32, z: f32) -> f32 {
    let rolling = fbm_2d(x * 0.024, z * 0.024, 4, 5);
    let detail = fbm_2d(x * 0.065 + 5.0, z * 0.065, 4, 9);
    let ripple = fbm_2d(x * 0.21, z * 0.21, 2, 33) - 0.5;
    let mut h = 34.0 + rolling * 9.0 + detail * 4.0 + ripple * 2.2;

    // Colina rocosa al fondo.
    let back = ((70.0 - z) / 70.0).clamp(0.0, 1.0);
    h += back * back * 24.0;

    // Cauce de la laguna.
    h -= lagoon_field(x, z).max(0.0) * 28.0;

    h.clamp(BEDROCK as f32 + 2.0, (SIZE_Y - 30) as f32)
}

pub struct World {
    pub grid: Grid,
    height: Vec<i32>,
    reserved: Vec<(i32, i32, i32, i32)>,
}

impl World {
    fn top(&self, x: i32, z: i32) -> i32 {
        if x < 0 || z < 0 || x >= SIZE_X || z >= SIZE_Z {
            return 0;
        }
        self.height[(z * SIZE_X + x) as usize]
    }

    fn set_top(&mut self, x: i32, z: i32, y: i32) {
        if x >= 0 && z >= 0 && x < SIZE_X && z < SIZE_Z {
            self.height[(z * SIZE_X + x) as usize] = y;
        }
    }

    fn reserve(&mut self, x0: i32, z0: i32, x1: i32, z1: i32) {
        self.reserved.push((x0, z0, x1, z1));
    }

    fn is_reserved(&self, x: i32, z: i32) -> bool {
        self.reserved.iter().any(|&(x0, z0, x1, z1)| x >= x0 && x <= x1 && z >= z0 && z <= z1)
    }

    fn set(&mut self, x: i32, y: i32, z: i32, kind: Kind) {
        self.grid.set(x, y, z, kind.id());
    }

    fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        self.grid.get(x, y, z)
    }

    fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == Kind::Air.id()
    }

    fn is_water(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == Kind::Water.id()
    }
}

pub fn build() -> Grid {
    let grid = Grid::new([SIZE_X, SIZE_Y, SIZE_Z], Vec3::new(-80.0, -40.0, -80.0));
    let mut world =
        World { grid, height: vec![0; (SIZE_X * SIZE_Z) as usize], reserved: Vec::new() };

    terrain(&mut world);
    outcrops(&mut world);
    let floor = cabin(&mut world, 88, 54);
    pier(&mut world, floor);
    boat(&mut world, 48, 118);
    stone_path(&mut world);
    signpost(&mut world, 40, 40);
    bench(&mut world, 118, 104);
    vegetation(&mut world);
    shoreline(&mut world);

    world.grid
}

fn terrain(world: &mut World) {
    for z in 0..SIZE_Z {
        for x in 0..SIZE_X {
            let h = terrain_height(x as f32, z as f32).round() as i32;

            for y in 0..=h {
                let shore = (h - WATER_LEVEL).abs() <= 2;
                // Estratos horizontales en la roca del zócalo del diorama.
                let strata = {
                    let band = (y as f32 * 0.16 + fbm_2d(x as f32 * 0.05, z as f32 * 0.05, 2, 87) * 1.4)
                        .floor() as i32;
                    if band.rem_euclid(3) == 0 { Kind::Dirt } else { Kind::Stone }
                };
                let kind = if y < BEDROCK || y < h - 8 {
                    strata
                } else if y < h {
                    Kind::Dirt
                } else if shore {
                    Kind::Sand
                } else if h < WATER_LEVEL {
                    // Fondo de la laguna: oscuro, para que el agua no se vea lechosa.
                    Kind::Dirt
                } else {
                    Kind::Grass
                };
                world.set(x, y, z, kind);
            }

            for y in (h + 1)..=WATER_LEVEL {
                world.set(x, y, z, Kind::Water);
            }

            world.set_top(x, z, h);
        }
    }
}

/// Afloramientos de piedra en laderas empinadas y peñascos sueltos.
fn outcrops(world: &mut World) {
    for z in 1..SIZE_Z - 1 {
        for x in 1..SIZE_X - 1 {
            let h = world.top(x, z);
            if h <= WATER_LEVEL + 2 {
                continue;
            }
            let slope = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|&(dx, dz)| (h - world.top(x + dx, z + dz)).abs())
                .max()
                .unwrap_or(0);
            let noise = fbm_2d(x as f32 * 0.06, z as f32 * 0.06, 3, 63);

            if slope >= 4 || (h > 58 && noise > 0.63) {
                for y in (h - 2)..=h {
                    world.set(x, y, z, Kind::Stone);
                }
            }
        }
    }

    // Peñascos redondeados sobre el pasto.
    for i in 0..26 {
        let x = 8 + (hash_2i(i, 0, 301) * (SIZE_X - 16) as f32) as i32;
        let z = 8 + (hash_2i(i, 1, 307) * (SIZE_Z - 16) as f32) as i32;
        let ground = world.top(x, z);
        if ground <= WATER_LEVEL + 1 {
            continue;
        }
        let r = 2 + (hash_2i(i, 2, 311) * 3.0) as i32;
        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                    let wobble = hash_3i(x + dx, ground + dy, z + dz, 313) * 1.1;
                    if d <= r as f32 - 0.4 + wobble {
                        world.set(x + dx, ground + dy, z + dz, Kind::Stone);
                    }
                }
            }
        }
    }
}

fn fill(world: &mut World, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, kind: Kind) {
    for y in y0..=y1 {
        for z in z0..=z1 {
            for x in x0..=x1 {
                world.set(x, y, z, kind);
            }
        }
    }
}

/// Aplana una zona al nivel indicado.
fn flatten(world: &mut World, x0: i32, z0: i32, x1: i32, z1: i32, y: i32) {
    for z in z0..=z1 {
        for x in x0..=x1 {
            for yy in (y + 1)..SIZE_Y {
                world.set(x, yy, z, Kind::Air);
            }
            for yy in 0..=y {
                if world.is_air(x, yy, z) || world.is_water(x, yy, z) {
                    let kind = if yy < y - 6 { Kind::Stone } else { Kind::Dirt };
                    world.set(x, yy, z, kind);
                }
            }
            world.set(x, y, z, Kind::Grass);
            world.set_top(x, z, y);
        }
    }
}

/// Pilote que baja hasta encontrar suelo firme (atraviesa el agua).
fn stilt(world: &mut World, x: i32, z: i32, from_y: i32) {
    let mut y = from_y;
    while y > 0 {
        let below = world.get(x, y, z);
        if below != Kind::Air.id() && below != Kind::Water.id() {
            break;
        }
        world.set(x, y, z, Kind::Log);
        y -= 1;
    }
}

/// Cabaña de madera con terraza sobre pilotes. Devuelve la altura del piso.
fn cabin(world: &mut World, x0: i32, z0: i32) -> i32 {
    const W: i32 = 30;
    const D: i32 = 24;
    const DECK: i32 = 26;

    let (x1, z1) = (x0 + W - 1, z0 + D - 1);
    let ground = world.top(x0 + W / 2, z0 + 4).max(WATER_LEVEL + 8);

    flatten(world, x0 - 6, z0 - 6, x1 + 6, z1, ground);
    let floor = ground + 1;

    // Terraza de tablones que se proyecta sobre la orilla, sobre pilotes.
    fill(world, x0 - 5, floor, z0 - 2, x1 + 4, floor, z1 + DECK, Kind::Plank);
    for pz in [z1 + DECK, z1 + DECK - 7, z1 + DECK - 14, z1 + 3] {
        for px in [x0 - 5, x0 + 6, x1 - 6, x1 + 4] {
            stilt(world, px, pz, floor - 1);
        }
    }

    // Muros con esquinas de tronco y zócalo de piedra.
    let wall_top = floor + 8;
    for y in (floor + 1)..=wall_top {
        for z in z0..=z1 {
            for x in x0..=x1 {
                if x != x0 && x != x1 && z != z0 && z != z1 {
                    continue;
                }
                let corner = (x - x0).min(x1 - x).min((z - z0).min(z1 - z)) < 2;
                let kind = if corner {
                    Kind::Log
                } else if y <= floor + 2 {
                    Kind::Roof // ladrillo del zócalo
                } else {
                    Kind::Plank
                };
                world.set(x, y, z, kind);
            }
        }
    }

    // Ventanas de vidrio con marco de tronco.
    let window = |world: &mut World, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32| {
        fill(world, x0, y0, z0, x1, y1, z1, Kind::Glass);
    };
    for (wx, wz) in [(x0 + 5, z1), (x0 + 19, z1)] {
        window(world, wx, floor + 4, wz, wx + 5, floor + 7, wz);
    }
    for (wx, wz) in [(x0 + 6, z0), (x0 + 18, z0)] {
        window(world, wx, floor + 4, wz, wx + 5, floor + 7, wz);
    }
    for wz in [z0 + 6, z0 + 15] {
        window(world, x0, floor + 4, wz, x0, floor + 7, wz + 4);
        window(world, x1, floor + 4, wz, x1, floor + 7, wz + 4);
    }

    // Puerta hacia la terraza.
    fill(world, x0 + 13, floor + 1, z1, x0 + 16, floor + 6, z1, Kind::Air);
    fill(world, x0 + 12, floor + 1, z1, x0 + 12, floor + 7, z1, Kind::Log);
    fill(world, x0 + 17, floor + 1, z1, x0 + 17, floor + 7, z1, Kind::Log);

    // Faroles: uno adentro y uno en la terraza.
    world.set(x0 + W / 2, floor + 7, z0 + D / 2, Kind::Lantern);
    world.set(x1 + 4, floor + 1, z1 + DECK, Kind::Lantern);
    world.set(x0 - 5, floor + 1, z1 + DECK, Kind::Lantern);

    // Techo a dos aguas con alero.
    let half = (W + 7) / 2;
    let mut ridge_y = wall_top;
    for i in 0..half {
        let y = wall_top + 1 + i;
        let (left, right) = (x0 - 4 + i, x1 + 4 - i);
        if left > right {
            break;
        }
        ridge_y = y;
        for z in (z0 - 3)..=(z1 + 3) {
            world.set(left, y, z, Kind::Roof);
            world.set(right, y, z, Kind::Roof);
            // Alero más grueso en el borde.
            if i == 0 {
                world.set(left, y - 1, z, Kind::Roof);
                world.set(right, y - 1, z, Kind::Roof);
            }
        }
        for x in (left + 1)..right {
            world.set(x, y, z0, Kind::Plank);
            world.set(x, y, z1, Kind::Plank);
        }
    }
    fill(world, x0 + W / 2 - 2, ridge_y, z0 - 3, x0 + W / 2 + 1, ridge_y, z1 + 3, Kind::Roof);

    // Ventana del ático en el frontón.
    fill(world, x0 + W / 2 - 2, wall_top + 3, z1, x0 + W / 2 + 1, wall_top + 5, z1, Kind::Glass);

    // Chimenea de piedra.
    fill(world, x1 - 5, floor, z0 + 4, x1 - 3, ridge_y + 4, z0 + 6, Kind::Stone);

    // Barandal de la terraza con postes de tronco.
    for z in (z1 + 2)..=(z1 + DECK) {
        for x in [x0 - 5, x1 + 4] {
            world.set(x, floor + 1, z, Kind::Plank);
            if (z - z1) % 5 == 0 {
                fill(world, x, floor + 1, z, x, floor + 3, z, Kind::Log);
            }
        }
    }
    for x in (x0 - 5)..=(x1 + 4) {
        world.set(x, floor + 1, z1 + DECK, Kind::Plank);
        if (x - x0) % 5 == 0 {
            fill(world, x, floor + 1, z1 + DECK, x, floor + 3, z1 + DECK, Kind::Log);
        }
    }

    // Jardineras con flores sobre la terraza.
    for x in [x0 - 3, x0 + 2, x1 - 2, x1 + 2] {
        fill(world, x, floor + 1, z1 + 2, x + 1, floor + 1, z1 + 3, Kind::Roof);
        fill(world, x, floor + 2, z1 + 2, x + 1, floor + 2, z1 + 3, Kind::Flower);
    }

    // Escalera de tablones bajando al terreno.
    let mut y = floor;
    let mut x = x0 - 6;
    while y > world.top(x, z0 + 8) && x > 8 {
        fill(world, x, y, z0 + 8, x, y, z0 + 12, Kind::Plank);
        y -= 1;
        x -= 1;
    }

    world.reserve(x0 - 10, z0 - 8, x1 + 8, z1 + DECK + 2);
    floor
}

/// Muelle de tablones que entra a la laguna.
fn pier(world: &mut World, floor: i32) {
    let y = (WATER_LEVEL + 4).min(floor);
    let (x0, x1) = (62, 68);
    for z in 112..=142 {
        fill(world, x0, y, z, x1, y, z, Kind::Plank);
        for yy in (y + 1)..(y + 8) {
            fill(world, x0, yy, z, x1, yy, z, Kind::Air);
        }
        if z % 8 == 0 {
            for yy in (WATER_LEVEL - 8)..y {
                world.set(x0, yy, z, Kind::Log);
                world.set(x1, yy, z, Kind::Log);
            }
        }
    }
    // Farol al final del muelle.
    fill(world, x0, y + 1, 142, x0, y + 3, 142, Kind::Log);
    world.set(x0, y + 4, 142, Kind::Lantern);
    world.reserve(x0 - 4, 108, x1 + 4, 146);
}

/// Bote de madera en la laguna.
fn boat(world: &mut World, cx: i32, cz: i32) {
    let y = WATER_LEVEL;
    for dz in -9i32..=9 {
        // Casco más angosto en proa y popa.
        let width = match dz.abs() {
            0..=4 => 4,
            5..=6 => 3,
            7 => 2,
            _ => 1,
        };
        for dx in -width..=width {
            let (x, z) = (cx + dx, cz + dz);
            fill(world, x, y - 2, z, x, y - 1, z, Kind::Plank);
            let hull = dx.abs() == width || dz.abs() >= 8;
            if hull {
                fill(world, x, y, z, x, y + 1, z, Kind::Plank);
            } else {
                fill(world, x, y, z, x, y + 2, z, Kind::Air);
            }
        }
    }
    // Bancas.
    for dz in [-4, 2] {
        fill(world, cx - 3, y, cz + dz, cx + 3, y, cz + dz, Kind::Plank);
    }
    world.reserve(cx - 6, cz - 11, cx + 6, cz + 11);
}

/// Camino de piedra desde la escalera de la cabaña hacia el muelle.
fn stone_path(world: &mut World) {
    let mut x = 78.0f32;
    let mut z = 66.0f32;
    for step in 0..120 {
        let t = step as f32;
        x -= 0.12 + (t * 0.05).sin() * 0.25;
        z += 0.55;
        let (ix, iz) = (x.round() as i32, z.round() as i32);
        for dz in -2..=2 {
            for dx in -2..=2 {
                if dx * dx + dz * dz > 5 {
                    continue;
                }
                let (px, pz) = (ix + dx, iz + dz);
                let ground = world.top(px, pz);
                if ground > WATER_LEVEL + 1 {
                    world.set(px, ground, pz, Kind::Stone);
                    world.reserve(px, pz, px, pz);
                }
            }
        }
    }
}

/// Poste con letreros.
fn signpost(world: &mut World, x: i32, z: i32) {
    let ground = world.top(x, z);
    if ground <= WATER_LEVEL {
        return;
    }
    fill(world, x, ground + 1, z, x, ground + 11, z, Kind::Log);
    fill(world, x + 1, ground + 10, z, x + 5, ground + 11, z, Kind::Plank);
    fill(world, x - 5, ground + 6, z, x - 1, ground + 7, z, Kind::Plank);
    world.reserve(x - 7, z - 3, x + 7, z + 3);
}

/// Banca de madera mirando a la laguna.
fn bench(world: &mut World, x: i32, z: i32) {
    let ground = world.top(x, z);
    if ground <= WATER_LEVEL + 1 {
        return;
    }
    fill(world, x, ground + 1, z, x + 7, ground + 1, z + 1, Kind::Plank);
    fill(world, x, ground + 2, z + 1, x + 7, ground + 3, z + 1, Kind::Plank);
    for px in [x, x + 7] {
        world.set(px, ground + 1, z, Kind::Log);
    }
    world.reserve(x - 2, z - 2, x + 9, z + 3);
}

fn leaf_kind(seed: f32) -> Kind {
    if seed < 0.40 {
        Kind::LeafGreen
    } else if seed < 0.72 {
        Kind::LeafPink
    } else {
        Kind::LeafOrange
    }
}

/// Copa esponjosa: esfera deformada con ruido 3D.
fn canopy(world: &mut World, cx: i32, cy: i32, cz: i32, radius: f32, leaf: Kind) {
    let r = radius.ceil() as i32 + 1;
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                let p = Vec3::new((cx + dx) as f32, (cy + dy) as f32, (cz + dz) as f32);
                let wobble = (fbm_3d(p * 0.22, 3, 401) - 0.5) * radius * 0.85;
                // Copa un poco achatada.
                let squash = 1.0 + (dy.max(0) as f32) * 0.06;
                if d * squash <= radius + wobble && world.is_air(cx + dx, cy + dy, cz + dz) {
                    world.set(cx + dx, cy + dy, cz + dz, leaf);
                }
            }
        }
    }
}

/// Árbol frondoso con tronco grueso y algunas ramas.
fn broadleaf(world: &mut World, x: i32, z: i32, ground: i32, height: i32, leaf: Kind) {
    for y in (ground + 1)..=(ground + height) {
        fill(world, x, y, z, x + 1, y, z + 1, Kind::Log);
    }
    let top = ground + height;
    let radius = 4.5 + height as f32 * 0.30;

    // Ramas hacia dos lados.
    for (dx, dz) in [(2, 0), (-2, 1)] {
        let by = top - 3;
        fill(world, x + dx.min(0), by, z + dz.min(0), x + dx.max(0), by, z + dz.max(0), Kind::Log);
    }

    canopy(world, x, top + 1, z, radius, leaf);
}

/// Conífera: capas de hojas que se cierran hacia la punta.
fn conifer(world: &mut World, x: i32, z: i32, ground: i32, height: i32) {
    fill(world, x, ground + 1, z, x, ground + height, z, Kind::Log);

    let layers = height - 3;
    for i in 0..layers {
        let y = ground + 4 + i;
        let t = i as f32 / layers as f32;
        // Contorno de cono con un leve escalonado entre ramas.
        let step = if i % 3 == 0 { 0.9 } else { 0.0 };
        let r = (6.2 * (1.0 - t * 0.92) + step) as i32;
        for dz in -r..=r {
            for dx in -r..=r {
                let d2 = dx * dx + dz * dz;
                if d2 > r * r || hash_2i(x + dx, z + dz + y * 13, 71) < 0.12 {
                    continue;
                }
                if world.is_air(x + dx, y, z + dz) {
                    world.set(x + dx, y, z + dz, Kind::LeafGreen);
                }
            }
        }
    }
    fill(world, x, ground + height + 1, z, x, ground + height + 2, z, Kind::LeafGreen);
}

/// Mata redonda de arbusto.
fn bush(world: &mut World, x: i32, z: i32, ground: i32, leaf: Kind) {
    let r = 2 + (hash_2i(x, z, 131) * 2.0) as i32;
    for dy in 0..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                if d <= r as f32 + hash_3i(x + dx, dy, z + dz, 137) - 0.3
                    && world.is_air(x + dx, ground + 1 + dy, z + dz)
                {
                    world.set(x + dx, ground + 1 + dy, z + dz, leaf);
                }
            }
        }
    }
}

fn vegetation(world: &mut World) {
    let mut placed: Vec<(i32, i32)> = Vec::new();

    for z in 4..(SIZE_Z - 4) {
        for x in 4..(SIZE_X - 4) {
            let ground = world.top(x, z);
            if world.get(x, ground, z) != Kind::Grass.id() || !world.is_air(x, ground + 1, z) {
                continue;
            }
            if world.is_reserved(x, z) {
                continue;
            }

            let forest = fbm_2d(x as f32 * 0.028, z as f32 * 0.028, 3, 41);
            let roll = hash_2i(x, z, 101);

            // Bosque cerrado arriba, árboles sueltos en el prado.
            let (density, spacing) = if forest > 0.56 { (0.20, 13) } else { (0.05, 20) };

            if roll < density {
                let close = placed
                    .iter()
                    .any(|&(px, pz)| (px - x).abs() < spacing && (pz - z).abs() < spacing);
                if !close {
                    placed.push((x, z));
                    let pick = hash_2i(x, z, 103);
                    if ground > 62 || pick < 0.30 {
                        conifer(world, x, z, ground, 14 + (pick * 12.0) as i32);
                    } else {
                        broadleaf(world, x, z, ground, 11 + (pick * 9.0) as i32, leaf_kind(hash_2i(x, z, 107)));
                    }
                    continue;
                }
            }

            // Arbustos en grupos.
            let patch = fbm_2d(x as f32 * 0.09, z as f32 * 0.09, 2, 77);
            if patch > 0.60 && roll > 0.94 {
                let close = placed.iter().any(|&(px, pz)| (px - x).abs() < 5 && (pz - z).abs() < 5);
                if !close {
                    placed.push((x, z));
                    bush(world, x, z, ground, leaf_kind(hash_2i(x, z, 151) * 0.9));
                    continue;
                }
            }

            // Hierba alta y flores.
            if roll > 0.975 {
                fill(world, x, ground + 1, z, x, ground + 1 + (roll > 0.99) as i32, z, Kind::Flower);
            } else if patch > 0.52 && roll > 0.72 {
                let h = 1 + (roll > 0.90) as i32;
                fill(world, x, ground + 1, z, x, ground + h, z, Kind::LeafGreen);
            }
        }
    }
}

/// Juncos en la orilla y nenúfares sobre el agua.
fn shoreline(world: &mut World) {
    for z in 2..SIZE_Z - 2 {
        for x in 2..SIZE_X - 2 {
            // Nenúfares.
            if world.is_water(x, WATER_LEVEL, z) && world.is_air(x, WATER_LEVEL + 1, z) {
                if hash_2i(x, z, 211) < 0.012 {
                    world.set(x, WATER_LEVEL + 1, z, Kind::LeafGreen);
                }
                continue;
            }

            // Juncos: arena junto al agua.
            let ground = world.top(x, z);
            if ground < WATER_LEVEL || ground > WATER_LEVEL + 3 || !world.is_air(x, ground + 1, z) {
                continue;
            }
            if world.is_reserved(x, z) {
                continue;
            }
            let near_water = [(2, 0), (-2, 0), (0, 2), (0, -2), (3, 3), (-3, -3)]
                .iter()
                .any(|&(dx, dz)| world.is_water(x + dx, WATER_LEVEL, z + dz));
            if !near_water {
                continue;
            }

            let roll = hash_2i(x, z, 223);
            if roll < 0.20 {
                let h = 2 + (roll * 10.0) as i32;
                fill(world, x, ground + 1, z, x, ground + h, z, Kind::LeafGreen);
                if roll < 0.06 {
                    world.set(x, ground + h + 1, z, Kind::Flower);
                }
            }
        }
    }
}
