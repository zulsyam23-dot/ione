use std::sync::OnceLock;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};
use image::{AnimationDecoder, Frame, codecs::gif::GifDecoder};

pub struct LoadingOverlay {
    raw: &'static [(egui::ColorImage, Duration)],
    frames: Vec<(egui::TextureHandle, Duration)>,
    started: Option<Instant>,
    frame_idx: usize,
    /// Full-screen startup splash that blocks the whole UI (`true`), vs an
    /// editor-area splash that leaves the chrome visible (`false`).
    pub fullscreen: bool,
}

impl LoadingOverlay {
    /// Startup splash, shown once while the application prepares itself.
    pub fn new() -> Option<Self> {
        Self::from_raw(loding_frames()?).map(|mut o| {
            o.fullscreen = true;
            o
        })
    }

    /// Editor-area splash for when a heavy file (hundreds/thousands of lines)
    /// is being opened.
    pub fn new_loading_file() -> Option<Self> {
        Self::from_raw(loading_file_frames()?)
    }

    fn from_raw(raw: &'static [(egui::ColorImage, Duration)]) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        Some(Self {
            raw,
            frames: Vec::new(),
            started: None,
            frame_idx: 0,
            fullscreen: false,
        })
    }

    /// True once the splash has been shown for at least `min`. Counted from the
    /// *first frame it was actually painted* (`started`), not from when the
    /// overlay was scheduled — so a slow first frame can't eat the whole
    /// display window.
    pub fn done(&self, min: Duration) -> bool {
        self.started.is_some_and(|s| s.elapsed() >= min)
    }

    pub fn show(&mut self, ctx: &egui::Context, area: Option<egui::Rect>) {
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
        // ponytail: playback slow-down factor, since loading-file.gif frame delays are tiny.
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

        let screen = area.unwrap_or_else(|| ctx.input(|i| i.content_rect()));
        egui::Area::new(egui::Id::new("loading_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                // Translucent veil, not an opaque blackout — the app behind
                // stays visible while the GIF prepares.
                ui.painter().rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 140));

                let (tex, _) = &self.frames[self.frame_idx];
                let size = self.raw[self.frame_idx].0.size;
                let aspect = size[1] as f32 / size[0] as f32;
                let disp_w = 140.0_f32;
                let disp_h = disp_w * aspect;
                let center = screen.center();
                let img_rect = egui::Rect::from_center_size(
                    center,
                    egui::vec2(disp_w, disp_h),
                );
                ui.painter().image(
                    tex.id(),
                    img_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            });

        let (_, delay) = &self.frames[self.frame_idx];
        ctx.request_repaint_after(std::time::Duration::from_millis(
            (delay.as_millis() * 5 / 2) as u64,
        ));
    }
}

/// Decode a GIF once per asset (decoding hundreds of frames on every splash
/// would defeat its own purpose).
fn loding_frames() -> Option<&'static [(egui::ColorImage, Duration)]> {
    static FRAMES: OnceLock<Option<Vec<(egui::ColorImage, Duration)>>> = OnceLock::new();
    FRAMES
        .get_or_init(|| decode_frames(include_bytes!("..\\assets\\icons\\app\\loding.gif")))
        .as_deref()
}

fn loading_file_frames() -> Option<&'static [(egui::ColorImage, Duration)]> {
    static FRAMES: OnceLock<Option<Vec<(egui::ColorImage, Duration)>>> = OnceLock::new();
    FRAMES
        .get_or_init(|| decode_frames(include_bytes!("..\\assets\\icons\\app\\loading-file.gif")))
        .as_deref()
}

fn decode_frames(bytes: &[u8]) -> Option<Vec<(egui::ColorImage, Duration)>> {
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
