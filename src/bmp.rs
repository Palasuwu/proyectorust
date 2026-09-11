use std::fs;
use std::io;
use std::path::Path;

use crate::vec3::Vec3;

const HEADER_SIZE: u32 = 54;

/// Guarda un framebuffer (colores en 0..1, fila 0 arriba) como BMP de 24 bits.
pub fn write_bmp(path: &Path, width: usize, height: usize, pixels: &[Vec3]) -> io::Result<()> {
    let row_size = (width * 3 + 3) & !3; // cada fila se rellena a múltiplos de 4
    let padding = row_size - width * 3;
    let data_size = (row_size * height) as u32;
    let file_size = HEADER_SIZE + data_size;

    let mut buf = Vec::with_capacity(file_size as usize);

    // Encabezado de archivo
    buf.extend_from_slice(b"BM");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&HEADER_SIZE.to_le_bytes());

    // Encabezado DIB (BITMAPINFOHEADER)
    buf.extend_from_slice(&40u32.to_le_bytes());
    buf.extend_from_slice(&(width as i32).to_le_bytes());
    buf.extend_from_slice(&(height as i32).to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // planos
    buf.extend_from_slice(&24u16.to_le_bytes()); // bits por pixel
    buf.extend_from_slice(&0u32.to_le_bytes()); // sin compresión
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.extend_from_slice(&2835i32.to_le_bytes()); // 72 DPI
    buf.extend_from_slice(&2835i32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    // BMP guarda las filas de abajo hacia arriba, en orden BGR.
    for y in (0..height).rev() {
        for c in &pixels[y * width..(y + 1) * width] {
            buf.extend_from_slice(&[to_byte(c.z), to_byte(c.y), to_byte(c.x)]);
        }
        buf.extend(std::iter::repeat_n(0u8, padding));
    }

    fs::write(path, buf)
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}
