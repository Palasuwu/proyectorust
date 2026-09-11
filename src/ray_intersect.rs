use crate::material::Material;
use crate::vec3::Vec3;

pub struct Intersect {
    pub distance: f32,
    pub point: Vec3,
    pub normal: Vec3,
    pub material: Material,
}

pub trait RayIntersect: Sync {
    fn ray_intersect(&self, origin: Vec3, direction: Vec3) -> Option<Intersect>;
}
