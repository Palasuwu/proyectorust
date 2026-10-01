//! Tipos de bloque y sus parámetros de material.

use crate::vec3::Vec3;

/// Cada variante es un tipo de bloque distinto (id 0 = aire).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Kind {
    Air = 0,
    Grass,
    Dirt,
    Stone,
    Sand,
    Plank,
    Log,
    LeafGreen,
    LeafPink,
    LeafOrange,
    Water,
    Glass,
    Lantern,
    Roof,
    Flower,
    /// Viga oscura de entramado.
    Beam,
}

impl Kind {
    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Kind {
        use Kind::*;
        const TABLE: [Kind; 16] = [
            Air, Grass, Dirt, Stone, Sand, Plank, Log, LeafGreen, LeafPink, LeafOrange, Water,
            Glass, Lantern, Roof, Flower, Beam,
        ];
        TABLE[id as usize]
    }
}

/// Parámetros de sombreado de un material.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Color base sobre el que trabaja la textura procedural.
    pub base: Vec3,
    /// Pesos de iluminación: [difusa, especular].
    pub albedo: [f32; 2],
    /// Exponente especular (Phong).
    pub specular: f32,
    /// Cuánta luz refleja como espejo (0..1, modulado por Fresnel).
    pub reflectivity: f32,
    /// Cuánta luz transmite (0..1).
    pub transparency: f32,
    /// Índice de refracción (1.0 = sin desviación).
    pub ior: f32,
    /// Luz propia del bloque.
    pub emissive: Vec3,
}

const fn mat(
    base: Vec3,
    albedo: [f32; 2],
    specular: f32,
    reflectivity: f32,
    transparency: f32,
    ior: f32,
) -> Material {
    Material { base, albedo, specular, reflectivity, transparency, ior, emissive: Vec3::new(0.0, 0.0, 0.0) }
}

/// Tabla de materiales indexada por el id del bloque.
pub const MATERIALS: [Material; 16] = [
    // Air (nunca se sombrea)
    mat(Vec3::new(0.0, 0.0, 0.0), [0.0, 0.0], 1.0, 0.0, 0.0, 1.0),
    // Grass
    mat(Vec3::new(0.26, 0.47, 0.19), [0.95, 0.05], 8.0, 0.0, 0.0, 1.0),
    // Dirt
    mat(Vec3::new(0.42, 0.29, 0.18), [0.95, 0.03], 6.0, 0.0, 0.0, 1.0),
    // Stone
    mat(Vec3::new(0.56, 0.57, 0.55), [0.85, 0.25], 40.0, 0.08, 0.0, 1.0),
    // Sand
    mat(Vec3::new(0.80, 0.73, 0.52), [0.95, 0.10], 12.0, 0.0, 0.0, 1.0),
    // Plank
    mat(Vec3::new(0.52, 0.34, 0.22), [0.90, 0.18], 24.0, 0.03, 0.0, 1.0),
    // Log
    mat(Vec3::new(0.38, 0.26, 0.17), [0.92, 0.10], 14.0, 0.0, 0.0, 1.0),
    // LeafGreen
    mat(Vec3::new(0.18, 0.40, 0.16), [0.95, 0.08], 10.0, 0.0, 0.0, 1.0),
    // LeafPink
    mat(Vec3::new(0.78, 0.47, 0.62), [0.95, 0.08], 10.0, 0.0, 0.0, 1.0),
    // LeafOrange
    mat(Vec3::new(0.82, 0.50, 0.16), [0.95, 0.08], 10.0, 0.0, 0.0, 1.0),
    // Water: refracción + reflejo especular
    mat(Vec3::new(0.10, 0.34, 0.42), [0.25, 0.9], 220.0, 1.0, 0.92, 1.333),
    // Glass: ventana reflectante y transparente
    mat(Vec3::new(0.70, 0.84, 0.90), [0.15, 0.8], 160.0, 0.80, 0.72, 1.52),
    // Lantern (emisivo, se ajusta abajo)
    mat(Vec3::new(1.0, 0.85, 0.55), [0.6, 0.4], 60.0, 0.0, 0.25, 1.0),
    // Roof (teja de barro)
    mat(Vec3::new(0.64, 0.27, 0.22), [0.9, 0.15], 20.0, 0.04, 0.0, 1.0),
    // Flower
    mat(Vec3::new(0.88, 0.42, 0.52), [0.95, 0.10], 10.0, 0.0, 0.0, 1.0),
    // Beam: madera oscura del entramado
    mat(Vec3::new(0.26, 0.16, 0.11), [0.88, 0.14], 28.0, 0.03, 0.0, 1.0),
];

pub fn material_of(id: u8) -> Material {
    let mut material = MATERIALS[id as usize];
    if Kind::from_id(id) == Kind::Lantern {
        material.emissive = Vec3::new(1.0, 0.72, 0.36) * 3.0;
    }
    material
}
