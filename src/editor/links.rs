//! Clickable hyperlink handling drawn on top of the text edit, mirroring
//! `egui_code_editor::hyperlinks`.

use eframe::egui;
use egui::widgets::text_edit::TextEditOutput;
use egui_code_editor::highlighting::Links;

use super::SPACE_HOLDER;

pub(crate) fn handle_links(text_edit: &TextEditOutput, links: &Links) {
    if !text_edit.response.contains_pointer() {
        return;
    }
    let galley = &text_edit.galley;
    let top_left = text_edit.galley_pos.to_vec2();
    let ctx = &text_edit.response.ctx;

    let chars = galley.chars().collect::<Vec<char>>();
    let char_count = chars.len();
    for link_range in links {
        let url = match chars.get(link_range.clone()) {
            Some(c) => c.iter().collect::<String>(),
            None => continue,
        };
        let start = link_range.start.min(char_count);
        let end = link_range.end.min(char_count.saturating_sub(1));
        let cursors = (start..=end)
            .map(|index| {
                galley.pos_from_cursor(egui::text::CCursor {
                    index: index.into(),
                    prefer_next_row: false,
                })
            })
            .collect::<Vec<egui::Rect>>();
        let rects = join_cursor_rects(&cursors, top_left);
        for rect in rects {
            if ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p)) {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);

                if ctx.input(|r| r.pointer.primary_pressed()) {
                    if url.to_lowercase().starts_with("file://") {
                        let path = &url[7..].replace(SPACE_HOLDER, " ");
                        opener::open(path)
                            .inspect_err(|e| {
                                if cfg!(debug_assertions) {
                                    println!("{e:?}");
                                }
                            })
                            .ok();
                    } else {
                        let url = if url.to_lowercase().starts_with("www") {
                            format!("https://{url}")
                        } else {
                            url.to_string()
                        };
                        ctx.open_url(egui::OpenUrl { url: url.clone(), new_tab: true });
                    }
                }
            }
        }
    }
}

fn join_cursor_rects(cursors: &[egui::Rect], top_left: egui::Vec2) -> Vec<egui::Rect> {
    let mut rects = Vec::<egui::Rect>::new();
    if cursors.is_empty() {
        return rects;
    }
    let mut last = cursors.first().copied().expect("first exists");
    let mut start_x = last.min.x;
    let mut cursors = cursors.iter().peekable();

    while let Some(current) = cursors.next() {
        if cursors.peek().is_none_or(|n| n.min.y != last.min.y) {
            if current.min.y == last.min.y {
                last = *current;
            }
            rects.push(
                egui::Rect::from_min_max(
                    egui::Pos2::new(start_x, last.min.y),
                    egui::Pos2::new(last.max.x, last.max.y),
                )
                .translate(top_left),
            );
            start_x = current.min.x;
        }
        last = *current;
    }

    rects
}