//! Construcción del diorama: terreno, laguna, cabaña, vegetación y detalles.

use crate::material::Kind;
use crate::noise::{fbm_2d, fbm_3d, hash_2i, hash_3i};
use crate::vec3::Vec3;
use crate::voxel::Grid;

/// Resolución del diorama: cuántos cuartos de voxel mide cada unidad de diseño.
/// 4 = tamaño base, 5 = bloques 25% más pequeños, 6 = 50% más pequeños.
/// Subirlo da más detalle y cuesta más; bajarlo va más rápido.
const DETAIL: i32 = 4;

/// Unidades de diseño a voxeles.
const fn u(v: i32) -> i32 {
    v * DETAIL / 4
}

/// Factor de escala como número real (coordenadas y ruido).
const K: f32 = DETAIL as f32 / 4.0;

pub const SIZE_X: i32 = u(176);
pub const SIZE_Y: i32 = u(112);
pub const SIZE_Z: i32 = u(176);

/// Superficie del agua (en voxeles).
pub const WATER_LEVEL: i32 = u(32);
/// Alcance de la luz de un farol, en voxeles.
pub const LIGHT_RADIUS: f32 = u(42) as f32;
/// Roca maciza sobre la que se apoya el diorama.
const BEDROCK: i32 = u(12);

/// Qué tanto entra la laguna en una columna (positivo = agua).
/// Todas las fórmulas del terreno se evalúan en unidades de diseño, para que la
/// composición no cambie al variar DETAIL.
fn lagoon_field(x: f32, z: f32) -> f32 {
    let (x, z) = (x / K, z / K);
    let wobble = (fbm_2d(x * 0.034, z * 0.034, 3, 21) - 0.5) * 0.50;
    // Elipse grande al frente, abierta en el borde del diorama.
    let d = (((x - 72.0) / 76.0).powi(2) + ((z - 134.0) / 66.0).powi(2)).sqrt();
    (1.0 - d) + wobble
}

fn terrain_height(x_vox: f32, z_vox: f32) -> f32 {
    let (x, z) = (x_vox / K, z_vox / K);
    let rolling = fbm_2d(x * 0.024, z * 0.024, 4, 5);
    let detail = fbm_2d(x * 0.065 + 5.0, z * 0.065, 4, 9);
    let ripple = fbm_2d(x * 0.21, z * 0.21, 2, 33) - 0.5;
    let mut h = 34.0 + rolling * 9.0 + detail * 4.0 + ripple * 2.2;

    // Colina rocosa al fondo.
    let back = ((70.0 - z) / 70.0).clamp(0.0, 1.0);
    h += back * back * 24.0;

    // Cauce del lago. Recibe voxeles: la propia función pasa a diseño.
    h -= lagoon_field(x_vox, z_vox).max(0.0) * 28.0;

    // La altura se calcula en diseño y se lleva a voxeles.
    (h * K).clamp(BEDROCK as f32 + 2.0, (SIZE_Y - u(20)) as f32)
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
    // El origen centra el diorama en el mundo, para que la cámara orbite
    // siempre alrededor de su centro aunque cambie el nivel de detalle.
    let origin = Vec3::new(
        -(SIZE_X as f32) / 2.0,
        -(SIZE_Y as f32) * 0.45,
        -(SIZE_Z as f32) / 2.0,
    );
    let grid = Grid::new([SIZE_X, SIZE_Y, SIZE_Z], origin);
    let mut world =
        World { grid, height: vec![0; (SIZE_X * SIZE_Z) as usize], reserved: Vec::new() };

    terrain(&mut world);
    outcrops(&mut world);
    let floor = cabin(&mut world, u(88), u(46));
    pier(&mut world, floor);
    boat(&mut world, u(52), u(128));
    stone_path(&mut world);
    signpost(&mut world, u(44), u(62));
    bench(&mut world, u(140), u(104));
    vegetation(&mut world);
    shoreline(&mut world);

    // Índice de espacio vacío para acelerar el trazado de rayos.
    world.grid.rebuild_coarse();

    let (mut solid, mut top_max) = (0usize, 0i32);
    for z in 0..SIZE_Z {
        for x in 0..SIZE_X {
            top_max = top_max.max(world.top(x, z));
            for y in 0..SIZE_Y {
                if !world.is_air(x, y, z) {
                    solid += 1;
                }
            }
        }
    }
    println!(
        "mundo {}x{}x{} voxeles | agua en y={} | terreno más alto y={} | {} bloques sólidos",
        SIZE_X, SIZE_Y, SIZE_Z, WATER_LEVEL, top_max, solid
    );

    world.grid
}

fn terrain(world: &mut World) {
    for z in 0..SIZE_Z {
        for x in 0..SIZE_X {
            let h = terrain_height(x as f32, z as f32).round() as i32;

            for y in 0..=h {
                let shore = (h - WATER_LEVEL).abs() <= u(2);
                // Estratos horizontales en la roca del zócalo del diorama.
                let strata = {
                    let band = (y as f32 * 0.16 / K
                        + fbm_2d(x as f32 * 0.05 / K, z as f32 * 0.05 / K, 2, 87) * 1.4)
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
            if h <= WATER_LEVEL + u(2) {
                continue;
            }
            let slope = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|&(dx, dz)| (h - world.top(x + dx, z + dz)).abs())
                .max()
                .unwrap_or(0);
            let noise = fbm_2d(x as f32 * 0.06 / K, z as f32 * 0.06 / K, 3, 63);

            let ridge = z < u(44) && h > u(52);
            if slope >= u(4) || (h > u(62) && noise > 0.70) || (ridge && slope >= u(3) && noise > 0.55) {
                for y in (h - u(2))..=h {
                    world.set(x, y, z, Kind::Stone);
                }
            }
        }
    }

    // Peñascos redondeados sobre el pasto.
    for i in 0..40 {
        let x = u(8) + (hash_2i(i, 0, 301) * (SIZE_X - u(16)) as f32) as i32;
        let z = u(8) + (hash_2i(i, 1, 307) * (SIZE_Z - u(16)) as f32) as i32;
        let ground = world.top(x, z);
        if ground <= WATER_LEVEL + 1 {
            continue;
        }
        let r = u(2) + (hash_2i(i, 2, 311) * 3.0 * K) as i32;
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
                    let kind = if yy < y - u(6) { Kind::Stone } else { Kind::Dirt };
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
    const W: i32 = u(44);
    const D: i32 = u(36);
    const DECK: i32 = u(28);

    let (x1, z1) = (x0 + W - 1, z0 + D - 1);
    let ground = world.top(x0 + W / 2, z0 + u(4)).max(WATER_LEVEL + u(8));

    flatten(world, x0 - u(6), z0 - u(6), x1 + u(6), z1, ground);
    let floor = ground + 1;

    // Terraza de tablones que se proyecta sobre el agua, sobre pilotes.
    fill(world, x0 - u(5), floor, z0 - u(2), x1 + u(4), floor, z1 + DECK, Kind::Plank);
    for pz in [z1 + DECK, z1 + DECK - u(7), z1 + DECK - u(14), z1 + DECK - u(21), z1 + u(3)] {
        for px in [x0 - u(5), x0 + u(8), x0 + u(21), x1 - u(8), x1 + u(4)] {
            stilt(world, px, pz, floor - 1);
        }
    }

    // Muros con esquinas de tronco y zócalo de ladrillo.
    let wall_top = floor + u(11);
    for y in (floor + 1)..=wall_top {
        for z in z0..=z1 {
            for x in x0..=x1 {
                if x != x0 && x != x1 && z != z0 && z != z1 {
                    continue;
                }
                let corner = (x - x0).min(x1 - x).min((z - z0).min(z1 - z)) < u(3);
                let kind = if corner {
                    Kind::Log
                } else if y <= floor + u(3) {
                    Kind::Roof
                } else {
                    Kind::Plank
                };
                world.set(x, y, z, kind);
            }
        }
    }

    // Entramado de vigas: evita que el muro quede como un panel liso.
    for y in [floor + u(4), floor + u(8)] {
        for z in z0..=z1 {
            for x in x0..=x1 {
                if x == x0 || x == x1 || z == z0 || z == z1 {
                    world.set(x, y, z, Kind::Beam);
                }
            }
        }
    }
    for x in (x0..=x1).step_by(u(11) as usize) {
        fill(world, x, floor + u(4), z0, x, wall_top, z0, Kind::Beam);
        fill(world, x, floor + u(4), z1, x, wall_top, z1, Kind::Beam);
    }
    for z in (z0..=z1).step_by(u(11) as usize) {
        fill(world, x0, floor + u(4), z, x0, wall_top, z, Kind::Beam);
        fill(world, x1, floor + u(4), z, x1, wall_top, z, Kind::Beam);
    }

    // Ventanales con marco de tronco.
    let window = |world: &mut World, ax0: i32, ay0: i32, az0: i32, ax1: i32, ay1: i32, az1: i32| {
        let (fx0, fx1) = if ax0 == ax1 { (ax0, ax1) } else { (ax0 - 1, ax1 + 1) };
        let (fz0, fz1) = if az0 == az1 { (az0, az1) } else { (az0 - 1, az1 + 1) };
        fill(world, fx0, ay0 - 1, fz0, fx1, ay1 + 1, fz1, Kind::Beam);
        fill(world, ax0, ay0, az0, ax1, ay1, az1, Kind::Glass);
    };
    for wx in [x0 + u(5), x0 + u(32)] {
        window(world, wx, floor + u(5), z1, wx + u(7), floor + u(10), z1);
    }
    for wx in [x0 + u(7), x0 + u(28)] {
        window(world, wx, floor + u(5), z0, wx + u(8), floor + u(10), z0);
    }
    for wz in [z0 + u(7), z0 + u(22)] {
        window(world, x0, floor + u(5), wz, x0, floor + u(10), wz + u(6));
        window(world, x1, floor + u(5), wz, x1, floor + u(10), wz + u(6));
    }

    // Puerta hacia la terraza, con jambas y dintel de tronco.
    fill(world, x0 + u(19), floor + 1, z1, x0 + u(24), floor + u(8), z1, Kind::Air);
    fill(world, x0 + u(18), floor + 1, z1, x0 + u(18), floor + u(9), z1, Kind::Log);
    fill(world, x0 + u(25), floor + 1, z1, x0 + u(25), floor + u(9), z1, Kind::Log);
    fill(world, x0 + u(18), floor + u(9), z1, x0 + u(25), floor + u(9), z1, Kind::Log);

    // Faroles: uno adentro y dos en la terraza.
    for (lx, lz) in [(u(11), u(9)), (u(33), u(9)), (u(22), u(26))] {
        world.set(x0 + lx, floor + u(9), z0 + lz, Kind::Lantern);
    }
    world.set(x1 + u(4), floor + 1, z1 + DECK, Kind::Lantern);
    world.set(x0 - u(5), floor + 1, z1 + DECK, Kind::Lantern);

    // Techo a dos aguas con alero.
    let half = (W + u(7)) / 2;
    let mut ridge_y = wall_top;
    for i in 0..half {
        let y = wall_top + 1 + i;
        let (left, right) = (x0 - u(4) + i, x1 + u(4) - i);
        if left > right {
            break;
        }
        ridge_y = y;
        for z in (z0 - u(3))..=(z1 + u(3)) {
            world.set(left, y, z, Kind::Roof);
            world.set(right, y, z, Kind::Roof);
            if i == 0 {
                world.set(left, y - 1, z, Kind::Roof);
                world.set(right, y - 1, z, Kind::Roof);
            }
        }
        // Frontón: tablones con algunas vigas verticales, como el muro.
        for x in (left + 1)..right {
            let beam = (x - x0).rem_euclid(u(11)) == 0 || i == 0;
            let kind = if beam { Kind::Beam } else { Kind::Plank };
            world.set(x, y, z0, kind);
            world.set(x, y, z1, kind);
        }
    }
    fill(world, x0 + W / 2 - u(2), ridge_y, z0 - u(3), x0 + W / 2 + u(1), ridge_y, z1 + u(3), Kind::Roof);

    // Ventana del ático en el frontón.
    fill(world, x0 + W / 2 - u(4), wall_top + u(4), z1, x0 + W / 2 + u(3), wall_top + u(8), z1, Kind::Glass);

    // Chimenea de piedra.
    // Pegada a la cumbrera, para que se lea como parte del techo.
    let cx = x0 + W / 2 + u(5);
    fill(world, cx, floor, z0 + u(5), cx + u(3), ridge_y + u(5), z0 + u(8), Kind::Stone);

    // Barandal de la terraza con postes de tronco.
    for z in (z1 + u(2))..=(z1 + DECK) {
        for x in [x0 - u(5), x1 + u(4)] {
            world.set(x, floor + 1, z, Kind::Plank);
            if (z - z1) % u(5) == 0 {
                fill(world, x, floor + 1, z, x, floor + u(3), z, Kind::Log);
            }
        }
    }
    for x in (x0 - u(5))..=(x1 + u(4)) {
        world.set(x, floor + 1, z1 + DECK, Kind::Plank);
        if (x - x0) % u(5) == 0 {
            fill(world, x, floor + 1, z1 + DECK, x, floor + u(3), z1 + DECK, Kind::Log);
        }
    }

    // Jardineras pegadas a la fachada.
    for x in [x0 - u(3), x0 + u(2), x1 - u(2), x1 + u(2)] {
        fill(world, x, floor + 1, z1 + u(2), x + u(2) - 1, floor + 1, z1 + u(3), Kind::Roof);
        fill(world, x, floor + 2, z1 + u(2), x + u(2) - 1, floor + 2, z1 + u(3), Kind::Flower);
    }

    // Mesa, sillas y macetas en la terraza.
    let (tx, tz) = (x0 + u(12), z1 + u(12));
    table(world, tx, tz, floor);
    chair(world, tx - u(4), tz + u(1), floor, 3);
    chair(world, tx + u(7), tz + u(1), floor, 2);
    chair(world, tx + u(2), tz + u(6), floor, 1);
    for (px, pz) in [(x0 - u(4), z1 + DECK - u(2)), (x1 + u(2), z1 + DECK - u(2)), (x1 + u(2), z1 + u(6))] {
        planter(world, px, pz, floor);
    }

    // Faroles de poste a los lados de la casa.
    for (lx, lz) in [(x0 - u(8), z1 + u(4)), (x1 + u(6), z1 + u(16))] {
        let ground = world.top(lx, lz).max(WATER_LEVEL);
        fill(world, lx, ground + 1, lz, lx, ground + u(7), lz, Kind::Log);
        world.set(lx, ground + u(8), lz, Kind::Lantern);
    }

    // Escalera de tablones bajando al terreno.
    let mut y = floor;
    let mut x = x0 - u(6);
    while y > world.top(x, z0 + u(8)) && x > u(8) {
        fill(world, x, y, z0 + u(8), x, y, z0 + u(12), Kind::Plank);
        y -= 1;
        x -= 1;
    }

    world.reserve(x0 - u(10), z0 - u(8), x1 + u(8), z1 + DECK + u(2));
    floor
}

/// Muelle de tablones que entra al lago.
fn pier(world: &mut World, floor: i32) {
    let y = (WATER_LEVEL + u(4)).min(floor);
    let (x0, x1) = (u(62), u(68));
    for z in u(112)..=u(142) {
        fill(world, x0, y, z, x1, y, z, Kind::Plank);
        for yy in (y + 1)..(y + u(8)) {
            fill(world, x0, yy, z, x1, yy, z, Kind::Air);
        }
        if (z - u(112)) % u(8) == 0 {
            for yy in (WATER_LEVEL - u(8))..y {
                world.set(x0, yy, z, Kind::Log);
                world.set(x1, yy, z, Kind::Log);
            }
        }
    }
    // Faroles a lo largo del muelle.
    for pz in [u(126), u(142)] {
        fill(world, x0, y + 1, pz, x0, y + u(3), pz, Kind::Log);
        world.set(x0, y + u(4), pz, Kind::Lantern);
    }
    world.reserve(x0 - u(4), u(108), x1 + u(4), u(146));
}

/// Bote de madera en el lago.
fn boat(world: &mut World, cx: i32, cz: i32) {
    let y = WATER_LEVEL;
    for dz in -u(9)..=u(9) {
        // Casco más angosto en proa y popa.
        let along = (dz as f32 / K).abs() as i32;
        let width = match along {
            0..=4 => u(4),
            5..=6 => u(3),
            7 => u(2),
            _ => u(1),
        };
        for dx in -width..=width {
            let (x, z) = (cx + dx, cz + dz);
            fill(world, x, y - u(2), z, x, y - 1, z, Kind::Plank);
            let hull = dx.abs() == width || along >= 8;
            if hull {
                fill(world, x, y, z, x, y + u(1), z, Kind::Plank);
            } else {
                fill(world, x, y, z, x, y + u(2), z, Kind::Air);
            }
        }
    }
    // Bancas.
    for dz in [-u(4), u(2)] {
        fill(world, cx - u(3), y, cz + dz, cx + u(3), y, cz + dz, Kind::Plank);
    }

    // Faroles en la proa y en la popa, sobre postes.
    for dz in [-u(6), u(6)] {
        fill(world, cx, y + 1, cz + dz, cx, y + u(4), cz + dz, Kind::Log);
        world.set(cx, y + u(5), cz + dz, Kind::Lantern);
    }
    world.reserve(cx - u(6), cz - u(11), cx + u(6), cz + u(11));
}

/// Camino de piedra que baja hacia el muelle.
fn stone_path(world: &mut World) {
    let (mut x, mut z) = (u(78) as f32, u(66) as f32);
    for step in 0..(120.0 * K) as i32 {
        let t = step as f32 / K;
        x -= (0.12 + (t * 0.05).sin() * 0.25) * K;
        z += 0.55 * K;
        let (ix, iz) = (x.round() as i32, z.round() as i32);
        let r = u(2);
        for dz in -r..=r {
            for dx in -r..=r {
                if dx * dx + dz * dz > r * r + 1 {
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
    fill(world, x, ground + 1, z, x, ground + u(11), z, Kind::Log);
    fill(world, x + 1, ground + u(10), z, x + u(5), ground + u(11), z, Kind::Plank);
    fill(world, x - u(5), ground + u(6), z, x - 1, ground + u(7), z, Kind::Plank);
    world.reserve(x - u(7), z - u(3), x + u(7), z + u(3));
}

/// Banca de madera mirando al lago.
fn bench(world: &mut World, x: i32, z: i32) {
    let ground = world.top(x, z);
    if ground <= WATER_LEVEL + 1 {
        return;
    }
    fill(world, x, ground + u(1), z, x + u(7), ground + u(1), z + u(1), Kind::Plank);
    fill(world, x, ground + u(2), z + u(1), x + u(7), ground + u(3), z + u(1), Kind::Plank);
    for px in [x, x + u(7)] {
        fill(world, px, ground + 1, z, px, ground + u(1), z, Kind::Log);
    }
    world.reserve(x - u(2), z - u(2), x + u(9), z + u(3));
}

/// Mesa de tablones con patas de tronco.
fn table(world: &mut World, x: i32, z: i32, floor: i32) {
    for (dx, dz) in [(0, 0), (u(5), 0), (0, u(4)), (u(5), u(4))] {
        fill(world, x + dx, floor + 1, z + dz, x + dx, floor + u(2), z + dz, Kind::Log);
    }
    fill(world, x, floor + u(3), z, x + u(5), floor + u(3), z + u(4), Kind::Plank);
}

/// Silla mirando hacia `face`: 0 = -z, 1 = +z, 2 = -x, 3 = +x.
fn chair(world: &mut World, x: i32, z: i32, floor: i32, face: u8) {
    let s = u(2);
    fill(world, x, floor + 1, z, x + s, floor + 1, z + s, Kind::Log);
    fill(world, x, floor + u(2), z, x + s, floor + u(2), z + s, Kind::Plank);
    match face {
        0 => fill(world, x, floor + u(3), z + s, x + s, floor + u(4), z + s, Kind::Plank),
        1 => fill(world, x, floor + u(3), z, x + s, floor + u(4), z, Kind::Plank),
        2 => fill(world, x + s, floor + u(3), z, x + s, floor + u(4), z + s, Kind::Plank),
        _ => fill(world, x, floor + u(3), z, x, floor + u(4), z + s, Kind::Plank),
    }
}

/// Maceta de barro con flores.
fn planter(world: &mut World, x: i32, z: i32, floor: i32) {
    let s = u(2) - 1;
    fill(world, x, floor + 1, z, x + s, floor + u(2), z + s, Kind::Roof);
    fill(world, x, floor + u(3), z, x + s, floor + u(3), z + s, Kind::Flower);
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
                let wobble = (fbm_3d(p * (0.22 / K), 3, 401) - 0.5) * radius * 0.85;
                let squash = 1.0 + (dy.max(0) as f32) * 0.06;
                if d * squash <= radius + wobble && world.is_air(cx + dx, cy + dy, cz + dz) {
                    world.set(cx + dx, cy + dy, cz + dz, leaf);
                }
            }
        }
    }
}

/// Árbol frondoso: `height` va en unidades de diseño.
fn broadleaf(world: &mut World, x: i32, z: i32, ground: i32, height: i32, leaf: Kind) {
    let h = u(height);
    let thick = u(2) - 1;
    fill(world, x, ground + 1, z, x + thick, ground + h, z + thick, Kind::Log);

    let top = ground + h;
    let radius = (4.5 + height as f32 * 0.30) * K;

    // Ramas hacia dos lados.
    for (dx, dz) in [(u(2), 0), (-u(2), u(1))] {
        let by = top - u(3);
        fill(world, x + dx.min(0), by, z + dz.min(0), x + dx.max(0), by, z + dz.max(0), Kind::Log);
    }

    canopy(world, x, top + u(1), z, radius, leaf);
}

/// Conífera: capas de hojas que se cierran hacia la punta.
fn conifer(world: &mut World, x: i32, z: i32, ground: i32, height: i32) {
    let h = u(height);
    fill(world, x, ground + 1, z, x, ground + h, z, Kind::Log);

    let layers = h - u(3);
    for i in 0..layers {
        let y = ground + u(4) + i;
        let t = i as f32 / layers as f32;
        let step = if (i as f32 / K) as i32 % 3 == 0 { 0.9 } else { 0.0 };
        let r = ((6.2 * (1.0 - t * 0.92) + step) * K) as i32;
        for dz in -r..=r {
            for dx in -r..=r {
                if dx * dx + dz * dz > r * r || hash_2i(x + dx, z + dz + y * 13, 71) < 0.12 {
                    continue;
                }
                if world.is_air(x + dx, y, z + dz) {
                    world.set(x + dx, y, z + dz, Kind::LeafGreen);
                }
            }
        }
    }
    fill(world, x, ground + h + 1, z, x, ground + h + u(2), z, Kind::LeafGreen);
}

/// Mata redonda de arbusto.
fn bush(world: &mut World, x: i32, z: i32, ground: i32, leaf: Kind) {
    let r = u(2) + (hash_2i(x, z, 131) * 2.0 * K) as i32;
    for dy in 0..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                if d <= r as f32 + hash_3i(x + dx, dy, z + dz, 137) * K - 0.3
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
    // Las probabilidades son por columna: al subir el detalle hay más columnas
    // por la misma área, así que se reparten entre K².
    let area = K * K;

    for z in u(4)..(SIZE_Z - u(4)) {
        for x in u(4)..(SIZE_X - u(4)) {
            let ground = world.top(x, z);
            if world.get(x, ground, z) != Kind::Grass.id() || !world.is_air(x, ground + 1, z) {
                continue;
            }
            if world.is_reserved(x, z) {
                continue;
            }

            let (dx, dz) = (x as f32 / K, z as f32 / K);
            let forest = fbm_2d(dx * 0.028, dz * 0.028, 3, 41);
            let roll = hash_2i(x, z, 101);

            // Los árboles guardan margen con el borde para que ninguna copa
            // sobresalga al vacío; los arbustos y la hierba sí llegan a la orilla.
            let margin = u(12);
            let near_edge = x < margin || z < margin || x >= SIZE_X - margin || z >= SIZE_Z - margin;

            let high = ground > u(54);
            let (density, spacing) = if high {
                (0.26, u(11))
            } else if forest > 0.56 {
                (0.20, u(13))
            } else {
                (0.05, u(20))
            };

            if roll < density / area && !near_edge {
                let close = placed
                    .iter()
                    .any(|&(px, pz)| (px - x).abs() < spacing && (pz - z).abs() < spacing);
                if !close {
                    placed.push((x, z));
                    let pick = hash_2i(x, z, 103);
                    if ground > u(62) || pick < 0.30 {
                        conifer(world, x, z, ground, 14 + (pick * 12.0) as i32);
                    } else {
                        broadleaf(world, x, z, ground, 11 + (pick * 9.0) as i32, leaf_kind(hash_2i(x, z, 107)));
                    }
                    continue;
                }
            }

            // Arbustos en grupos.
            let patch = fbm_2d(dx * 0.09, dz * 0.09, 2, 77);
            if patch > 0.60 && roll > 1.0 - 0.06 / area {
                let close = placed.iter().any(|&(px, pz)| (px - x).abs() < u(5) && (pz - z).abs() < u(5));
                if !close {
                    placed.push((x, z));
                    bush(world, x, z, ground, leaf_kind(hash_2i(x, z, 151) * 0.9));
                    continue;
                }
            }

            // Hierba alta y flores.
            if roll > 1.0 - 0.025 / area {
                fill(world, x, ground + 1, z, x, ground + u(2), z, Kind::Flower);
            } else if patch > 0.52 && roll > 1.0 - 0.28 / area {
                fill(world, x, ground + 1, z, x, ground + u(2), z, Kind::LeafGreen);
            }
        }
    }
}

/// Juncos en la orilla y nenúfares sobre el agua.
fn shoreline(world: &mut World) {
    let area = K * K;

    for z in u(2)..SIZE_Z - u(2) {
        for x in u(2)..SIZE_X - u(2) {
            // Nenúfares.
            if world.is_water(x, WATER_LEVEL, z) && world.is_air(x, WATER_LEVEL + 1, z) {
                if hash_2i(x, z, 211) < 0.005 / area {
                    fill(world, x, WATER_LEVEL + 1, z, x + u(2) - 1, WATER_LEVEL + 1, z + u(2) - 1, Kind::LeafGreen);
                }
                continue;
            }

            // Juncos: arena junto al agua.
            let ground = world.top(x, z);
            if ground < WATER_LEVEL || ground > WATER_LEVEL + u(3) || !world.is_air(x, ground + 1, z) {
                continue;
            }
            if world.is_reserved(x, z) {
                continue;
            }
            let near_water = [(u(2), 0), (-u(2), 0), (0, u(2)), (0, -u(2)), (u(3), u(3)), (-u(3), -u(3))]
                .iter()
                .any(|&(dx, dz)| world.is_water(x + dx, WATER_LEVEL, z + dz));
            if !near_water {
                continue;
            }

            let roll = hash_2i(x, z, 223);
            if roll < 0.20 / area {
                let h = u(2) + (roll * 10.0 * K) as i32;
                fill(world, x, ground + 1, z, x, ground + h, z, Kind::LeafGreen);
                if roll < 0.06 / area {
                    world.set(x, ground + h + 1, z, Kind::Flower);
                }
            }
        }
    }
}
