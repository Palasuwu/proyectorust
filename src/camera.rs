use crate::vec3::Vec3;

const MIN_DISTANCE: f32 = 1.5;
const MAX_PITCH: f32 = 1.5; // ~86°, evita que la cámara se voltee

/// Cámara orbital: gira alrededor de `center` y se acerca/aleja con `distance`.
pub struct Camera {
    pub center: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Camera {
    pub fn new(center: Vec3, yaw: f32, pitch: f32, distance: f32) -> Self {
        let mut camera = Self { center, yaw: 0.0, pitch: 0.0, distance: MIN_DISTANCE };
        camera.orbit(yaw, pitch);
        camera.zoom(distance - MIN_DISTANCE);
        camera
    }

    pub fn eye(&self) -> Vec3 {
        let horizontal = self.distance * self.pitch.cos();
        self.center
            + Vec3::new(
                horizontal * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                horizontal * self.yaw.cos(),
            )
    }

    /// Rota la cámara alrededor del centro (ángulos en radianes).
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Positivo aleja la cámara, negativo la acerca.
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance + delta).max(MIN_DISTANCE);
    }

    /// Convierte una dirección en espacio de cámara (mirando a -Z) a espacio mundo.
    pub fn basis_change(&self, v: Vec3) -> Vec3 {
        let forward = (self.center - self.eye()).normalize();
        let right = forward.cross(Vec3::new(0.0, 1.0, 0.0)).normalize();
        let up = right.cross(forward);

        (right * v.x + up * v.y - forward * v.z).normalize()
    }
}
