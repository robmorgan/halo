use eframe::egui::Color32;

pub struct Theme {
    pub bg_color: Color32,
    pub panel_bg: Color32,
    pub element_bg: Color32,
    pub text_color: Color32,
    pub text_dim: Color32,
    pub border_color: Color32,
    pub highlight_color: Color32,
    pub active_color: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_color: Color32::from_rgb(0, 0, 0),
            panel_bg: Color32::from_rgb(16, 16, 16),
            element_bg: Color32::from_rgb(32, 32, 32),
            text_color: Color32::from_rgb(255, 255, 255),
            text_dim: Color32::from_rgb(156, 163, 175),
            border_color: Color32::from_rgb(55, 65, 81),
            highlight_color: Color32::from_rgb(59, 130, 246),
            active_color: Color32::from_rgb(30, 64, 175),
        }
    }
}
