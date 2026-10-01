//! Rejilla de voxeles y su recorrido con DDA (Amanatides & Woo).
//!
//! Cada celda mide 1x1x1 en coordenadas de mundo, así que el diorama entero se
//! recorre saltando de cubo en cubo en vez de probar miles de cubos uno por uno.

use crate::vec3::Vec3;

const EPSILON: f32 = 1e-4;
/// Lado (en voxeles) de cada macro-bloque de la rejilla gruesa.
const COARSE: i32 = 8;

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
    /// Rejilla gruesa: marca qué macro-bloques de 8x8x8 tienen algo sólido, para
    /// que los rayos puedan saltarse el aire vacío de un tirón.
    coarse_size: [i32; 3],
    coarse: Vec<bool>,
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
        let up = |n: i32| (n + COARSE - 1) / COARSE;
        let coarse_size = [up(size[0]), up(size[1]), up(size[2])];
        let coarse_total = (coarse_size[0] * coarse_size[1] * coarse_size[2]) as usize;
        Self { size, origin, cells: vec![0; total], coarse_size, coarse: vec![false; coarse_total] }
    }

    fn coarse_index(&self, cx: i32, cy: i32, cz: i32) -> usize {
        ((cy * self.coarse_size[2] + cz) * self.coarse_size[0] + cx) as usize
    }

    /// Recalcula la rejilla gruesa. Se llama una vez terminada la escena.
    pub fn rebuild_coarse(&mut self) {
        self.coarse.iter_mut().for_each(|c| *c = false);
        for y in 0..self.size[1] {
            for z in 0..self.size[2] {
                for x in 0..self.size[0] {
                    if self.cells[self.index(x, y, z)] != 0 {
                        let i = self.coarse_index(
                            x.div_euclid(COARSE),
                            y.div_euclid(COARSE),
                            z.div_euclid(COARSE),
                        );
                        self.coarse[i] = true;
                    }
                }
            }
        }
    }

    /// ¿El macro-bloque que contiene esta celda tiene algo sólido?
    fn coarse_occupied(&self, cell: [i32; 3]) -> bool {
        let (cx, cy, cz) = (
            cell[0].div_euclid(COARSE),
            cell[1].div_euclid(COARSE),
            cell[2].div_euclid(COARSE),
        );
        if cx < 0 || cy < 0 || cz < 0 {
            return true;
        }
        if cx >= self.coarse_size[0] || cy >= self.coarse_size[1] || cz >= self.coarse_size[2] {
            return true;
        }
        self.coarse[self.coarse_index(cx, cy, cz)]
    }

    fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.size[0] && y < self.size[1] && z < self.size[2]
    }

    /// Índice lineal de una celda; lo usa la caché de oclusión ambiental.
    pub fn cell_index(&self, x: i32, y: i32, z: i32) -> usize {
        self.index(x, y, z)
    }

    pub fn cell_count(&self) -> usize {
        (self.size[0] * self.size[1] * self.size[2]) as usize
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
            if id != 0 {
                let c = self.coarse_index(
                    x.div_euclid(COARSE),
                    y.div_euclid(COARSE),
                    z.div_euclid(COARSE),
                );
                self.coarse[c] = true;
            }
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
    ///
    /// Avanza celda por celda (DDA), pero cuando entra en un macro-bloque vacío
    /// salta directo a su salida en vez de recorrer sus 8 celdas.
    pub fn traverse<F>(&self, origin: Vec3, dir: Vec3, max_dist: f32, skip: F) -> Option<VoxelHit>
    where
        F: Fn(u8) -> bool,
    {
        let (t_enter, t_exit, entry_normal) = self.bounds_hit(origin, dir)?;
        let mut t = t_enter.max(0.0);
        if t > max_dist {
            return None;
        }
        let limit = max_dist.min(t_exit);

        let mut step = [0i32; 3];
        let mut t_delta = [f32::INFINITY; 3];
        for i in 0..3 {
            let d = axis(dir, i);
            if d.abs() < 1e-9 {
                continue;
            }
            step[i] = if d > 0.0 { 1 } else { -1 };
            t_delta[i] = 1.0 / d.abs();
        }

        // Estado del DDA en el punto `t`: celda actual y distancia a cada frontera.
        let cell_at = |t: f32| {
            let p = origin + dir * (t + EPSILON) - self.origin;
            [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
        };
        let boundaries = |cell: [i32; 3]| {
            let mut t_max = [f32::INFINITY; 3];
            for i in 0..3 {
                if step[i] == 0 {
                    continue;
                }
                let edge = axis(self.origin, i) + (cell[i] + (step[i] > 0) as i32) as f32;
                t_max[i] = (edge - axis(origin, i)) / axis(dir, i);
            }
            t_max
        };

        let mut cell = cell_at(t);
        for i in 0..3 {
            cell[i] = cell[i].clamp(0, self.size[i] - 1);
        }
        let mut t_max = boundaries(cell);
        let mut normal = entry_normal;
        let mut check_coarse = true;

        loop {
            if !self.in_bounds(cell[0], cell[1], cell[2]) {
                return None;
            }

            let id = self.get(cell[0], cell[1], cell[2]);
            if id != 0 && !skip(id) {
                return Some(VoxelHit { distance: t, point: origin + dir * t, normal, id, cell });
            }

            // Macro-bloque vacío: salta hasta su salida. Sólo hace falta
            // consultarlo al entrar a un macro-bloque nuevo.
            if check_coarse && !self.coarse_occupied(cell) {
                let mut t_jump = f32::INFINITY;
                let mut jump_axis = 0usize;
                for i in 0..3 {
                    if step[i] == 0 {
                        continue;
                    }
                    let block = cell[i].div_euclid(COARSE);
                    let edge_cell = (block + (step[i] > 0) as i32) * COARSE;
                    let edge = axis(self.origin, i) + edge_cell as f32;
                    let t_axis = (edge - axis(origin, i)) / axis(dir, i);
                    if t_axis < t_jump {
                        t_jump = t_axis;
                        jump_axis = i;
                    }
                }

                if t_jump > t + EPSILON {
                    if t_jump > limit {
                        return None;
                    }
                    t = t_jump;
                    cell = cell_at(t);
                    t_max = boundaries(cell);
                    check_coarse = true;
                    normal = Vec3::default();
                    let sign = -step[jump_axis] as f32;
                    match jump_axis {
                        0 => normal.x = sign,
                        1 => normal.y = sign,
                        _ => normal.z = sign,
                    }
                    continue;
                }
            }

            // Paso normal: avanza al voxel vecino por el eje más cercano.
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
            // Sólo se cambió de macro-bloque si se cruzó un múltiplo de COARSE.
            check_coarse = (cell[a] & (COARSE - 1)) == if step[a] > 0 { 0 } else { COARSE - 1 };
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
