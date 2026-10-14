use crate::config::Settings;
use eframe::egui::{self, Color32, FontId, TextStyle};
pub fn dark(ctx: &egui::Context, s: &Settings) -> bool {
    match s.theme.as_str() {
        "Light" => false,
        "Dark" => true,
        _ => ctx.style().visuals.dark_mode,
    }
}
pub fn apply(ctx: &egui::Context, s: &Settings) {
    ctx.set_theme(match s.theme.as_str() {
        "Light" => egui::ThemePreference::Light,
        "System" => egui::ThemePreference::System,
        _ => egui::ThemePreference::Dark,
    });
    let is_dark = dark(ctx, s);
    let mut style = (*ctx.style()).clone();
    let accent = Color32::from_rgb(s.accent[0], s.accent[1], s.accent[2]);
    style.visuals = if is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    style.visuals.override_text_color = Some(if is_dark {
        Color32::from_rgb(237, 235, 248)
    } else {
        Color32::from_rgb(32, 29, 51)
    });
    style.visuals.panel_fill = if is_dark {
        Color32::from_rgb(19, 20, 29)
    } else {
        Color32::from_rgb(247, 246, 252)
    };
    style.visuals.window_fill = if is_dark {
        Color32::from_rgb(26, 26, 38)
    } else {
        Color32::from_rgb(250, 249, 255)
    };
    style.visuals.extreme_bg_color = if is_dark {
        Color32::from_rgb(24, 23, 36)
    } else {
        Color32::from_rgb(255, 254, 255)
    };
    style.visuals.faint_bg_color = if is_dark {
        Color32::from_rgb(32, 31, 45)
    } else {
        Color32::from_rgb(236, 233, 250)
    };
    style.visuals.selection.bg_fill = if is_dark {
        Color32::from_rgb(s.accent[0] / 2, s.accent[1] / 2, s.accent[2] / 2)
    } else {
        Color32::from_rgb(
            (s.accent[0] as f32 * 0.4 + 255.0 * 0.6) as u8,
            (s.accent[1] as f32 * 0.4 + 255.0 * 0.6) as u8,
            (s.accent[2] as f32 * 0.4 + 255.0 * 0.6) as u8,
        )
    };
    style.visuals.selection.stroke = egui::Stroke::new(1.0, accent);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(
        1.0,
        accent.gamma_multiply(if is_dark { 0.55 } else { 0.65 }),
    );
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, accent);
    style.visuals.widgets.inactive.bg_fill = style.visuals.faint_bg_color;
    style.visuals.widgets.inactive.weak_bg_fill = style.visuals.faint_bg_color;
    style.visuals.widgets.hovered.bg_fill = if is_dark {
        Color32::from_rgb(s.accent[0] / 2, s.accent[1] / 2, s.accent[2] / 2)
    } else {
        Color32::from_rgb(
            (s.accent[0] as f32 * 0.23 + 255.0 * 0.77) as u8,
            (s.accent[1] as f32 * 0.23 + 255.0 * 0.77) as u8,
            (s.accent[2] as f32 * 0.23 + 255.0 * 0.77) as u8,
        )
    };
    style.visuals.widgets.hovered.weak_bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.gamma_multiply(0.7));
    style.visuals.widgets.active.bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.widgets.active.weak_bg_fill = style.visuals.widgets.hovered.bg_fill;
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(9);
    }
    style.visuals.widgets.open.bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.widgets.open.weak_bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.window_corner_radius = egui::CornerRadius::same(16);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(9);
    if is_dark {
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(55, 48, 78);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(68, 57, 95);
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(2.0, accent);
        style.visuals.slider_trailing_fill = true;
    }
    style.visuals.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(40.0, 34.0);
    style.spacing.scroll.bar_width = 7.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.foreground_color = true;
    style.visuals.widgets.inactive.fg_stroke.color = accent;
    style.visuals.widgets.hovered.fg_stroke.color = accent;
    style.visuals.widgets.active.fg_stroke.color = accent;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(25.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    ctx.set_style(style);
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if s.theme == "System" {
        egui::SystemTheme::SystemDefault
    } else if is_dark {
        egui::SystemTheme::Dark
    } else {
        egui::SystemTheme::Light
    }));
}
pub fn colors(s: &Settings, dark: bool) -> (Color32, Color32) {
    let tone = s.popup_tone;
    let base = if dark {
        [24.0, 17.0, 32.0]
    } else {
        [248.0, 246.0, 253.0]
    };
    let strength = s.popup_gradient.clamp(0.0, 1.0);
    let blend = if dark {
        0.25 + strength * 0.35
    } else {
        0.06 + strength * 0.18
    };
    let top = std::array::from_fn::<u8, 3, _>(|i| {
        (base[i] * (1.0 - blend) + tone[i] as f32 * blend) as u8
    });
    let bottom = std::array::from_fn::<u8, 3, _>(|i| {
        (base[i] * (1.0 - blend * 0.2) + tone[i] as f32 * blend * 0.2) as u8
    });
    let alpha = (255.0 * s.popup_opacity.clamp(0.1, 1.0)) as u8;
    (
        Color32::from_rgba_unmultiplied(top[0], top[1], top[2], alpha),
        Color32::from_rgba_unmultiplied(bottom[0], bottom[1], bottom[2], alpha),
    )
}
pub fn background(ctx: &egui::Context, s: &Settings, quick: bool) {
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let (top, bottom) = if quick {
        colors(s, dark(ctx, s))
    } else {
        let c = ctx.style().visuals.panel_fill;
        (c, c)
    };
    // A rounded triangle fan supplies one alpha surface, rather than stacking opaque panels.
    let center = rect.center();
    let mut perimeter = vec![];
    // Popup content follows the native Windows acrylic corner radius.
    let radius = if quick { 8.0f32 } else { 16.0f32 }.min(rect.height() * 0.5);
    for (corner, start) in [
        (
            rect.right_top() + egui::vec2(-radius, radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (rect.right_bottom() + egui::vec2(-radius, -radius), 0.0),
        (
            rect.left_bottom() + egui::vec2(radius, -radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            rect.left_top() + egui::vec2(radius, radius),
            std::f32::consts::PI,
        ),
    ] {
        for n in 0..=10 {
            let a = start + n as f32 / 10.0 * std::f32::consts::FRAC_PI_2;
            perimeter.push(corner + egui::vec2(a.cos(), a.sin()) * radius);
        }
    }
    let mix = |y: f32| {
        let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let a = top.to_array();
        let b = bottom.to_array();
        Color32::from_rgba_premultiplied(
            (a[0] as f32 * (1.0 - t) + b[0] as f32 * t) as u8,
            (a[1] as f32 * (1.0 - t) + b[1] as f32 * t) as u8,
            (a[2] as f32 * (1.0 - t) + b[2] as f32 * t) as u8,
            (a[3] as f32 * (1.0 - t) + b[3] as f32 * t) as u8,
        )
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, mix(center.y));
    for p in &perimeter {
        mesh.colored_vertex(*p, mix(p.y));
    }
    for i in 0..perimeter.len() {
        mesh.add_triangle(0, i as u32 + 1, ((i + 1) % perimeter.len()) as u32 + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(
        rect.shrink(0.5),
        radius,
        egui::Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(
                s.popup_tone[0],
                s.popup_tone[1],
                s.popup_tone[2],
                if quick { 42 } else { 30 },
            ),
        ),
        egui::StrokeKind::Inside,
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_change_material() {
        let mut s = Settings::default();
        let a = colors(&s, true);
        s.popup_opacity = 0.2;
        let b = colors(&s, true);
        assert!(b.0.a() < a.0.a());
        s.popup_gradient = 1.0;
        let c = colors(&s, true);
        assert_ne!(b.0, c.0);
        s.popup_tone = [255, 0, 0];
        assert_ne!(colors(&s, true).0, c.0);
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;
    #[test]
    fn saved_dark_theme_overrides_light_system_first_frame() {
        let ctx = egui::Context::default();
        let mut s = Settings::default();
        s.theme = "Dark".into();
        apply(&ctx, &s);
        let input = egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| assert!(ctx.style().visuals.dark_mode));
    }
}

pub fn component_color(dark: bool, loaded: bool, installed: bool) -> Color32 {
    match (dark, loaded, installed) {
        (true, true, _) => Color32::GREEN,
        (false, true, _) => Color32::from_rgb(24, 112, 58),
        (true, false, true) => Color32::YELLOW,
        (false, false, true) => Color32::from_rgb(124, 83, 0),
        (true, false, false) => Color32::LIGHT_RED,
        (false, false, false) => Color32::from_rgb(176, 34, 51),
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    fn luminance(color: Color32) -> f64 {
        let values = [color.r(), color.g(), color.b()].map(|v| {
            let c = v as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722
    }
    #[test]
    fn model_status_text_has_readable_contrast_in_both_themes() {
        for dark in [true, false] {
            let background = luminance(if dark {
                Color32::from_rgb(19, 20, 29)
            } else {
                Color32::from_rgb(247, 246, 252)
            });
            for (loaded, installed) in [(true, true), (false, true), (false, false)] {
                let text = luminance(component_color(dark, loaded, installed));
                assert!((text.max(background) + 0.05) / (text.min(background) + 0.05) >= 4.5);
            }
        }
    }
}
