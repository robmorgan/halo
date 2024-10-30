// Color representation
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 7 || !hex.starts_with('#') {
            return None;
        }

        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;

        Some(Color { r, g, b })
    }

    pub fn lerp(&self, target: &Color, t: f32) -> Self {
        Color {
            r: self.lerp_component(self.r, target.r, t),
            g: self.lerp_component(self.g, target.g, t),
            b: self.lerp_component(self.b, target.b, t),
        }
    }

    pub fn lerp_component(&self, start: u8, end: u8, t: f32) -> u8 {
        let t = t.clamp(0.0, 1.0);
        let start_f = start as f32;
        let end_f = end as f32;
        ((start_f + (end_f - start_f) * t) as u8).clamp(0, 255)
    }
}
