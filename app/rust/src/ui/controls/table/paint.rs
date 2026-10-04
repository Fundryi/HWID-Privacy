//! Column strip, row typography and diff-aware value painting for the compare list.

use super::*;

impl TableData {
    pub(in crate::ui::controls) fn paint_frame(&self, st: &CtlState, dc: HDC) {
        let area = self.frame.get();
        frame(dc, area, theme::BORDER);
        let strip = Rect {
            x: area.x + theme::STROKE,
            y: area.y + theme::STROKE,
            w: area.w - 2 * theme::STROKE,
            h: dpi::scale(theme::COMPARE_HEADER_HEIGHT, st.dpi.get()),
        };
        fill(dc, strip, theme::CARD);
        let mut client = RECT::default();
        // SAFETY: Save and clip the borrowed parent DC; restore before painting the frame.
        let saved = unsafe {
            let saved = windows::Win32::Graphics::Gdi::SaveDC(dc);
            if let Err(error) = GetClientRect(st.hwnd, &mut client) {
                win::record(win::Error::from_win("Compare title size", error));
            }
            windows::Win32::Graphics::Gdi::IntersectClipRect(
                dc,
                strip.x,
                strip.y,
                strip.x + client.right,
                strip.bottom(),
            );
            saved
        };
        let offset = self.scroll_offset(st.hwnd);
        let columns = self.columns.get();
        let padding = dpi::scale(theme::COMPARE_CELL_PADDING, st.dpi.get());
        let fonts = self.fonts.borrow();
        for (i, title) in ["Field", "Before", "After", "Status"].iter().enumerate() {
            paint_text(
                dc,
                title,
                fonts[0].handle(),
                Rect {
                    x: strip.x + columns[i] + padding - offset,
                    y: strip.y,
                    w: columns[i + 1] - columns[i] - 2 * padding,
                    h: strip.h,
                },
                theme::SECONDARY,
            );
        }
        // SAFETY: Restores the parent DC state saved above.
        unsafe {
            if !windows::Win32::Graphics::Gdi::RestoreDC(dc, saved).as_bool() {
                win::record(win::Error::last("Compare title clip restore"));
            }
        }
        fill(
            dc,
            Rect {
                y: strip.bottom() - theme::STROKE,
                h: theme::STROKE,
                ..strip
            },
            theme::BORDER,
        );
    }

    pub(in crate::ui::controls) fn draw(&self, st: &CtlState, dis: &DRAWITEMSTRUCT) {
        let area = Rect {
            // The native LISTBOX already shifts the draw DC's viewport while scrolling.
            x: dis.rcItem.left,
            y: dis.rcItem.top,
            w: self.columns.get()[4].max(dis.rcItem.right - dis.rcItem.left),
            h: dis.rcItem.bottom - dis.rcItem.top,
        };
        let model = self.model.borrow();
        let Some(row) = model.rows.get(dis.itemID as usize) else {
            return;
        };
        let scale = |n| dpi::scale(n, st.dpi.get());
        let fonts = self.fonts.borrow();
        let [meta, small, name, data, icon] = std::array::from_fn(|i| fonts[i].handle());
        let columns = self.columns.get();
        let padding = scale(theme::COMPARE_CELL_PADDING);
        let cell = |i: usize| Rect {
            x: area.x + columns[i] + padding,
            y: area.y,
            w: columns[i + 1] - columns[i] - 2 * padding,
            h: area.h,
        };
        let span = |x: i32, right: i32| Rect {
            x: area.x + x,
            y: area.y,
            w: (right - x).max(0),
            h: area.h,
        };
        let selected = dis.itemState.0 & ODS_SELECTED.0 != 0;
        buffered(dis.hDC, area, |dc| {
            fill(
                dc,
                area,
                if selected {
                    theme::BORDER_STRONG
                } else if matches!(row.id, RowId::Section(_)) {
                    theme::HOVER
                } else {
                    theme::CARD
                },
            );
            match row.id {
                RowId::Notice => paint_text(
                    dc,
                    &row.label,
                    small,
                    span(padding, area.w - padding),
                    theme::WARNING,
                ),
                RowId::More(_) => paint_text(
                    dc,
                    &row.label,
                    small,
                    span(
                        scale(if row.flat {
                            theme::COMPARE_TEXT_INSET
                        } else {
                            theme::COMPARE_FIELD_INSET
                        }),
                        area.w - padding,
                    ),
                    theme::FAINT,
                ),
                RowId::Section(_) | RowId::Device(_) => {
                    let section = matches!(row.id, RowId::Section(_));
                    let inset = scale(if section {
                        theme::COMPARE_SECTION_INSET
                    } else {
                        theme::COMPARE_DEVICE_INSET
                    });
                    draw_glyph(
                        dc,
                        if row.expanded { '\u{e70d}' } else { '\u{e76c}' },
                        icon,
                        span(inset, inset + scale(theme::ICON_PX)),
                        theme::SECONDARY,
                    );
                    let mut x = scale(if section {
                        theme::COMPARE_TEXT_INSET
                    } else {
                        theme::COMPARE_FIELD_INSET
                    });
                    let end = if section {
                        area.w - padding
                    } else {
                        columns[3] - padding
                    };
                    let counts = if row.counts == [0; 2] {
                        [String::new(), String::new(), String::new()]
                    } else {
                        legend(row.counts)
                    };
                    let count_width: i32 = counts.iter().map(|s| width(dc, s, meta)).sum();
                    let suffix = if section && !row.expanded {
                        row.unchanged
                            .map(|n| format!(" · {n} values, none changed"))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let suffix_width = width(dc, &suffix, meta);
                    let gap = scale(theme::COMPARE_CELL_PADDING);
                    let position_width = width(dc, &row.position, small);
                    let title_font = if section { meta } else { name };
                    let reserve = count_width
                        + suffix_width
                        + if count_width > 0 { scale(12) } else { 0 }
                        + if section { 0 } else { position_width + gap };
                    let title_width =
                        width(dc, &row.label, title_font).min((end - x - reserve).max(0));
                    paint_text(
                        dc,
                        &row.label,
                        title_font,
                        span(x, x + title_width),
                        if section {
                            theme::SECONDARY
                        } else {
                            theme::TEXT
                        },
                    );
                    x += title_width;
                    if !section {
                        x += gap;
                        paint_text(
                            dc,
                            &row.position,
                            small,
                            span(x, (x + position_width).min(end)),
                            theme::FAINT,
                        );
                        x += position_width;
                        paint_text(
                            dc,
                            crate::ui::compare::text::kind_text(row.kind),
                            meta,
                            cell(3),
                            if row.kind == Change::Same {
                                theme::FAINT
                            } else {
                                theme::SECONDARY
                            },
                        );
                    }
                    if count_width > 0 {
                        x += scale(12);
                        for (text, color) in
                            counts
                                .iter()
                                .zip([theme::DANGER, theme::FAINT, theme::SUCCESS])
                        {
                            let w = width(dc, text, meta);
                            paint_text(dc, text, meta, span(x, (x + w).min(end)), color);
                            x += w;
                        }
                    }
                    paint_text(dc, &suffix, meta, span(x, end), theme::FAINT);
                }
                RowId::Field(_, _) => {
                    let verdict = if model.verdicts() && row.identifier && !row.not_unique {
                        match row.kind {
                            Change::Same => Some(theme::DANGER),
                            Change::Changed => Some(theme::SUCCESS),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(color) = verdict {
                        fill(
                            dc,
                            Rect {
                                w: theme::COMPARE_VERDICT_BAR,
                                ..area
                            },
                            color,
                        );
                    }
                    let neutral_same = row.kind == Change::Same && verdict.is_none();
                    paint_text(
                        dc,
                        &row.label,
                        small,
                        span(
                            scale(if row.flat {
                                theme::COMPARE_TEXT_INSET
                            } else {
                                theme::COMPARE_FIELD_INSET
                            }),
                            columns[1] - padding,
                        ),
                        if neutral_same {
                            theme::FAINT
                        } else {
                            theme::SECONDARY
                        },
                    );
                    let tag = verdict.unwrap_or(if row.not_unique {
                        theme::SECONDARY
                    } else if row.kind == Change::Same {
                        theme::FAINT
                    } else if row.kind == Change::Changed {
                        theme::TEXT
                    } else {
                        theme::SECONDARY
                    });
                    paint_text(
                        dc,
                        if row.not_unique {
                            "not unique"
                        } else {
                            crate::ui::compare::text::kind_text(row.kind)
                        },
                        meta,
                        cell(3),
                        tag,
                    );
                    if row.kind == Change::Changed && !row.generic.iter().any(|g| *g) {
                        paint_pair(dc, row, data, cell(1), cell(2));
                    } else {
                        let color = if neutral_same {
                            theme::FAINT
                        } else {
                            theme::TEXT
                        };
                        paint_value(
                            dc,
                            &row.before,
                            data,
                            cell(1),
                            if row.generic[0] { theme::FAINT } else { color },
                        );
                        paint_value(
                            dc,
                            &row.after,
                            data,
                            cell(2),
                            if row.generic[1] { theme::FAINT } else { color },
                        );
                    }
                }
            }
        });
    }
}

// Unlike TextRenderer's UI label margins, exact advances keep colored monospace runs joined.
pub(super) fn width(dc: HDC, text: &str, font: HFONT) -> i32 {
    let _font = Select::new(dc, font);
    let mut size = windows::Win32::Foundation::SIZE::default();
    let text: Vec<_> = text.encode_utf16().collect();
    // SAFETY: Valid DC, font selected, UTF-16 slice and writable SIZE.
    if !unsafe { windows::Win32::Graphics::Gdi::GetTextExtentPoint32W(dc, &text, &mut size) }
        .as_bool()
    {
        win::record(win::Error::msg(
            "Compare text width",
            "GDI could not measure the text",
        ));
    }
    size.cx
}

fn paint_text(dc: HDC, text: &str, font: HFONT, area: Rect, color: Color) {
    paint_run(dc, text, font, area, color, DT_END_ELLIPSIS);
}

fn paint_value(dc: HDC, text: &str, font: HFONT, area: Rect, color: Color) {
    paint_run(dc, text, font, area, color, DRAW_TEXT_FORMAT(0));
}

fn paint_run(dc: HDC, text: &str, font: HFONT, area: Rect, color: Color, flags: DRAW_TEXT_FORMAT) {
    if area.w <= 0 || text.is_empty() {
        return;
    }
    let _font = Select::new(dc, font);
    let mut text: Vec<_> = text.encode_utf16().collect();
    let mut area = rect(area);
    // SAFETY: GDI borrows these initialized buffers for this draw call only.
    unsafe {
        SetTextColor(dc, color.colorref());
        SetBkMode(dc, TRANSPARENT);
        DrawTextExW(
            dc,
            &mut text,
            &mut area,
            DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | flags,
            None,
        );
    }
}

fn paint_pair(dc: HDC, row: &Row, font: HFONT, before: Rect, after: Rect) {
    let a: Vec<_> = row.before.chars().collect();
    let b: Vec<_> = row.after.chars().collect();
    let prefix = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    for (chars, area) in [(&a, before), (&b, after)] {
        let shown = chars.iter().enumerate().map(|(i, c)| {
            (
                *c,
                if i < prefix || i >= chars.len() - suffix {
                    theme::SECONDARY
                } else {
                    theme::TEXT
                },
            )
        });
        let mut x = area.x;
        let mut run = String::new();
        let mut color = theme::SECONDARY;
        for (c, next) in shown.chain(std::iter::once(('\0', theme::FAINT))) {
            if next != color {
                let w = width(dc, &run, font);
                paint_value(dc, &run, font, Rect { x, w, ..area }, color);
                x += w;
                run.clear();
                color = next;
            }
            run.push(c);
        }
    }
}
