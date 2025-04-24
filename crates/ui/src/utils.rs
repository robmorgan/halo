use eframe::egui::Color32;

pub struct Theme {
    pub bg_color: Color32,
    pub _panel_bg: Color32,
    pub _element_bg: Color32,
    pub _text_color: Color32,
    pub text_dim: Color32,
    pub _border_color: Color32,
    pub _highlight_color: Color32,
    pub _active_color: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_color: Color32::from_rgb(0, 0, 0),
            _panel_bg: Color32::from_rgb(16, 16, 16),
            _element_bg: Color32::from_rgb(32, 32, 32),
            _text_color: Color32::from_rgb(255, 255, 255),
            text_dim: Color32::from_rgb(156, 163, 175),
            _border_color: Color32::from_rgb(55, 65, 81),
            _highlight_color: Color32::from_rgb(59, 130, 246),
            _active_color: Color32::from_rgb(30, 64, 175),
        }
    }
}
