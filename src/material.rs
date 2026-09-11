use crate::vec3::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Color base del material (RGB en 0..1).
    pub diffuse: Vec3,
    /// Pesos de la iluminación: [difusa, especular].
    pub albedo: [f32; 2],
    /// Exponente especular (Phong): más alto = brillo más concentrado.
    pub specular: f32,
}

impl Material {
    pub const fn new(diffuse: Vec3, albedo: [f32; 2], specular: f32) -> Self {
        Self { diffuse, albedo, specular }
    }
}
