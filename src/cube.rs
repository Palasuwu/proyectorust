use crate::material::Material;
use crate::ray_intersect::{Intersect, RayIntersect};
use crate::vec3::Vec3;

const EPSILON: f32 = 1e-4;

/// Cubo alineado a los ejes (AABB).
pub struct Cube {
    pub min: Vec3,
    pub max: Vec3,
    pub material: Material,
}

impl Cube {
    pub fn new(center: Vec3, size: f32, material: Material) -> Self {
        let half = Vec3::new(size, size, size) * 0.5;
        Self { min: center - half, max: center + half, material }
    }

    fn normal_at(&self, point: Vec3) -> Vec3 {
        let center = (self.min + self.max) * 0.5;
        let half = (self.max - self.min) * 0.5;
        let local = point - center;

        // La cara golpeada es el eje donde el punto está más cerca del borde.
        let ax = (local.x / half.x).abs();
        let ay = (local.y / half.y).abs();
        let az = (local.z / half.z).abs();

        if ax >= ay && ax >= az {
            Vec3::new(local.x.signum(), 0.0, 0.0)
        } else if ay >= az {
            Vec3::new(0.0, local.y.signum(), 0.0)
        } else {
            Vec3::new(0.0, 0.0, local.z.signum())
        }
    }
}

impl RayIntersect for Cube {
    // Método de "slabs": intersecta el rayo con los tres pares de planos.
    fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<Intersect> {
        let inv = Vec3::new(1.0 / direction.x, 1.0 / direction.y, 1.0 / direction.z);
        let t1 = (self.min - origin) * inv;
        let t2 = (self.max - origin) * inv;

        let t_near = t1.x.min(t2.x).max(t1.y.min(t2.y)).max(t1.z.min(t2.z));
        let t_far = t1.x.max(t2.x).min(t1.y.max(t2.y)).min(t1.z.max(t2.z));

        if t_far < EPSILON || t_near > t_far {
            return None;
        }

        // Si el origen está dentro del cubo, la salida es t_far.
        let distance = if t_near > EPSILON { t_near } else { t_far };
        let point = origin + direction * distance;

        Some(Intersect {
            distance,
            point,
            normal: self.normal_at(point),
            material: self.material,
        })
    }
}
