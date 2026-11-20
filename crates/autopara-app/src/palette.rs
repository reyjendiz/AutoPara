use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub canvas: Color32,
    pub surface: Color32,
    pub sunken: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub accent: Color32,
    pub accent_fill: Color32,
    pub accent_soft: Color32,
    pub accent_text: Color32,
    pub today_bg: Color32,
    pub opened_bg: Color32,
    pub missed_bg: Color32,
    pub nolink_bg: Color32,
    pub skipped_bg: Color32,
    pub chip_bg: Color32,
    pub success_bg: Color32,
    pub success_fg: Color32,
    pub danger: Color32,
    pub danger_bg: Color32,
    pub danger_fg: Color32,
    pub warn_bg: Color32,
    pub warn_fg: Color32,
    pub zoom_bg: Color32,
    pub zoom_fg: Color32,
    pub meet_bg: Color32,
    pub meet_fg: Color32,
    pub teams_bg: Color32,
    pub teams_fg: Color32,
    pub discord_bg: Color32,
    pub discord_fg: Color32,
    pub drop_bg: Color32,
    pub drop_border: Color32,
    pub drop_hover_bg: Color32,
    pub banner_bg: Color32,
    pub banner_border: Color32,
    pub banner_text: Color32,
    pub rail_bg: Color32,
    pub rail_icon: Color32,
    pub rail_hover: Color32,
    pub rail_primary_bg: Color32,
    pub rail_primary_fg: Color32,
    pub tint_alpha: f32,
}

pub const fn hex(v: u32) -> Color32 {
    Color32::from_rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    )
}

pub fn parse_hex(s: &str) -> Color32 {
    let clean = s.trim_start_matches('#');
    u32::from_str_radix(clean, 16)
        .map(hex)
        .unwrap_or(hex(0x86cdf7))
}

pub fn blend(base: Color32, tint: Color32, alpha: f32) -> Color32 {
    let inv = 1.0 - alpha;
    let r = (f32::from(base.r()) * inv + f32::from(tint.r()) * alpha).round() as u8;
    let g = (f32::from(base.g()) * inv + f32::from(tint.g()) * alpha).round() as u8;
    let b = (f32::from(base.b()) * inv + f32::from(tint.b()) * alpha).round() as u8;
    Color32::from_rgb(r, g, b)
}

/// Dedicated dark palette for AutoPara (user-requested dark-only theme).
pub fn dark_palette() -> Palette {
    Palette {
        canvas: hex(0x0a0a0b),
        surface: hex(0x151517),
        sunken: hex(0x1d1d20),
        text: hex(0xf2f2f4),
        text_muted: hex(0xa0a0aa),
        text_faint: hex(0x8a8a94),
        border: hex(0x2a2a2e),
        border_soft: hex(0x232326),
        accent: hex(0xf2f2f4),
        accent_fill: hex(0xf2f2f4),
        accent_soft: hex(0x26262b),
        accent_text: hex(0x0e0e10),
        today_bg: hex(0x19191c),
        opened_bg: hex(0x18181a),
        missed_bg: hex(0x241a1a),
        nolink_bg: hex(0x22200f),
        skipped_bg: hex(0x18181a),
        chip_bg: hex(0x26262b),
        success_bg: hex(0x17301f),
        success_fg: hex(0x7edc9e),
        danger: hex(0xf0665c),
        danger_bg: hex(0x33191a),
        danger_fg: hex(0xff8a84),
        warn_bg: hex(0x33290c),
        warn_fg: hex(0xf1d86b),
        zoom_bg: hex(0x142b40),
        zoom_fg: hex(0x8cc8f5),
        meet_bg: hex(0x17301f),
        meet_fg: hex(0x7edc9e),
        teams_bg: hex(0x232040),
        teams_fg: hex(0xb4a9f5),
        discord_bg: hex(0x202344),
        discord_fg: hex(0x99a5ff),
        drop_bg: hex(0x1d1d20),
        drop_border: hex(0x3a3a40),
        drop_hover_bg: hex(0x26262b),
        banner_bg: hex(0x33290c),
        banner_border: hex(0x5c4a12),
        banner_text: hex(0xf1d86b),
        rail_bg: hex(0x1d1d20),
        rail_icon: hex(0xf2f2f4),
        rail_hover: hex(0x2e2e33),
        rail_primary_bg: hex(0xf2f2f4),
        rail_primary_fg: hex(0x0e0e10),
        tint_alpha: 0.20,
    }
}

pub fn apply_visuals(ctx: &egui::Context, p: Palette) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = p.canvas;
    visuals.window_fill = p.surface;
    visuals.window_stroke = Stroke::new(1.0_f32, p.border);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.extreme_bg_color = p.surface;
    visuals.faint_bg_color = p.sunken;
    visuals.selection.bg_fill = hex(0x32323a);
    visuals.selection.stroke = Stroke::new(1.0_f32, p.text);

    visuals.widgets.noninteractive.bg_fill = p.surface;
    visuals.widgets.noninteractive.weak_bg_fill = p.surface;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = p.surface;
    visuals.widgets.inactive.weak_bg_fill = p.surface;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(10);

    visuals.widgets.hovered.bg_fill = p.accent_soft;
    visuals.widgets.hovered.weak_bg_fill = p.accent_soft;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(10);

    visuals.widgets.active.bg_fill = p.accent_soft;
    visuals.widgets.active.weak_bg_fill = p.accent_soft;
    visuals.widgets.active.bg_stroke = Stroke::new(1.2_f32, p.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, p.text);
    visuals.widgets.active.corner_radius = CornerRadius::same(10);

    visuals.widgets.open.bg_fill = p.surface;
    visuals.widgets.open.weak_bg_fill = p.surface;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, p.accent);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, p.text);
    visuals.widgets.open.corner_radius = CornerRadius::same(10);

    let mut style = Style {
        visuals,
        ..(*ctx.style()).clone()
    };
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.icon_width = 16.0;
    style.spacing.icon_spacing = 10.0;
    style.spacing.interact_size.y = 32.0;
    style.interaction.tooltip_delay = 0.22;
    ctx.set_style(style);
}
