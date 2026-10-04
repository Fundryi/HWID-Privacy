//! Compare-only decorations on the kit's native buttons and labels.

use super::table::paint::{paint_value, width};
use super::*;

pub(super) fn swatch(dc: HDC, area: Rect, color: Color, hollow: bool, back: Color, round: bool) {
    let Some(mut pixels) = tiny_skia::Pixmap::new(area.w.max(0) as u32, area.h.max(0) as u32)
    else {
        return;
    };
    pixels.fill(skia_color(back));
    let local = Rect { x: 0, y: 0, ..area };
    let radius = if round { area.w / 2 } else { area.w * 3 / 10 };
    let stroke = theme::COMPARE_RING_STROKE * area.w as f32
        / if round {
            theme::COMPARE_DOT
        } else {
            theme::COMPARE_SWATCH
        } as f32;
    if let Some(path) = rounded(local, radius, if hollow { stroke / 2.0 } else { 0.0 }) {
        if hollow {
            pixels.stroke_path(
                &path,
                &skia_paint(color),
                &tiny_skia::Stroke {
                    width: stroke,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
        } else {
            pixels.fill_path(
                &path,
                &skia_paint(color),
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    blit(dc, area.x, area.y, &pixels);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn chip(
    dc: HDC,
    mut area: Rect,
    text: &str,
    font: HFONT,
    count_font: HFONT,
    dot: Option<(Color, bool)>,
    color: Color,
    back: Color,
    dpi: u32,
) {
    if let Some((color, hollow)) = dot {
        let size = dpi::scale(theme::COMPARE_DOT, dpi);
        swatch(
            dc,
            Rect {
                y: area.y + (area.h - size) / 2,
                w: size,
                h: size,
                ..area
            },
            color,
            hollow,
            back,
            true,
        );
        let inset = size + dpi::scale(theme::COMPARE_GAP, dpi);
        area.x += inset;
        area.w -= inset;
    }
    let (caption, count) = chip_parts(text);
    let w = width(dc, caption, font);
    paint_value(dc, caption, font, Rect { w, ..area }, color);
    if !count.is_empty() {
        let x = area.x + w + dpi::scale(theme::COMPARE_SWATCH_GAP, dpi);
        paint_value(
            dc,
            count,
            count_font,
            Rect {
                x,
                w: area.right() - x,
                ..area
            },
            color,
        );
    }
}

pub(super) fn chip_parts(text: &str) -> (&str, &str) {
    match text.find(|c: char| c.is_ascii_digit() || c == '+') {
        Some(i) => (text[..i].trim_end(), &text[i..]),
        None => (text, ""),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn label(
    dc: HDC,
    mut area: Rect,
    spec: &LabelSpec,
    decoration: &CompareLabel,
    font: HFONT,
    count_font: HFONT,
    back: Color,
    dpi: u32,
) {
    let scale = |n| dpi::scale(n, dpi);
    match *decoration {
        CompareLabel::Legend(color, hollow) => {
            let size = scale(theme::COMPARE_SWATCH);
            swatch(
                dc,
                Rect {
                    y: area.y + (area.h - size) / 2,
                    w: size,
                    h: size,
                    ..area
                },
                color,
                hollow,
                back,
                false,
            );
            let inset = size + scale(theme::COMPARE_SWATCH_GAP);
            area.x += inset;
            area.w -= inset;
            paint_value(dc, &spec.text, font, area, spec.fore);
        }
        CompareLabel::Verdict(counts, enabled) => {
            if !enabled || counts == [0; 2] {
                paint_value(
                    dc,
                    if enabled {
                        "No identifiers found"
                    } else {
                        "Masked exports: spoof verdict unavailable"
                    },
                    font,
                    area,
                    theme::FAINT,
                );
                return;
            }
            let parts = [
                ("Spoof check: ".to_owned(), theme::TEXT),
                (counts[1].to_string(), theme::SUCCESS),
                (
                    format!(" of {} unique IDs changed · ", counts[0] + counts[1]),
                    theme::TEXT,
                ),
                (counts[0].to_string(), theme::DANGER),
                (" unchanged".to_owned(), theme::TEXT),
            ];
            let mut x = area.x;
            for (text, color) in parts {
                let font = if color == theme::TEXT {
                    font
                } else {
                    count_font
                };
                let w = width(dc, &text, font);
                paint_value(dc, &text, font, Rect { x, w, ..area }, color);
                x += w;
            }
            x += scale(theme::COMPARE_TEXT_GAP);
            let bar = Rect {
                x,
                y: area.y + (area.h - scale(theme::COMPARE_DOT)) / 2,
                w: (area.right() - x).max(scale(theme::COMPARE_BAR_MIN)),
                h: scale(theme::COMPARE_DOT),
            };
            let Some(mut pixels) = tiny_skia::Pixmap::new(bar.w.max(0) as u32, bar.h.max(0) as u32)
            else {
                return;
            };
            pixels.fill(skia_color(back));
            let local = Rect { x: 0, y: 0, ..bar };
            if let Some(path) = rounded(local, bar.h / 2, 0.0) {
                let id = tiny_skia::Transform::identity();
                pixels.fill_path(
                    &path,
                    &skia_paint(theme::DANGER),
                    tiny_skia::FillRule::Winding,
                    id,
                    None,
                );
                if let Some(mut mask) = tiny_skia::Mask::new(pixels.width(), pixels.height()) {
                    mask.fill_path(&path, tiny_skia::FillRule::Winding, true, id);
                    let share = bar.w as f32 * counts[1] as f32 / (counts[0] + counts[1]) as f32;
                    if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, share, bar.h as f32) {
                        pixels.fill_rect(rect, &skia_paint(theme::SUCCESS), id, Some(&mask));
                    }
                }
            }
            blit(dc, bar.x, bar.y, &pixels);
        }
    }
}
