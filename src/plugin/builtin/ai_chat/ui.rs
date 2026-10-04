//! All egui rendering for the docked chat panel, styled after the ChatGPT
//! mobile app: right-aligned user bubbles in the accent colour, left-aligned
//! assistant bubbles on a raised surface, and a rounded composer row.

use eframe::egui::{self, Align, Color32, Layout, RichText, ScrollArea};

use crate::core::style::Palette;

use super::provider::Provider;
use super::AiChatPlugin;

const BUBBLE_MAX_FRAC: f32 = 0.85;

pub fn render(plugin: &mut AiChatPlugin, ui: &mut egui::Ui, p: &Palette) {
    provider_chips(plugin, ui, p);
    ui.add_space(6.0);
    settings_section(plugin, ui, p);
    plugin.save_fields();
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);

    let input_h = 52.0;
    let msg_h = (ui.available_height() - input_h).max(120.0);
    message_surface(plugin, ui, p, msg_h - 16.0);

    ui.add_space(8.0);
    composer(plugin, ui, p);
}

fn provider_chips(plugin: &mut AiChatPlugin, ui: &mut egui::Ui, _p: &Palette) {
    ui.horizontal_wrapped(|ui| {
        for prov in Provider::ALL {
            if ui
                .selectable_label(plugin.provider() == prov, prov.name())
                .clicked()
            {
                plugin.set_provider(prov);
            }
        }
    });
}

fn settings_section(plugin: &mut AiChatPlugin, ui: &mut egui::Ui, p: &Palette) {
    egui::CollapsingHeader::new(RichText::new("Pengaturan").color(p.text_muted).size(12.0))
        .default_open(false)
        .show(ui, |ui| {
            ui.add_space(4.0);
            field_row(ui, p, "Endpoint", plugin.endpoint_mut(), false);
            field_row(ui, p, "Model", plugin.model_mut(), false);
            field_row(ui, p, "API Key", plugin.api_key_mut(), true);
        });
}

fn field_row(ui: &mut egui::Ui, p: &Palette, label: &str, text: &mut String, secret: bool) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new(label).color(p.text_muted).size(12.0));
        ui.add(
            egui::TextEdit::singleline(text)
                .password(secret)
                .desired_width(f32::INFINITY),
        );
    });
    ui.add_space(4.0);
}

fn message_surface(plugin: &AiChatPlugin, ui: &mut egui::Ui, p: &Palette, max_h: f32) {
    egui::Frame::NONE
        .fill(p.bg)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("ai_chat_messages")
                .max_height(max_h)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if plugin.messages.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Tanyakan apa saja tentang kodemu.")
                                    .color(p.text_muted),
                            );
                        });
                    }
                    for m in &plugin.messages {
                        match m.role {
                            "user" => bubble(ui, &m.text, true, p.accent, Color32::WHITE),
                            "assistant" => bubble(ui, &m.text, false, p.panel_hover, p.text),
                            _ => error_bubble(ui, &m.text, p),
                        }
                        ui.add_space(6.0);
                    }
                    if plugin.pending() {
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new("AI sedang mengetik…")
                                    .color(p.text_muted)
                                    .italics(),
                            );
                        });
                    }
                });
        });
}

/// A single chat bubble: `mine` bubbles sit right, others left.
fn bubble(ui: &mut egui::Ui, text: &str, mine: bool, bg: Color32, fg: Color32) {
    let max_w = (ui.available_width() * BUBBLE_MAX_FRAC).max(80.0);
    let font = egui::FontId::proportional(13.5);
    let job = egui::text::LayoutJob::simple(text.to_string(), font, fg, max_w);
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let pad = egui::vec2(12.0, 8.0);
    let size = galley.size() + egui::vec2(pad.x * 2.0, pad.y * 2.0);

    let row = |ui: &mut egui::Ui| {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(14), bg);
        ui.painter().galley(
            egui::pos2(rect.min.x + pad.x, rect.min.y + pad.y),
            galley,
            fg,
        );
    };

    if mine {
        ui.with_layout(Layout::right_to_left(Align::Min), row);
    } else {
        ui.with_layout(Layout::left_to_right(Align::Min), row);
    }
}

fn error_bubble(ui: &mut egui::Ui, text: &str, p: &Palette) {
    let bg = Color32::from_rgba_unmultiplied(p.diag_error.r(), p.diag_error.g(), p.diag_error.b(), 24);
    bubble(ui, text, false, bg, p.diag_error);
}

fn composer(plugin: &mut AiChatPlugin, ui: &mut egui::Ui, p: &Palette) {
    egui::Frame::NONE
        .fill(p.panel_hover)
        .corner_radius(20.0)
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let send_w = 64.0;
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut plugin.input)
                        .hint_text("Ketik pesan…")
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::symmetric(2, 4))
                        .desired_width((avail - send_w - 8.0).max(60.0)),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = ui
                    .add_sized(
                        [send_w, 26.0],
                        egui::Button::new(RichText::new("Send").color(Color32::WHITE).strong())
                            .fill(p.accent)
                            .corner_radius(13.0),
                    )
                    .clicked();
                if (enter || clicked) && !plugin.pending() {
                    plugin.send();
                    resp.request_focus();
                }
            });
        });
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("Clear").clicked() {
                plugin.clear();
            }
        });
    });
}
