use crate::models::{
    apply_theme_style, ease_out_back, ease_out_cubic, get_or_load_image, semibold_font,
    with_alpha, AttachmentKind, EgrammApp, ThemePalette, AUTH_BLUE, AUTH_BLUE_HOVER, DANGER_RED,
    READ_TICK_BLUE,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Stroke, Vec2};
use std::time::Instant;

pub fn draw_read_ticks(
    p: &egui::Painter,
    rect: Rect,
    is_read: bool,
    muted_color: Color32,
    alpha: f32,
) {
    let base_col = if is_read {
        READ_TICK_BLUE
    } else {
        muted_color
    };
    let stroke = Stroke::new(1.8, with_alpha(base_col, alpha));
    let cy = rect.center().y;

    if is_read {
        let x1 = rect.min.x + 2.0;
        p.line_segment([Pos2::new(x1, cy), Pos2::new(x1 + 3.5, cy + 3.5)], stroke);
        p.line_segment(
            [Pos2::new(x1 + 3.5, cy + 3.5), Pos2::new(x1 + 9.0, cy - 3.5)],
            stroke,
        );

        let x2 = rect.min.x + 6.5;
        p.line_segment([Pos2::new(x2, cy), Pos2::new(x2 + 3.5, cy + 3.5)], stroke);
        p.line_segment(
            [Pos2::new(x2 + 3.5, cy + 3.5), Pos2::new(x2 + 9.0, cy - 3.5)],
            stroke,
        );
    } else {
        let x1 = rect.min.x + 4.0;
        p.line_segment([Pos2::new(x1, cy), Pos2::new(x1 + 3.8, cy + 3.8)], stroke);
        p.line_segment(
            [Pos2::new(x1 + 3.8, cy + 3.8), Pos2::new(x1 + 9.5, cy - 3.8)],
            stroke,
        );
    }
}

pub fn draw_reply_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment(
        [Pos2::new(c.x + 6.0, c.y - 1.0), Pos2::new(c.x - 5.5, c.y - 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 1.5, c.y - 5.2), Pos2::new(c.x - 6.0, c.y - 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 1.5, c.y + 3.2), Pos2::new(c.x - 6.0, c.y - 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 6.0, c.y - 1.0), Pos2::new(c.x + 6.0, c.y + 5.0)],
        s,
    );
}

pub fn draw_reply_quote_bar(
    ui: &mut egui::Ui,
    sender: &str,
    preview: &str,
    pal: &ThemePalette,
    alpha: f32,
) {
    let clean_preview: String = preview.chars().take(72).collect();
    let display_preview = if preview.chars().count() > 72 {
        format!("{}...", clean_preview)
    } else {
        clean_preview
    };

    egui::Frame::none()
        .fill(with_alpha(pal.card_inner, alpha))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (bar_rect, _) =
                    ui.allocate_exact_size(Vec2::new(3.0, 26.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    bar_rect,
                    Rounding::same(1.5),
                    with_alpha(pal.accent_light, alpha),
                );
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(sender)
                            .font(semibold_font(11.5))
                            .color(with_alpha(pal.accent_light, alpha)),
                    );
                    ui.label(
                        egui::RichText::new(display_preview)
                            .size(12.0)
                            .color(with_alpha(pal.text_muted, alpha)),
                    );
                });
            });
        });
}

pub fn draw_reaction_avatar_badge(
    p: &egui::Painter,
    rect: Rect,
    avatar: &Option<egui::TextureHandle>,
    user_label: &str,
    fallback_color: Color32,
    alpha: f32,
) {
    if let Some(tex) = avatar {
        p.image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            with_alpha(Color32::WHITE, alpha),
        );
    } else {
        p.circle_filled(
            rect.center(),
            rect.width() * 0.5,
            with_alpha(fallback_color, alpha),
        );
        let init: String = user_label
            .split('@')
            .next()
            .unwrap_or("U")
            .chars()
            .take(1)
            .collect::<String>()
            .to_uppercase();
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if init.is_empty() { "U" } else { &init },
            semibold_font(9.5),
            with_alpha(Color32::WHITE, alpha),
        );
    }
}

pub fn draw_star_icon(p: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment([Pos2::new(c.x, c.y - r), Pos2::new(c.x, c.y + r)], s);
    p.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], s);
    let d = r * 0.48;
    p.line_segment(
        [Pos2::new(c.x - d, c.y - d), Pos2::new(c.x + d, c.y + d)],
        Stroke::new(1.2, color),
    );
    p.line_segment(
        [Pos2::new(c.x + d, c.y - d), Pos2::new(c.x - d, c.y + d)],
        Stroke::new(1.2, color),
    );
    p.circle_filled(c, r * 0.34, color);
}

pub fn draw_person_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.7, color);
    p.circle_stroke(Pos2::new(c.x, c.y - 4.0), 3.6, s);
    let body = Rect::from_min_max(
        Pos2::new(c.x - 6.5, c.y + 1.5),
        Pos2::new(c.x + 6.5, c.y + 7.5),
    );
    p.rect_stroke(
        body,
        Rounding {
            nw: 6.0,
            ne: 6.0,
            sw: 2.0,
            se: 2.0,
        },
        s,
    );
}

pub fn draw_group_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.circle_stroke(Pos2::new(c.x - 2.5, c.y - 4.0), 3.0, s);
    p.circle_stroke(Pos2::new(c.x + 4.0, c.y - 3.2), 2.3, s);
    let b1 = Rect::from_min_max(
        Pos2::new(c.x - 7.5, c.y + 1.2),
        Pos2::new(c.x + 2.5, c.y + 7.2),
    );
    p.rect_stroke(
        b1,
        Rounding {
            nw: 5.0,
            ne: 5.0,
            sw: 1.5,
            se: 1.5,
        },
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 3.5, c.y + 2.0), Pos2::new(c.x + 7.5, c.y + 2.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 7.5, c.y + 2.0), Pos2::new(c.x + 7.5, c.y + 6.8)],
        s,
    );
}

pub fn draw_logout_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y - 6.5), Pos2::new(c.x, c.y - 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y - 6.5), Pos2::new(c.x - 6.0, c.y + 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y + 6.5), Pos2::new(c.x, c.y + 6.5)],
        s,
    );
    p.line_segment([Pos2::new(c.x - 1.5, c.y), Pos2::new(c.x + 6.5, c.y)], s);
    p.line_segment(
        [Pos2::new(c.x + 3.2, c.y - 3.2), Pos2::new(c.x + 6.5, c.y)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 3.2, c.y + 3.2), Pos2::new(c.x + 6.5, c.y)],
        s,
    );
}

pub fn draw_phone_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8, color);
    p.line_segment(
        [Pos2::new(c.x - 5.0, c.y - 5.5), Pos2::new(c.x - 2.0, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 5.0, c.y - 5.5), Pos2::new(c.x - 6.5, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.5, c.y - 2.5), Pos2::new(c.x + 2.5, c.y + 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.5, c.y + 6.5), Pos2::new(c.x + 5.5, c.y + 5.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 5.5, c.y + 5.0), Pos2::new(c.x + 2.5, c.y + 2.0)],
        s,
    );
}

pub fn draw_hangup_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(2.2, color);
    p.line_segment(
        [Pos2::new(c.x - 9.0, c.y + 2.0), Pos2::new(c.x - 4.0, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 4.0, c.y - 2.5), Pos2::new(c.x + 4.0, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 4.0, c.y - 2.5), Pos2::new(c.x + 9.0, c.y + 2.0)],
        s,
    );
    p.circle_filled(Pos2::new(c.x - 8.5, c.y + 2.5), 2.1, color);
    p.circle_filled(Pos2::new(c.x + 8.5, c.y + 2.5), 2.1, color);
}

pub fn draw_mic_icon(p: &egui::Painter, c: Pos2, muted: bool, color: Color32) {
    let s = Stroke::new(1.8, color);
    let body = Rect::from_center_size(Pos2::new(c.x, c.y - 2.5), Vec2::new(6.8, 10.5));
    p.rect_stroke(body, Rounding::same(3.4), s);
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y - 1.0), Pos2::new(c.x - 6.0, c.y + 2.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 6.0, c.y - 1.0), Pos2::new(c.x + 6.0, c.y + 2.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y + 2.0), Pos2::new(c.x + 6.0, c.y + 2.0)],
        s,
    );
    p.line_segment([Pos2::new(c.x, c.y + 2.0), Pos2::new(c.x, c.y + 7.5)], s);
    p.line_segment(
        [Pos2::new(c.x - 4.0, c.y + 7.5), Pos2::new(c.x + 4.0, c.y + 7.5)],
        s,
    );
    if muted {
        p.line_segment(
            [Pos2::new(c.x - 8.5, c.y - 8.0), Pos2::new(c.x + 8.5, c.y + 8.0)],
            Stroke::new(2.2, DANGER_RED),
        );
    }
}

pub fn draw_speaker_icon(p: &egui::Painter, c: Pos2, muted: bool, color: Color32) {
    let s = Stroke::new(1.8, color);
    let box_r = Rect::from_min_max(
        Pos2::new(c.x - 7.5, c.y - 3.0),
        Pos2::new(c.x - 3.0, c.y + 3.0),
    );
    p.rect_stroke(box_r, Rounding::same(1.0), s);
    p.line_segment(
        [Pos2::new(c.x - 3.0, c.y - 3.0), Pos2::new(c.x + 2.0, c.y - 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.0, c.y - 6.5), Pos2::new(c.x + 2.0, c.y + 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.0, c.y + 6.5), Pos2::new(c.x - 3.0, c.y + 3.0)],
        s,
    );
    if muted {
        p.line_segment(
            [Pos2::new(c.x + 4.5, c.y - 3.0), Pos2::new(c.x + 9.0, c.y + 3.0)],
            Stroke::new(2.0, DANGER_RED),
        );
        p.line_segment(
            [Pos2::new(c.x + 9.0, c.y - 3.0), Pos2::new(c.x + 4.5, c.y + 3.0)],
            Stroke::new(2.0, DANGER_RED),
        );
    } else {
        p.line_segment(
            [Pos2::new(c.x + 5.2, c.y - 3.2), Pos2::new(c.x + 5.2, c.y + 3.2)],
            s,
        );
        p.line_segment(
            [Pos2::new(c.x + 8.4, c.y - 5.5), Pos2::new(c.x + 8.4, c.y + 5.5)],
            s,
        );
    }
}

pub fn draw_screen_share_icon(p: &egui::Painter, c: Pos2, active: bool, color: Color32) {
    let s = Stroke::new(1.8, color);
    let monitor = Rect::from_center_size(Pos2::new(c.x, c.y - 1.5), Vec2::new(18.0, 12.5));
    p.rect_stroke(monitor, Rounding::same(2.2), s);
    p.line_segment(
        [Pos2::new(c.x, c.y + 4.8), Pos2::new(c.x, c.y + 8.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 4.5, c.y + 8.0), Pos2::new(c.x + 4.5, c.y + 8.0)],
        s,
    );
    if active {
        p.line_segment(
            [Pos2::new(c.x, c.y + 1.5), Pos2::new(c.x, c.y - 4.5)],
            s,
        );
        p.line_segment(
            [Pos2::new(c.x - 2.6, c.y - 2.2), Pos2::new(c.x, c.y - 4.5)],
            s,
        );
        p.line_segment(
            [Pos2::new(c.x + 2.6, c.y - 2.2), Pos2::new(c.x, c.y - 4.5)],
            s,
        );
    }
}

pub fn draw_video_play_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    let box_r = Rect::from_center_size(c, Vec2::new(17.0, 12.0));
    p.rect_stroke(box_r, Rounding::same(2.5), s);
    p.line_segment(
        [Pos2::new(c.x - 2.0, c.y - 3.2), Pos2::new(c.x - 2.0, c.y + 3.2)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.0, c.y - 3.2), Pos2::new(c.x + 3.5, c.y)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.0, c.y + 3.2), Pos2::new(c.x + 3.5, c.y)],
        s,
    );
}

pub fn draw_fullscreen_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.7, color);
    let l = 5.0;
    let d = 7.0;
    p.line_segment([Pos2::new(c.x - d, c.y - d + l), Pos2::new(c.x - d, c.y - d)], s);
    p.line_segment([Pos2::new(c.x - d, c.y - d), Pos2::new(c.x - d + l, c.y - d)], s);
    p.line_segment([Pos2::new(c.x + d - l, c.y - d), Pos2::new(c.x + d, c.y - d)], s);
    p.line_segment([Pos2::new(c.x + d, c.y - d), Pos2::new(c.x + d, c.y - d + l)], s);
    p.line_segment([Pos2::new(c.x - d, c.y + d - l), Pos2::new(c.x - d, c.y + d)], s);
    p.line_segment([Pos2::new(c.x - d, c.y + d), Pos2::new(c.x - d + l, c.y + d)], s);
    p.line_segment([Pos2::new(c.x + d - l, c.y + d), Pos2::new(c.x + d, c.y + d)], s);
    p.line_segment([Pos2::new(c.x + d, c.y + d - l), Pos2::new(c.x + d, c.y + d)], s);
}

pub fn draw_exit_fullscreen_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.7, color);
    let l = 4.5;
    let d = 6.5;
    p.line_segment([Pos2::new(c.x - d, c.y - d + l), Pos2::new(c.x - d + l, c.y - d + l)], s);
    p.line_segment([Pos2::new(c.x - d + l, c.y - d), Pos2::new(c.x - d + l, c.y - d + l)], s);
    p.line_segment([Pos2::new(c.x + d - l, c.y - d + l), Pos2::new(c.x + d, c.y - d + l)], s);
    p.line_segment([Pos2::new(c.x + d - l, c.y - d), Pos2::new(c.x + d - l, c.y - d + l)], s);
    p.line_segment([Pos2::new(c.x - d, c.y + d - l), Pos2::new(c.x - d + l, c.y + d - l)], s);
    p.line_segment([Pos2::new(c.x - d + l, c.y + d - l), Pos2::new(c.x - d + l, c.y + d)], s);
    p.line_segment([Pos2::new(c.x + d - l, c.y + d - l), Pos2::new(c.x + d, c.y + d - l)], s);
    p.line_segment([Pos2::new(c.x + d - l, c.y + d - l), Pos2::new(c.x + d - l, c.y + d)], s);
}

pub fn draw_lock_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.4, color);
    let body = Rect::from_center_size(Pos2::new(c.x, c.y + 2.0), Vec2::new(8.5, 6.5));
    p.rect_stroke(body, Rounding::same(1.5), s);
    let arch = Rect::from_center_size(Pos2::new(c.x, c.y - 2.5), Vec2::new(5.5, 5.5));
    p.rect_stroke(
        arch,
        Rounding {
            nw: 3.0,
            ne: 3.0,
            sw: 0.0,
            se: 0.0,
        },
        s,
    );
}

pub fn draw_call_fingerprint_glyph(p: &egui::Painter, c: Pos2, kind: u8, color: Color32) {
    match kind {
        0 => draw_star_icon(p, c, 5.5, color),
        1 => {
            p.circle_stroke(c, 5.0, Stroke::new(1.6, color));
            p.circle_filled(c, 2.2, color);
        }
        2 => {
            let r = 5.4;
            let pts = [
                Pos2::new(c.x, c.y - r),
                Pos2::new(c.x + r, c.y),
                Pos2::new(c.x, c.y + r),
                Pos2::new(c.x - r, c.y),
            ];
            for i in 0..4 {
                p.line_segment([pts[i], pts[(i + 1) % 4]], Stroke::new(1.6, color));
            }
            p.circle_filled(c, 1.8, color);
        }
        _ => {
            let r = Rect::from_center_size(c, Vec2::splat(9.0));
            p.rect_stroke(r, Rounding::same(2.5), Stroke::new(1.6, color));
            p.circle_filled(c, 2.0, color);
        }
    }
}

pub fn draw_sliders_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    for (dy, knob_x) in [(-4.5, -2.5), (0.0, 3.0), (4.5, -1.5)] {
        p.line_segment(
            [Pos2::new(c.x - 7.0, c.y + dy), Pos2::new(c.x + 7.0, c.y + dy)],
            s,
        );
        p.circle_filled(Pos2::new(c.x + knob_x, c.y + dy), 2.3, color);
    }
}

pub fn draw_sun_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    p.circle_filled(c, 3.4, color);
    let s = Stroke::new(1.35, color);
    for i in 0..8 {
        let angle = i as f32 * std::f32::consts::FRAC_PI_4;
        let dir = Vec2::new(angle.cos(), angle.sin());
        p.line_segment([c + dir * 5.2, c + dir * 7.2], s);
    }
}

pub fn draw_moon_icon(p: &egui::Painter, c: Pos2, color: Color32, cutout_bg: Color32) {
    p.circle_filled(c, 5.8, color);
    p.circle_filled(Pos2::new(c.x + 2.6, c.y - 2.0), 4.8, cutout_bg);
}

pub fn draw_theme_slider_row(
    ui: &mut egui::Ui,
    is_dark: bool,
    anim: f32,
    pal: &ThemePalette,
    cutout_bg: Color32,
) -> bool {
    let mut toggled = false;
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 7.0;

        let (sun_rect, sun_resp) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
        draw_sun_icon(
            ui.painter(),
            sun_rect.center(),
            if !is_dark {
                Color32::from_rgb(240, 165, 36)
            } else {
                pal.text_muted
            },
        );
        if sun_resp.clicked() && is_dark {
            toggled = true;
        }

        let (track_rect, track_resp) =
            ui.allocate_exact_size(Vec2::new(42.0, 22.0), egui::Sense::click());
        let p = ui.painter();
        let track_bg = if is_dark {
            AUTH_BLUE
        } else {
            Color32::from_rgb(198, 214, 232)
        };
        p.rect_filled(track_rect, Rounding::same(11.0), track_bg);

        let knob_x = egui::lerp(
            (track_rect.min.x + 11.0_f32)..=(track_rect.max.x - 11.0_f32),
            anim,
        );
        p.circle_filled(Pos2::new(knob_x, track_rect.center().y), 8.2, Color32::WHITE);

        if track_resp.on_hover_text("Переключить тему").clicked() {
            toggled = true;
        }

        let (moon_rect, moon_resp) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
        draw_moon_icon(
            ui.painter(),
            moon_rect.center(),
            if is_dark {
                Color32::from_rgb(122, 192, 255)
            } else {
                pal.text_muted
            },
            cutout_bg,
        );
        if moon_resp.clicked() && !is_dark {
            toggled = true;
        }
    });
    toggled
}

pub fn draw_toggle_switch(ui: &mut egui::Ui, value: &mut bool, pal: &ThemePalette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(38.0, 20.0), egui::Sense::click());
    let bg = if *value { pal.accent_blue } else { pal.bg_input };
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
    let knob_x = if *value {
        rect.max.x - 10.0
    } else {
        rect.min.x + 10.0
    };
    ui.painter()
        .circle_filled(Pos2::new(knob_x, rect.center().y), 7.5, Color32::WHITE);
    if resp.clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

pub fn draw_drawer_row(
    ui: &mut egui::Ui,
    label: &str,
    text_color: Color32,
    pal: &ThemePalette,
    draw_icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, Rounding::same(10.0), pal.bg_hover);
    }
    draw_icon(p, Pos2::new(rect.min.x + 22.0, rect.center().y), text_color);
    p.text(
        Pos2::new(rect.min.x + 48.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        text_color,
    );
    resp
}

pub fn draw_avatar_circle(
    p: &egui::Painter,
    rect: Rect,
    avatar: &Option<egui::TextureHandle>,
    fallback_text: &str,
    fallback_color: Color32,
    font_size: f32,
) {
    if let Some(tex) = avatar {
        p.image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        p.circle_filled(rect.center(), rect.width() * 0.5, fallback_color);
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            fallback_text,
            semibold_font(font_size),
            Color32::WHITE,
        );
    }
}

pub fn draw_paperclip_button(ui: &mut egui::Ui, is_open: bool, pal: &ThemePalette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
    let p = ui.painter();
    let c = rect.center();
    if response.hovered() || is_open {
        p.circle_filled(c, 17.0, pal.bg_hover);
    }
    let color = if is_open || response.hovered() {
        pal.accent_light
    } else {
        pal.text_muted
    };
    let s = Stroke::new(1.8, color);
    p.line_segment(
        [Pos2::new(c.x - 5.0, c.y + 1.0), Pos2::new(c.x + 3.0, c.y - 7.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 3.0, c.y - 7.0), Pos2::new(c.x + 7.0, c.y - 3.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 7.0, c.y - 3.0), Pos2::new(c.x - 2.0, c.y + 6.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.0, c.y + 6.0), Pos2::new(c.x - 5.5, c.y + 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 5.5, c.y + 2.5), Pos2::new(c.x + 1.5, c.y - 4.5)],
        s,
    );
    response
}

pub fn draw_copy_icon(p: &egui::Painter, c: Pos2, color: Color32, fill_bg: Color32) {
    let s = Stroke::new(1.5, color);
    let back = Rect::from_min_size(Pos2::new(c.x - 6.0, c.y - 7.0), Vec2::new(9.5, 11.0));
    let front = Rect::from_min_size(Pos2::new(c.x - 2.5, c.y - 3.5), Vec2::new(9.5, 11.0));
    p.rect_stroke(back, Rounding::same(2.0), s);
    p.rect_filled(front, Rounding::same(2.0), fill_bg);
    p.rect_stroke(front, Rounding::same(2.0), s);
}

pub fn draw_forward_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment(
        [Pos2::new(c.x - 6.5, c.y + 1.0), Pos2::new(c.x + 5.5, c.y + 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 1.5, c.y - 3.5), Pos2::new(c.x + 6.0, c.y + 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 1.5, c.y + 5.5), Pos2::new(c.x + 6.0, c.y + 1.0)],
        s,
    );
}

pub fn draw_trash_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.5, color);
    p.line_segment(
        [Pos2::new(c.x - 6.5, c.y - 4.5), Pos2::new(c.x + 6.5, c.y - 4.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.5, c.y - 7.0), Pos2::new(c.x + 2.5, c.y - 7.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.5, c.y - 7.0), Pos2::new(c.x - 2.5, c.y - 4.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.5, c.y - 7.0), Pos2::new(c.x + 2.5, c.y - 4.5)],
        s,
    );
    let body = Rect::from_min_max(
        Pos2::new(c.x - 5.0, c.y - 4.5),
        Pos2::new(c.x + 5.0, c.y + 6.5),
    );
    p.rect_stroke(body, Rounding::same(1.5), s);
    p.line_segment(
        [Pos2::new(c.x - 1.8, c.y - 1.5), Pos2::new(c.x - 1.8, c.y + 3.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 1.8, c.y - 1.5), Pos2::new(c.x + 1.8, c.y + 3.5)],
        s,
    );
}

pub fn draw_menu_icon(p: &egui::Painter, c: Pos2, kind: AttachmentKind, color: Color32) {
    let s = Stroke::new(1.6, color);
    match kind {
        AttachmentKind::Media => {
            let r = Rect::from_center_size(c, Vec2::new(16.0, 13.0));
            p.rect_stroke(r, Rounding::same(2.5), s);
            p.circle_filled(Pos2::new(c.x - 3.5, c.y - 2.5), 1.5, color);
            p.line_segment(
                [Pos2::new(r.min.x + 2.0, r.max.y - 2.0), Pos2::new(c.x - 1.0, c.y + 0.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x - 1.0, c.y + 0.5), Pos2::new(c.x + 2.0, c.y + 3.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 0.5, c.y + 1.5), Pos2::new(c.x + 4.0, c.y - 1.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 4.0, c.y - 1.5), Pos2::new(r.max.x - 1.5, r.max.y - 2.0)],
                s,
            );
        }
        AttachmentKind::Document => {
            let r = Rect::from_center_size(c, Vec2::new(13.0, 16.0));
            p.rect_stroke(r, Rounding::same(2.0), s);
            p.line_segment(
                [Pos2::new(c.x - 3.0, c.y - 2.0), Pos2::new(c.x + 3.0, c.y - 2.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x - 3.0, c.y + 2.0), Pos2::new(c.x + 3.0, c.y + 2.0)],
                s,
            );
        }
        AttachmentKind::Poll => {
            p.line_segment(
                [Pos2::new(c.x - 5.0, c.y + 6.0), Pos2::new(c.x - 5.0, c.y)],
                s,
            );
            p.line_segment([Pos2::new(c.x, c.y + 6.0), Pos2::new(c.x, c.y - 6.0)], s);
            p.line_segment(
                [Pos2::new(c.x + 5.0, c.y + 6.0), Pos2::new(c.x + 5.0, c.y - 2.0)],
                s,
            );
        }
        AttachmentKind::Article => {
            let r = Rect::from_center_size(c, Vec2::new(15.0, 15.0));
            p.rect_stroke(r, Rounding::same(2.0), s);
            p.line_segment(
                [Pos2::new(c.x - 4.0, c.y - 3.5), Pos2::new(c.x + 4.0, c.y - 3.5)],
                s,
            );
            p.line_segment([Pos2::new(c.x - 4.0, c.y), Pos2::new(c.x + 4.0, c.y)], s);
            p.line_segment(
                [Pos2::new(c.x - 4.0, c.y + 3.5), Pos2::new(c.x + 1.5, c.y + 3.5)],
                s,
            );
        }
        AttachmentKind::Location => {
            p.circle_stroke(Pos2::new(c.x, c.y - 2.0), 5.0, s);
            p.circle_filled(Pos2::new(c.x, c.y - 2.0), 1.6, color);
            p.line_segment(
                [Pos2::new(c.x - 3.5, c.y + 2.0), Pos2::new(c.x, c.y + 7.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 3.5, c.y + 2.0), Pos2::new(c.x, c.y + 7.5)],
                s,
            );
        }
        AttachmentKind::Music => {
            p.circle_stroke(Pos2::new(c.x - 3.5, c.y + 4.5), 2.3, s);
            p.circle_stroke(Pos2::new(c.x + 4.5, c.y + 3.0), 2.3, s);
            p.line_segment(
                [Pos2::new(c.x - 1.2, c.y + 4.5), Pos2::new(c.x - 1.2, c.y - 5.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 6.8, c.y + 3.0), Pos2::new(c.x + 6.8, c.y - 6.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x - 1.2, c.y - 5.0), Pos2::new(c.x + 6.8, c.y - 6.5)],
                s,
            );
        }
    }
}

pub fn draw_send_button(ui: &mut egui::Ui, is_ready: bool, pal: &ThemePalette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
    let painter = ui.painter();
    let center = rect.center();
    let fill_color = if !is_ready {
        pal.bg_input
    } else if response.hovered() {
        pal.accent_blue_hover
    } else {
        pal.accent_blue
    };
    painter.circle_filled(center, 17.5, fill_color);
    let arrow_color = if is_ready {
        Color32::WHITE
    } else {
        pal.text_muted
    };
    let stroke = Stroke::new(2.4, arrow_color);
    let top = Pos2::new(center.x, center.y - 6.5);
    painter.line_segment([Pos2::new(center.x, center.y + 6.5), top], stroke);
    painter.line_segment([top, Pos2::new(center.x - 5.2, center.y - 1.3)], stroke);
    painter.line_segment([top, Pos2::new(center.x + 5.2, center.y - 1.3)], stroke);
    response
}

pub fn draw_call_circle_btn(
    ui: &mut egui::Ui,
    bg: Color32,
    hover_bg: Color32,
    label: &str,
    draw_icon: impl FnOnce(&egui::Painter, Pos2),
) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::click());
        let fill = if resp.hovered() { hover_bg } else { bg };
        ui.painter().circle_filled(rect.center(), 25.5, fill);
        draw_icon(ui.painter(), rect.center());
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(label)
                .size(11.5)
                .color(Color32::from_rgb(198, 214, 232)),
        );
        if resp.clicked() {
            clicked = true;
        }
    });
    clicked
}

impl EgrammApp {
    pub fn show_intro_animation(&mut self, ctx: &egui::Context) {
        let Some(start) = self.intro_start else {
            return;
        };

        let t = start.elapsed().as_secs_f32();
        let total = 2.45_f32;
        if t >= total {
            self.intro_start = None;
            return;
        }
        ctx.request_repaint();

        let screen = ctx.screen_rect();
        let center = screen.center();

        let exit_t = ((t - 1.90) / 0.55).clamp(0.0, 1.0);
        let master_alpha = 1.0 - ease_out_cubic(exit_t);
        let exit_zoom = 1.0 + ease_out_cubic(exit_t) * 0.08;

        let mut skip = false;

        egui::Area::new(egui::Id::new("egramm_first_intro"))
            .fixed_pos(screen.min)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                if resp.clicked() && t > 0.35 {
                    skip = true;
                }
                let p = ui.painter();

                p.rect_filled(
                    rect,
                    Rounding::ZERO,
                    with_alpha(Color32::from_rgb(9, 15, 24), master_alpha),
                );

                let glow_t = ease_out_cubic((t / 0.85).clamp(0.0, 1.0));
                let glow_center = Pos2::new(center.x, center.y - 24.0);
                for (r_base, col, a) in [
                    (320.0, Color32::from_rgb(28, 92, 200), 0.12),
                    (210.0, Color32::from_rgb(43, 135, 232), 0.16),
                    (125.0, Color32::from_rgb(80, 185, 255), 0.20),
                ] {
                    let breathe = 1.0 + (t * 2.8).sin() * 0.03;
                    p.circle_filled(
                        glow_center,
                        r_base * glow_t * breathe * exit_zoom,
                        with_alpha(col, a * master_alpha),
                    );
                }

                let logo_t = ((t - 0.12) / 0.52).clamp(0.0, 1.0);
                let logo_scale = ease_out_back(logo_t) * exit_zoom;
                let logo_alpha = ease_out_cubic((t - 0.10) / 0.28) * master_alpha;

                if logo_alpha > 0.01 {
                    let r = 46.0 * logo_scale.max(0.01);
                    p.circle_filled(glow_center, r, with_alpha(AUTH_BLUE, logo_alpha));
                    p.circle_stroke(
                        glow_center,
                        r,
                        Stroke::new(
                            2.0,
                            with_alpha(Color32::from_rgb(156, 218, 255), 0.75 * logo_alpha),
                        ),
                    );
                    p.text(
                        glow_center,
                        egui::Align2::CENTER_CENTER,
                        "E",
                        semibold_font((46.0 * logo_scale).max(8.0)),
                        with_alpha(Color32::WHITE, logo_alpha),
                    );
                }

                let title_t = ease_out_cubic(((t - 0.42) / 0.42).clamp(0.0, 1.0));
                if title_t > 0.01 {
                    let title_y = glow_center.y + 82.0 + (1.0 - title_t) * 20.0;
                    p.text(
                        Pos2::new(center.x, title_y),
                        egui::Align2::CENTER_CENTER,
                        "Egramm",
                        semibold_font(34.0 * exit_zoom),
                        with_alpha(Color32::WHITE, title_t * master_alpha),
                    );
                }

                let welcome_t = ease_out_cubic(((t - 0.75) / 0.42).clamp(0.0, 1.0));
                if welcome_t > 0.01 {
                    let wy = glow_center.y + 124.0 + (1.0 - welcome_t) * 14.0;
                    p.text(
                        Pos2::new(center.x, wy),
                        egui::Align2::CENTER_CENTER,
                        format!("Добро пожаловать, {}", self.username),
                        FontId::proportional(17.0),
                        with_alpha(Color32::from_rgb(198, 224, 250), welcome_t * master_alpha),
                    );
                }
            });

        if skip {
            self.intro_start = None;
        }
    }

    pub fn show_login_screen(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        let elapsed = self.auth_anim_start.elapsed().as_secs_f32();
        if elapsed < 1.1 || self.is_authenticating {
            ctx.request_repaint();
        }

        let card_t = ease_out_cubic(elapsed / 0.45);
        let logo_t = ease_out_cubic((elapsed - 0.08) / 0.50);
        let text_t = ease_out_cubic((elapsed - 0.22) / 0.45);
        let btn_t = ease_out_cubic((elapsed - 0.36) / 0.48);

        let mut theme_toggled = false;
        let mut cancel_login = false;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.auth_bg))
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(18.0);
                        egui::Frame::none()
                            .fill(pal.auth_card)
                            .stroke(Stroke::new(1.0, pal.auth_border))
                            .rounding(Rounding::same(18.0))
                            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                            .show(ui, |ui| {
                                if draw_theme_slider_row(
                                    ui,
                                    self.is_dark_mode,
                                    self.theme_anim,
                                    &pal,
                                    pal.auth_card,
                                ) {
                                    theme_toggled = true;
                                }
                            });
                    });
                });

                let avail = ui.available_size();
                let card_w = 380.0;
                let card_h = 370.0;

                let float_y = (1.0 - card_t) * 26.0;
                let top_pad = ((avail.y - card_h) * 0.5 + float_y - 16.0).max(12.0);
                let left_pad = ((avail.x - card_w) * 0.5).max(16.0);

                ui.add_space(top_pad);
                ui.horizontal(|ui| {
                    ui.add_space(left_pad);

                    egui::Frame::none()
                        .fill(with_alpha(pal.auth_card, card_t))
                        .stroke(Stroke::new(1.2, with_alpha(pal.auth_border, card_t)))
                        .rounding(Rounding::same(24.0))
                        .inner_margin(egui::Margin::symmetric(32.0, 34.0))
                        .show(ui, |ui| {
                            ui.set_width(card_w - 64.0);
                            ui.vertical_centered(|ui| {
                                let logo_offset = (1.0 - logo_t) * 18.0;
                                ui.add_space(logo_offset);

                                let (logo_rect, logo_resp) =
                                    ui.allocate_exact_size(Vec2::splat(84.0), egui::Sense::click());
                                let p = ui.painter();
                                let c = logo_rect.center();

                                let hover_scale = if logo_resp.hovered() { 2.0 } else { 0.0 };
                                let r = (36.0 + hover_scale) * (0.85 + 0.15 * logo_t);

                                p.circle_filled(c, r + 7.0, with_alpha(pal.auth_blue_soft, logo_t));
                                p.circle_filled(
                                    c,
                                    r,
                                    with_alpha(
                                        if logo_resp.hovered() {
                                            AUTH_BLUE_HOVER
                                        } else {
                                            AUTH_BLUE
                                        },
                                        logo_t,
                                    ),
                                );
                                p.text(
                                    c,
                                    egui::Align2::CENTER_CENTER,
                                    "E",
                                    semibold_font(36.0),
                                    with_alpha(Color32::WHITE, logo_t),
                                );

                                if logo_resp.clicked() && !self.is_authenticating {
                                    self.auth_anim_start = Instant::now();
                                }

                                ui.add_space(16.0 - logo_offset * 0.5);

                                ui.label(
                                    egui::RichText::new("Egramm")
                                        .font(semibold_font(24.0))
                                        .color(with_alpha(pal.auth_text, text_t)),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("Вход в аккаунт")
                                        .size(14.0)
                                        .color(with_alpha(pal.auth_muted, text_t)),
                                );

                                ui.add_space(24.0 + (1.0 - btn_t) * 14.0);

                                let btn_w = ui.available_width();
                                let btn_h = 46.0;
                                let (btn_rect, btn_resp) = ui.allocate_exact_size(
                                    Vec2::new(btn_w, btn_h),
                                    egui::Sense::click(),
                                );

                                let btn_bg = if self.is_authenticating {
                                    pal.auth_blue_soft
                                } else if btn_resp.hovered() {
                                    AUTH_BLUE_HOVER
                                } else {
                                    AUTH_BLUE
                                };

                                let bp = ui.painter();
                                bp.rect_filled(
                                    btn_rect,
                                    Rounding::same(23.0),
                                    with_alpha(btn_bg, btn_t),
                                );

                                if self.is_authenticating {
                                    bp.text(
                                        btn_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "Подтвердите вход в браузере...",
                                        semibold_font(14.0),
                                        AUTH_BLUE,
                                    );
                                } else {
                                    let badge_center =
                                        Pos2::new(btn_rect.min.x + 24.0, btn_rect.center().y);
                                    bp.circle_filled(
                                        badge_center,
                                        14.0,
                                        with_alpha(Color32::WHITE, btn_t),
                                    );
                                    bp.text(
                                        badge_center,
                                        egui::Align2::CENTER_CENTER,
                                        "G",
                                        semibold_font(16.0),
                                        with_alpha(AUTH_BLUE, btn_t),
                                    );

                                    bp.text(
                                        Pos2::new(btn_rect.center().x + 8.0, btn_rect.center().y),
                                        egui::Align2::CENTER_CENTER,
                                        "Войти через Google",
                                        semibold_font(15.0),
                                        with_alpha(Color32::WHITE, btn_t),
                                    );

                                    if btn_resp.clicked() && btn_t > 0.5 {
                                        self.start_google_login();
                                    }
                                }

                                if self.is_authenticating {
                                    ui.add_space(10.0);
                                    if ui
                                        .add_sized(
                                            Vec2::new(btn_w, 30.0),
                                            egui::Button::new(
                                                egui::RichText::new(
                                                    "Закрыли вкладку? Отменить и повторить",
                                                )
                                                .size(12.2)
                                                .color(DANGER_RED),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .rounding(15.0),
                                        )
                                        .clicked()
                                    {
                                        cancel_login = true;
                                    }
                                }

                                if let Some(err) = &self.auth_error {
                                    ui.add_space(10.0);
                                    ui.label(egui::RichText::new(err).size(12.0).color(DANGER_RED));
                                }

                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new("Защищено TLS + ChaCha20-Poly1305")
                                        .size(11.5)
                                        .color(with_alpha(pal.auth_muted, btn_t)),
                                );
                            });
                        });
                });
            });

        if cancel_login {
            self.cancel_google_login();
        }

        if theme_toggled {
            self.is_dark_mode = !self.is_dark_mode;
            let new_pal = self.palette();
            apply_theme_style(ctx, &new_pal, self.is_dark_mode);
        }
    }

    pub fn show_lightbox(&mut self, ctx: &egui::Context) {
        let Some(path) = self.lightbox_image.clone() else {
            return;
        };

        let elapsed = self.lightbox_open_time.elapsed().as_secs_f32();
        let fade = ease_out_cubic(elapsed / 0.16);
        if elapsed < 0.2 {
            ctx.request_repaint();
        }

        let scroll_y = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll_y.abs() > 0.2 {
            let factor = if scroll_y > 0.0 { 1.14 } else { 1.0 / 1.14 };
            self.lightbox_zoom = (self.lightbox_zoom * factor).clamp(0.5, 6.0);
            if self.lightbox_zoom <= 1.02 {
                self.lightbox_offset = Vec2::ZERO;
            }
        }

        let tex = get_or_load_image(ctx, &mut self.image_cache, &path);
        let screen = ctx.screen_rect();
        let mut close_now = false;

        egui::Area::new(egui::Id::new("tg_photo_lightbox"))
            .fixed_pos(screen.min)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (bg_rect, bg_resp) =
                    ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());

                ui.painter().rect_filled(
                    bg_rect,
                    Rounding::ZERO,
                    Color32::from_rgba_unmultiplied(8, 12, 18, (232.0 * fade) as u8),
                );

                let mut img_rect = Rect::NOTHING;
                if let Some(t) = &tex {
                    let orig = t.size_vec2();
                    let max_w = (screen.width() - 120.0).max(200.0);
                    let max_h = (screen.height() - 150.0).max(200.0);
                    let fit_scale = (max_w / orig.x).min(max_h / orig.y).min(1.0);
                    let base_size = orig * fit_scale;
                    let disp_size = base_size * self.lightbox_zoom * (0.94 + 0.06 * fade);

                    img_rect =
                        Rect::from_center_size(screen.center() + self.lightbox_offset, disp_size);

                    ui.painter().image(
                        t.id(),
                        img_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        with_alpha(Color32::WHITE, fade),
                    );
                }

                let bar_w = 290.0;
                let bar_h = 40.0;
                let bar_rect = Rect::from_center_size(
                    Pos2::new(screen.center().x, screen.max.y - 38.0),
                    Vec2::new(bar_w, bar_h),
                );
                let close_rect = Rect::from_center_size(
                    Pos2::new(screen.max.x - 32.0, screen.min.y + 30.0),
                    Vec2::splat(38.0),
                );

                if bg_resp.dragged() && self.lightbox_zoom > 1.01 {
                    self.lightbox_offset += bg_resp.drag_delta();
                }

                if bg_resp.double_clicked() {
                    if let Some(pos) = bg_resp.interact_pointer_pos() {
                        if img_rect.contains(pos) {
                            if self.lightbox_zoom > 1.15 {
                                self.lightbox_zoom = 1.0;
                                self.lightbox_offset = Vec2::ZERO;
                            } else {
                                self.lightbox_zoom = 2.2;
                            }
                        }
                    }
                } else if bg_resp.clicked() {
                    if let Some(pos) = bg_resp.interact_pointer_pos() {
                        if !img_rect.contains(pos)
                            && !bar_rect.contains(pos)
                            && !close_rect.contains(pos)
                        {
                            close_now = true;
                        }
                    }
                }

                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("Фото");
                ui.painter().text(
                    Pos2::new(screen.min.x + 24.0, screen.min.y + 30.0),
                    egui::Align2::LEFT_CENTER,
                    fname,
                    semibold_font(14.5),
                    with_alpha(Color32::WHITE, fade),
                );

                let close_resp = ui.interact(
                    close_rect,
                    egui::Id::new("lightbox_close_btn"),
                    egui::Sense::click(),
                );
                ui.painter().circle_filled(
                    close_rect.center(),
                    18.0,
                    with_alpha(
                        if close_resp.hovered() {
                            Color32::from_rgb(56, 70, 88)
                        } else {
                            Color32::from_rgb(30, 40, 52)
                        },
                        fade,
                    ),
                );
                ui.painter().text(
                    close_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "✕",
                    semibold_font(16.0),
                    with_alpha(Color32::WHITE, fade),
                );
                if close_resp.clicked() {
                    close_now = true;
                }

                ui.painter().rect_filled(
                    bar_rect,
                    Rounding::same(20.0),
                    with_alpha(Color32::from_rgb(24, 34, 46), 0.94 * fade),
                );
                ui.painter().rect_stroke(
                    bar_rect,
                    Rounding::same(20.0),
                    Stroke::new(1.0, with_alpha(Color32::from_rgb(52, 70, 92), fade)),
                );

                let mut bar_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(bar_rect.shrink2(Vec2::new(10.0, 4.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );

                if bar_ui
                    .add_sized(
                        Vec2::new(34.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("−").size(18.0).color(Color32::WHITE),
                        )
                        .fill(Color32::TRANSPARENT)
                        .rounding(15.0),
                    )
                    .clicked()
                {
                    self.lightbox_zoom = (self.lightbox_zoom / 1.25).clamp(0.5, 6.0);
                    if self.lightbox_zoom <= 1.02 {
                        self.lightbox_offset = Vec2::ZERO;
                    }
                }

                let pct = format!("{}%", (self.lightbox_zoom * 100.0).round() as i32);
                if bar_ui
                    .add_sized(
                        Vec2::new(62.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new(pct)
                                .font(semibold_font(13.0))
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::TRANSPARENT)
                        .rounding(15.0),
                    )
                    .on_hover_text("Сбросить масштаб")
                    .clicked()
                {
                    self.lightbox_zoom = 1.0;
                    self.lightbox_offset = Vec2::ZERO;
                }

                if bar_ui
                    .add_sized(
                        Vec2::new(34.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("+").size(18.0).color(Color32::WHITE),
                        )
                        .fill(Color32::TRANSPARENT)
                        .rounding(15.0),
                    )
                    .clicked()
                {
                    self.lightbox_zoom = (self.lightbox_zoom * 1.25).clamp(0.5, 6.0);
                }

                bar_ui.add_space(10.0);
                if bar_ui
                    .add_sized(
                        Vec2::new(108.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("Открыть файл")
                                .size(12.5)
                                .color(Color32::from_rgb(125, 194, 255)),
                        )
                        .fill(Color32::from_rgb(34, 48, 66))
                        .rounding(15.0),
                    )
                    .clicked()
                {
                    let _ = open::that(&path);
                }
            });

        if close_now {
            self.lightbox_image = None;
            self.lightbox_zoom = 1.0;
            self.lightbox_offset = Vec2::ZERO;
        }
    }
}