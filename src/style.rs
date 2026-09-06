use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke};
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub panel_hover: Color32,
    pub panel_active: Color32,
    pub accent: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    /// Editor background as a hex string (no '#') to override the CodeEditor theme.
    pub editor_bg: &'static str,
    pub bracket_guide: Color32,
    pub bracket_active: Color32,
    pub bracket_rainbow: [Color32; 4],
    /// Error squiggle / gutter marker.
    pub diag_error: Color32,
    /// Warning squiggle / gutter marker.
    pub diag_warning: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            bg: Color32::BLACK,
            panel: Color32::BLACK,
            panel_hover: Color32::from_rgb(18, 18, 18),
            panel_active: Color32::from_rgb(28, 28, 28),
            accent: Color32::from_rgb(76, 141, 255),
            border: Color32::from_rgb(38, 38, 38),
            text: Color32::from_rgb(220, 220, 222),
            text_muted: Color32::from_rgb(140, 140, 145),
            editor_bg: "000000",
            bracket_guide: Color32::from_rgba_unmultiplied(160, 165, 185, 70),
            bracket_active: Color32::from_rgba_unmultiplied(90, 160, 255, 200),
            bracket_rainbow: [
                Color32::from_rgb(200, 160, 255),
                Color32::from_rgb(90, 200, 240),
                Color32::from_rgb(245, 170, 90),
                Color32::from_rgb(130, 220, 130),
            ],
            diag_error: Color32::from_rgb(255, 84, 84),
            diag_warning: Color32::from_rgb(255, 187, 68),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color32::from_rgb(240, 241, 244),
            panel: Color32::from_rgb(249, 249, 250),
            panel_hover: Color32::from_rgb(235, 236, 240),
            panel_active: Color32::from_rgb(224, 226, 232),
            accent: Color32::from_rgb(56, 120, 255),
            border: Color32::from_rgb(224, 226, 231),
            text: Color32::from_rgb(40, 42, 48),
            text_muted: Color32::from_rgb(112, 115, 124),
            editor_bg: "ffffff",
            bracket_guide: Color32::from_rgba_unmultiplied(70, 75, 95, 80),
            bracket_active: Color32::from_rgba_unmultiplied(56, 120, 255, 205),
            bracket_rainbow: [
                Color32::from_rgb(120, 70, 205),
                Color32::from_rgb(30, 120, 190),
                Color32::from_rgb(200, 130, 55),
                Color32::from_rgb(60, 150, 85),
            ],
            diag_error: Color32::from_rgb(214, 42, 42),
            diag_warning: Color32::from_rgb(186, 122, 34),
        }
    }
}

pub fn frame(fill: Color32, border: Color32, corner: u8, inner: i8) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(corner))
        .inner_margin(Margin::symmetric(inner, inner))
}

pub fn header_label(ui: &egui::Ui, text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase())
        .size(10.5)
        .color(ui.visuals().weak_text_color())
        .strong()
}

pub fn apply_style(ctx: &egui::Context, p: Palette) {
    let visuals = build_visuals(&p);
    ctx.set_visuals(visuals.clone());

    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.visuals = visuals.clone();
    });
}

fn build_visuals(p: &Palette) -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = p.panel;
    visuals.window_fill = p.panel;
    visuals.extreme_bg_color = p.bg;
    visuals.faint_bg_color = p.panel_hover;
    visuals.override_text_color = Some(p.text);
    visuals.selection.bg_fill = Color32::TRANSPARENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(115, 145, 255));
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, p.border);
    visuals.widgets.hovered.weak_bg_fill = p.panel_hover;
    visuals.widgets.hovered.bg_fill = p.panel_hover;
    visuals.widgets.active.weak_bg_fill = p.panel_active;
    visuals.widgets.active.bg_fill = p.panel_active;

    // Flat, sharp buttons: no corner radius anywhere.
    visuals.widgets.noninteractive.corner_radius = 0.0.into();
    visuals.widgets.hovered.corner_radius = 0.0.into();
    visuals.widgets.active.corner_radius = 0.0.into();
    visuals.widgets.open.corner_radius = 0.0.into();
    visuals.widgets.inactive.corner_radius = 0.0.into();

    visuals
}

