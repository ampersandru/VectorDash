use crate::config::Settings;
use eframe::egui::{self, Color32, FontId, TextStyle};
/// Matches the reference glass-card radius in logical pixels.
pub const WINDOW_RADIUS: f32 = 28.0;
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
    // Glass uses the compositor/aurora underneath; opaque controls would flatten it.
    style.visuals.panel_fill = if is_dark {
        Color32::from_rgb(15, 18, 32)
    } else {
        Color32::from_rgb(241, 243, 251)
    };
    style.visuals.window_fill = if is_dark {
        Color32::from_rgb(25, 29, 46)
    } else {
        Color32::from_rgb(248, 250, 255)
    };
    style.visuals.extreme_bg_color = if is_dark {
        Color32::from_rgba_unmultiplied(5, 9, 21, 170)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 195)
    };
    style.visuals.faint_bg_color = if is_dark {
        Color32::from_white_alpha(13)
    } else {
        Color32::from_white_alpha(145)
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
        if is_dark {
            Color32::from_white_alpha(42)
        } else {
            Color32::from_rgba_unmultiplied(62, 69, 102, 50)
        },
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
        widget.corner_radius = egui::CornerRadius::same(14);
    }
    style.visuals.widgets.open.bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.widgets.open.weak_bg_fill = style.visuals.widgets.hovered.bg_fill;
    style.visuals.window_corner_radius = egui::CornerRadius::same(24);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_black_alpha(if is_dark { 90 } else { 30 }),
    };
    style.visuals.window_stroke = egui::Stroke::new(
        1.0,
        if is_dark {
            Color32::from_white_alpha(42)
        } else {
            Color32::from_white_alpha(220)
        },
    );
    style.visuals.menu_corner_radius = egui::CornerRadius::same(14);
    if is_dark {
        style.visuals.widgets.inactive.bg_fill = Color32::from_white_alpha(16);
        style.visuals.widgets.active.bg_fill = accent.gamma_multiply(0.32);
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
fn surface(s: &Settings, dark: bool, _quick: bool, p: egui::Pos2, rect: egui::Rect) -> Color32 {
    let (top, bottom) = colors(s, dark);
    let t = ((p.y - rect.top()) / rect.height().max(1.0)).clamp(0.0, 1.0);
    let a = top.to_srgba_unmultiplied();
    let b = bottom.to_srgba_unmultiplied();
    let x = (p.x - rect.left()) / rect.width().max(1.0);
    let glow = |cx: f32, cy: f32| (-((x - cx).powi(2) * 4.0 + (t - cy).powi(2) * 3.0)).exp();
    let intensity = s.popup_gradient.clamp(0.0, 1.0);
    let accent = [s.accent[0], s.accent[1], s.accent[2]];
    let illumination =
        glow(0.12, 0.08) * (0.08 + intensity * 0.22) + glow(0.95, 0.85) * (0.04 + intensity * 0.14);
    let mut rgb = [0u8; 3];
    for i in 0..3 {
        let base = a[i] as f32 * (1.0 - t) + b[i] as f32 * t;
        rgb[i] =
            (base * (1.0 - illumination) + accent[i] as f32 * illumination).clamp(0.0, 255.0) as u8;
    }
    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], a[3])
}
pub fn background(ctx: &egui::Context, s: &Settings, quick: bool) {
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    // Paint and the native window region use the same radius, including DPI scaling.
    let radius = if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        0.0
    } else {
        WINDOW_RADIUS.min(rect.height() * 0.5)
    };
    let center = rect.center();
    let mut perimeter = vec![];
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
        for n in 0..=12 {
            let a = start + n as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
            perimeter.push(corner + egui::vec2(a.cos(), a.sin()) * radius);
        }
    }
    // Concentric rings give soft spatial lighting on one alpha surface. Nothing is
    // painted in the outer corners, avoiding a rectangular tint over acrylic.
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, surface(s, dark(ctx, s), quick, center, rect));
    for ring in 1..=8 {
        for p in &perimeter {
            let point = center + (*p - center) * (ring as f32 / 8.0);
            mesh.colored_vertex(point, surface(s, dark(ctx, s), quick, point, rect));
        }
    }
    let count = perimeter.len() as u32;
    for i in 0..count {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % count);
    }
    for ring in 0..7 {
        let inner = 1 + ring * count;
        let outer = inner + count;
        for i in 0..count {
            let next = (i + 1) % count;
            mesh.add_triangle(inner + i, outer + i, outer + next);
            mesh.add_triangle(inner + i, outer + next, inner + next);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(
        rect.shrink(0.5),
        radius,
        egui::Stroke::new(
            1.0,
            if dark(ctx, s) {
                Color32::from_white_alpha(46)
            } else {
                Color32::from_white_alpha(220)
            },
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

#[cfg(test)]
mod glass_tests {
    use super::*;
    #[test]
    fn frosted_surfaces_preserve_theme_opacity_and_rounded_cards() {
        let ctx = egui::Context::default();
        for dark in [true, false] {
            let s = Settings {
                theme: if dark { "Dark".into() } else { "Light".into() },
                ..Default::default()
            };
            apply(&ctx, &s);
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0));
            let main = surface(&s, dark, false, rect.center(), rect);
            assert_eq!(main, surface(&s, dark, true, rect.center(), rect));
            assert_eq!(main.a(), (255.0 * s.popup_opacity) as u8);
            let mut transparent = s.clone();
            transparent.popup_opacity = 0.2;
            assert!(
                surface(&transparent, dark, true, rect.center(), rect).a()
                    < surface(&s, dark, true, rect.center(), rect).a()
            );
            assert!(surface(&transparent, dark, false, rect.center(), rect).a() < main.a());
            let mut gradient = s.clone();
            gradient.popup_gradient = 1.0;
            assert_ne!(
                surface(&gradient, dark, false, rect.left_top(), rect),
                surface(&s, dark, false, rect.left_top(), rect)
            );
            assert_eq!(WINDOW_RADIUS, 28.0);
            assert!(ctx.style().visuals.extreme_bg_color.a() < 255);
            assert_eq!(
                ctx.style().visuals.window_corner_radius,
                egui::CornerRadius::same(24)
            );
            assert!(ctx.style().visuals.window_shadow.blur > 0);
        }
    }
}
