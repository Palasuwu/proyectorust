//! Rejilla de voxeles y su recorrido con DDA (Amanatides & Woo).
//!
//! Cada celda mide 1x1x1 en coordenadas de mundo, así que el diorama entero se
//! recorre saltando de cubo en cubo en vez de probar miles de cubos uno por uno.

use crate::vec3::Vec3;

const EPSILON: f32 = 1e-4;

pub struct VoxelHit {
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub id: u8,
    pub cell: [i32; 3],
}

pub struct Grid {
    pub size: [i32; 3],
    /// Esquina mínima de la rejilla en coordenadas de mundo.
    pub origin: Vec3,
    cells: Vec<u8>,
}

fn axis(v: Vec3, i: usize) -> f32 {
    match i {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

impl Grid {
    pub fn new(size: [i32; 3], origin: Vec3) -> Self {
        let total = (size[0] * size[1] * size[2]) as usize;
        Self { size, origin, cells: vec![0; total] }
    }

    fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.size[0] && y < self.size[1] && z < self.size[2]
    }

    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        ((y * self.size[2] + z) * self.size[0] + x) as usize
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if self.in_bounds(x, y, z) { self.cells[self.index(x, y, z)] } else { 0 }
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u8) {
        if self.in_bounds(x, y, z) {
            let i = self.index(x, y, z);
            self.cells[i] = id;
        }
    }

    pub fn min(&self) -> Vec3 {
        self.origin
    }

    pub fn max(&self) -> Vec3 {
        self.origin + Vec3::new(self.size[0] as f32, self.size[1] as f32, self.size[2] as f32)
    }

    /// Intersección con la caja que envuelve toda la rejilla.
    /// Devuelve (t de entrada, t de salida, normal de la cara de entrada).
    fn bounds_hit(&self, origin: Vec3, dir: Vec3) -> Option<(f32, f32, Vec3)> {
        let (min, max) = (self.min(), self.max());
        let mut t_near = f32::NEG_INFINITY;
        let mut t_far = f32::INFINITY;
        let mut entry_axis = 0usize;
        let mut entry_sign = 0.0f32;

        for i in 0..3 {
            let d = axis(dir, i);
            let o = axis(origin, i);
            if d.abs() < 1e-9 {
                if o < axis(min, i) || o > axis(max, i) {
                    return None;
                }
                continue;
            }
            let mut t1 = (axis(min, i) - o) / d;
            let mut t2 = (axis(max, i) - o) / d;
            let mut sign = -1.0;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                sign = 1.0;
            }
            if t1 > t_near {
                t_near = t1;
                entry_axis = i;
                entry_sign = sign;
            }
            t_far = t_far.min(t2);
            if t_near > t_far {
                return None;
            }
        }

        if t_far < 0.0 {
            return None;
        }

        let mut normal = Vec3::default();
        match entry_axis {
            0 => normal.x = entry_sign,
            1 => normal.y = entry_sign,
            _ => normal.z = entry_sign,
        }
        Some((t_near, t_far, normal))
    }

    /// Lanza un rayo por la rejilla. `skip` permite ignorar ciertos materiales
    /// (por ejemplo, el agua cuando el rayo ya viaja dentro de ella).
    pub fn traverse<F>(&self, origin: Vec3, dir: Vec3, max_dist: f32, skip: F) -> Option<VoxelHit>
    where
        F: Fn(u8) -> bool,
    {
        let (t_enter, t_exit, entry_normal) = self.bounds_hit(origin, dir)?;
        let mut t = t_enter.max(0.0);
        if t > max_dist {
            return None;
        }

        let start = origin + dir * (t + EPSILON) - self.origin;
        let mut cell = [
            (start.x.floor() as i32).clamp(0, self.size[0] - 1),
            (start.y.floor() as i32).clamp(0, self.size[1] - 1),
            (start.z.floor() as i32).clamp(0, self.size[2] - 1),
        ];

        let mut step = [0i32; 3];
        let mut t_max = [f32::INFINITY; 3];
        let mut t_delta = [f32::INFINITY; 3];

        for i in 0..3 {
            let d = axis(dir, i);
            if d.abs() < 1e-9 {
                continue;
            }
            step[i] = if d > 0.0 { 1 } else { -1 };
            let boundary =
                axis(self.origin, i) + (cell[i] + if d > 0.0 { 1 } else { 0 }) as f32;
            t_max[i] = (boundary - axis(origin, i)) / d;
            t_delta[i] = 1.0 / d.abs();
        }

        let mut normal = entry_normal;
        let limit = max_dist.min(t_exit);

        loop {
            if !self.in_bounds(cell[0], cell[1], cell[2]) {
                return None;
            }
            let id = self.get(cell[0], cell[1], cell[2]);
            if id != 0 && !skip(id) {
                return Some(VoxelHit {
                    distance: t,
                    point: origin + dir * t,
                    normal,
                    id,
                    cell,
                });
            }

            // Avanza al siguiente voxel por el eje cuya frontera está más cerca.
            let a = if t_max[0] < t_max[1] && t_max[0] < t_max[2] {
                0
            } else if t_max[1] < t_max[2] {
                1
            } else {
                2
            };
            t = t_max[a];
            if t > limit {
                return None;
            }
            cell[a] += step[a];
            t_max[a] += t_delta[a];
            normal = Vec3::default();
            let sign = -step[a] as f32;
            match a {
                0 => normal.x = sign,
                1 => normal.y = sign,
                _ => normal.z = sign,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        let mut g = Grid::new([8, 8, 8], Vec3::new(0.0, 0.0, 0.0));
        g.set(4, 4, 4, 1);
        g
    }

    #[test]
    fn hits_block_from_outside() {
        let hit = grid()
            .traverse(Vec3::new(4.5, -3.0, 4.5), Vec3::new(0.0, 1.0, 0.0), 100.0, |_| false)
            .expect("debe golpear el bloque");
        assert_eq!(hit.cell, [4, 4, 4]);
        assert!((hit.distance - 7.0).abs() < 1e-3, "distancia {}", hit.distance);
        assert_eq!(hit.normal, Vec3::new(0.0, -1.0, 0.0));
    }

    #[test]
    fn hits_block_starting_inside_the_grid() {
        let hit = grid()
            .traverse(Vec3::new(4.5, 0.5, 4.5), Vec3::new(0.0, 1.0, 0.0), 100.0, |_| false)
            .expect("debe golpear el bloque desde adentro de la rejilla");
        assert_eq!(hit.cell, [4, 4, 4]);
    }

    #[test]
    fn diagonal_shadow_ray_hits() {
        let hit = grid().traverse(
            Vec3::new(2.5, 2.5, 2.5),
            Vec3::new(1.0, 1.0, 1.0).normalize(),
            100.0,
            |_| false,
        );
        assert!(hit.is_some(), "el rayo diagonal debería golpear (4,4,4)");
    }

    /// Reproduce el caso de un rayo de sombra: desde la cara superior del suelo
    /// hacia un sol a 45 grados, con una torre que debería taparlo.
    #[test]
    fn ground_point_is_shadowed_by_a_tower() {
        let mut g = Grid::new([16, 16, 16], Vec3::new(0.0, 0.0, 0.0));
        for z in 0..16 {
            for x in 0..16 {
                for y in 0..5 {
                    g.set(x, y, z, 1);
                }
            }
        }
        for y in 5..13 {
            g.set(8, y, 8, 1);
        }

        let sun = Vec3::new(-1.0, 1.0, 0.0).normalize();
        let point = Vec3::new(10.5, 5.0, 8.5);
        let origin = point + Vec3::new(0.0, 1.0, 0.0) * 2e-3;

        let hit = g.traverse(origin, sun, 400.0, |_| false);
        assert!(hit.is_some(), "el punto debería quedar en sombra de la torre");
        assert_eq!(hit.unwrap().cell, [8, 6, 8]);
    }

    #[test]
    fn skip_filter_ignores_material() {
        let hit = grid().traverse(Vec3::new(4.5, 0.5, 4.5), Vec3::new(0.0, 1.0, 0.0), 100.0, |id| id == 1);
        assert!(hit.is_none(), "con el filtro activo no debe reportar impacto");
    }
}

#[cfg(test)]
mod scene_tests {
    use super::*;
    use crate::scene;

    /// La rejilla real no empieza en el origen: comprueba el DDA desplazado.
    #[test]
    fn works_with_a_shifted_origin() {
        let mut g = Grid::new([16, 16, 16], Vec3::new(-8.0, -8.0, -8.0));
        g.set(8, 8, 8, 1); // mundo: x 0..1, y 0..1, z 0..1
        let hit = g
            .traverse(Vec3::new(0.5, -5.0, 0.5), Vec3::new(0.0, 1.0, 0.0), 100.0, |_| false)
            .expect("debe golpear el bloque en una rejilla desplazada");
        assert_eq!(hit.cell, [8, 8, 8]);
    }

    /// Cuenta qué porcentaje de la superficie del diorama queda en sombra.
    #[test]
    fn diorama_surface_has_shadows() {
        let grid = scene::build();
        let elev: f32 = 40f32.to_radians();
        let azim: f32 = 110f32.to_radians();
        let sun = Vec3::new(elev.cos() * azim.sin(), elev.sin(), elev.cos() * azim.cos()).normalize();

        let (mut lit, mut shadowed) = (0, 0);
        for z in 0..scene::SIZE_Z {
            for x in 0..scene::SIZE_X {
                // Primer bloque sólido de arriba hacia abajo.
                let Some(y) = (0..scene::SIZE_Y).rev().find(|&y| grid.get(x, y, z) != 0) else {
                    continue;
                };
                let point = grid.origin + Vec3::new(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5);
                let origin = point + Vec3::new(0.0, 1.0, 0.0) * 2e-3;
                match grid.traverse(origin, sun, 400.0, |_| false) {
                    Some(_) => shadowed += 1,
                    None => lit += 1,
                }
            }
        }

        let pct = 100.0 * shadowed as f32 / (lit + shadowed) as f32;
        println!("columnas en sombra: {shadowed} / {} ({pct:.1}%)", lit + shadowed);
        assert!(pct > 5.0, "casi nada queda en sombra ({pct:.1}%), el rayo de sombra no golpea");
    }
}
