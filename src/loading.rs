use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};
use image::{AnimationDecoder, Frame, codecs::gif::GifDecoder};

pub struct LoadingOverlay {
    raw: Vec<(egui::ColorImage, Duration)>,
    frames: Vec<(egui::TextureHandle, Duration)>,
    started: Option<Instant>,
    frame_idx: usize,
}

impl LoadingOverlay {
    pub fn new() -> Option<Self> {
        let frames = decode_frames()?;
        if frames.is_empty() {
            return None;
        }
        Some(Self {
            raw: frames,
            frames: Vec::new(),
            started: None,
            frame_idx: 0,
        })
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        let elapsed = now.duration_since(started);

        if self.frames.is_empty() {
            self.frames = self
                .raw
                .iter()
                .map(|(color, dur)| {
                    let tex = ctx.load_texture(
                        "loading_frame",
                        color.clone(),
                        egui::TextureOptions::LINEAR,
                    );
                    (tex, *dur)
                })
                .collect();
        }

        // Advance through frames by accumulated delays (loop when past the end).
        // ponytail: playback slow-down factor, since loding.gif frame delays are tiny.
        let slow = 2.5f64;
        let total: f64 = self.frames.iter().map(|(_, d)| d.as_secs_f64() * slow).sum();
        let mut t = if total == 0.0 {
            0.0
        } else {
            elapsed.as_secs_f64() % total
        };
        for (_, d) in self.frames.iter() {
            let dur = d.as_secs_f64() * slow;
            if t < dur {
                break;
            }
            t -= dur;
            self.frame_idx += 1;
            if self.frame_idx >= self.frames.len() {
                self.frame_idx = 0;
            }
        }

        let screen = ctx.input(|i| i.content_rect());
        egui::Area::new(egui::Id::new("loading_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.painter().rect_filled(screen, 0.0, Color32::from_rgb(12, 12, 12));

                let (tex, _) = &self.frames[self.frame_idx];
                let size = self.raw[self.frame_idx].0.size;
                let aspect = size[1] as f32 / size[0] as f32;
                let disp_w = 140.0_f32;
                let disp_h = disp_w * aspect;
                let center = screen.center();
                let img_rect = egui::Rect::from_center_size(
                    center + egui::vec2(0.0, -30.0),
                    egui::vec2(disp_w, disp_h),
                );
                ui.painter().image(
                    tex.id(),
                    img_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                let text_rect = egui::Rect::from_center_size(
                    center + egui::vec2(0.0, disp_h / 2.0 + 30.0),
                    egui::vec2(200.0, 30.0),
                );
                ui.painter().text(
                    text_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "ione",
                    egui::FontId::proportional(22.0),
                    Color32::WHITE,
                );
            });

        let (_, delay) = &self.frames[self.frame_idx];
        ctx.request_repaint_after(std::time::Duration::from_millis(
            (delay.as_millis() * 5 / 2) as u64,
        ));
    }
}

fn decode_frames() -> Option<Vec<(egui::ColorImage, Duration)>> {
    let bytes = include_bytes!("..\\assets\\icons\\loding.gif");
    let cursor = std::io::Cursor::new(bytes);
    let decoder = GifDecoder::new(cursor).ok()?;
    let frames: Vec<Frame> = decoder.into_frames().collect_frames().ok()?;
    Some(
        frames
            .into_iter()
            .map(|f| {
                let dur = Duration::from(f.delay());
                let buf = f.into_buffer();
                let (w, h) = buf.dimensions();
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &buf.into_raw(),
                );
                (color, dur)
            })
            .collect(),
    )
}
