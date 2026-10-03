use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Pos2};
use image::{AnimationDecoder, Frame, codecs::gif::GifDecoder};

pub struct LoadingOverlay {
    raw: &'static [(egui::ColorImage, Duration)],
    frames: Vec<(egui::TextureHandle, Duration)>,
    started: Option<Instant>,
    frame_idx: usize,
    /// Full-screen startup splash that blocks the whole UI (`true`), vs an
    /// editor-area splash that leaves the chrome visible (`false`).
    pub fullscreen: bool,
    /// `(title, detail)` painted under the animation — what is being opened and
    /// how big it is. `None` for the plain startup splash.
    pub caption: Option<(String, String)>,
}

impl LoadingOverlay {
    /// Startup splash, shown once while the application prepares itself.
    pub fn new() -> Option<Self> {
        Self::from_raw(loding_frames()?).map(|mut o| {
            o.fullscreen = true;
            o
        })
    }

    /// Splash for an in-progress open, confined to the editor area.
    pub fn editor_area(caption: (String, String)) -> Option<Self> {
        Self::from_raw(loding_frames()?).map(|mut o| {
            o.fullscreen = false;
            o.caption = Some(caption);
            o
        })
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
            caption: None,
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
            self.frames = upload_frames(ctx, self.raw);
        }

        // Advance on the GIF's own timing. The frame is picked from wall-clock
        // time, so the animation runs at its authored speed no matter how often
        // we actually repaint.
        let total: f64 = self.frames.iter().map(|(_, d)| frame_time(*d)).sum();
        if total > 0.0 {
            let mut t = elapsed.as_secs_f64() % total;
            for (i, (_, d)) in self.frames.iter().enumerate() {
                let dur = frame_time(*d);
                if t < dur {
                    self.frame_idx = i;
                    break;
                }
                t -= dur;
                self.frame_idx = (i + 1) % self.frames.len();
            }
        }

        let screen = area.unwrap_or_else(|| ctx.input(|i| i.content_rect()));
        // Paint straight onto a foreground layer instead of using an `Area`:
        // an `Area` spends its first frame measuring itself and that pass paints
        // nothing (and fades in over the next few frames), which would make the
        // splash blink instead of appearing immediately.
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("loading_overlay"));
        let painter = ctx.layer_painter(layer).with_clip_rect(screen);

        // Translucent veil, not an opaque blackout — the app behind stays
        // visible while the GIF prepares.
        painter.rect_filled(screen, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 140));

        let (tex, _) = &self.frames[self.frame_idx];
        let size = self.raw[self.frame_idx].0.size;
        let aspect = size[1] as f32 / size[0] as f32;
        let disp_w = 140.0_f32;
        let disp_h = disp_w * aspect;
        let center = screen.center();
        let img_rect = egui::Rect::from_center_size(center, egui::vec2(disp_w, disp_h));
        painter.image(
            tex.id(),
            img_rect,
            egui::Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );

        // Caption block, centered under the animation: what is being opened,
        // and how much of it there is.
        if let Some((title, detail)) = &self.caption {
            let style = ctx.global_style();
            let title_font = egui::FontId::proportional(15.0);
            let top = img_rect.bottom() + 18.0;
            painter.text(
                egui::pos2(center.x, top),
                Align2::CENTER_TOP,
                title,
                title_font.clone(),
                style.visuals.text_color(),
            );
            // A full title line of leading, so the two never overlap.
            let leading = ctx.fonts_mut(|f| f.row_height(&title_font));
            painter.text(
                egui::pos2(center.x, top + leading),
                Align2::CENTER_TOP,
                detail,
                egui::FontId::proportional(12.0),
                style.visuals.weak_text_color(),
            );
        }

        // Repaint on the very next frame instead of sleeping until this one is
        // due. A sub-frame deadline lands *after* the refresh it aimed at, so
        // the wake-up slips a whole vsync and the animation steps unevenly -
        // that judder is what a laggy spinner looks like. One repaint per
        // display frame is smooth, and the wall-clock pick above keeps the
        // speed right whatever the repaint cadence turns out to be.
        ctx.request_repaint();
    }
}

/// One animation frame's authored duration, in seconds.
fn frame_time(delay: Duration) -> f64 {
    delay.as_secs_f64()
}

/// Upload the decoded frames to the GPU once per app, then hand out cheap
/// clones: `TextureHandle` is an `Arc`, so a second splash for another big file
/// re-uses the textures instead of re-uploading every frame of the GIF.
fn upload_frames(
    ctx: &egui::Context,
    raw: &'static [(egui::ColorImage, Duration)],
) -> Vec<(egui::TextureHandle, Duration)> {
    thread_local! {
        static CACHE: RefCell<Option<Vec<(egui::TextureHandle, Duration)>>> =
            const { RefCell::new(None) };
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let frames = cache.get_or_insert_with(|| {
            raw.iter()
                .map(|(color, dur)| {
                    let tex = ctx.load_texture(
                        "loading_frame",
                        color.clone(),
                        egui::TextureOptions::LINEAR,
                    );
                    (tex, *dur)
                })
                .collect()
        });
        frames.clone()
    })
}

/// Decode a GIF once per asset (decoding hundreds of frames on every splash
/// would defeat its own purpose).
fn loding_frames() -> Option<&'static [(egui::ColorImage, Duration)]> {
    static FRAMES: OnceLock<Option<Vec<(egui::ColorImage, Duration)>>> = OnceLock::new();
    FRAMES
        .get_or_init(|| decode_frames(include_bytes!("..\\assets\\icons\\app\\loding.gif")))
        .as_deref()
}

/// Longest edge kept for the animation.
///
/// The asset is 2048x2048 in 12 frames and the splash paints it ~140 px wide,
/// so keeping the source size meant uploading and holding ~200 MB of textures
/// (plus another ~200 MB of `ColorImage` clones) before the first frame could
/// be shown - that upload stall is what the spinner looked like: a frozen
/// veil. Decoding is one-time and cached, so shrinking here costs nothing and
/// lands ~200x less data on the GPU.
const MAX_EDGE: u32 = 256;

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
                let buf = if w > MAX_EDGE || h > MAX_EDGE {
                    image::imageops::resize(
                        &buf,
                        MAX_EDGE,
                        MAX_EDGE,
                        image::imageops::FilterType::Triangle,
                    )
                } else {
                    buf
                };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        ctx
    }

    #[test]
    fn the_editor_splash_is_not_fullscreen_and_carries_its_caption() {
        let Some(ov) = LoadingOverlay::editor_area(("big.rs".into(), "12.4 MB".into())) else {
            // Only fails when the GIF asset is missing, which is also fatal for
            // the startup splash — nothing to assert then.
            return;
        };
        assert!(!ov.fullscreen, "the chrome must stay visible");
        assert_eq!(
            ov.caption,
            Some(("big.rs".to_string(), "12.4 MB".to_string()))
        );
    }

    #[test]
    fn the_caption_is_painted_below_the_animation() {
        let Some(mut ov) = LoadingOverlay::editor_area(("big.rs".into(), "12.4 MB".into())) else {
            return;
        };
        let ctx = context();
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |_ui| ov.show(&ctx, Some(screen)),
        );

        // The animation as painted: the uploaded GIF frames are the only
        // managed textures here (the font atlas is not one).
        let mut img: Option<egui::Rect> = None;
        // The caption as painted. `TextShape::pos` is the top-left of the laid
        // out galley, so the painted rect has to be rebuilt from it.
        let mut caption: Vec<(egui::Rect, String)> = Vec::new();
        for cs in &output.shapes {
            match &cs.shape {
                egui::Shape::Mesh(m) if matches!(m.texture_id, egui::TextureId::Managed(_)) => {
                    let mut r = egui::Rect::NOTHING;
                    for v in m.vertices.iter() {
                        r.extend_with(v.pos);
                    }
                    img = Some(img.map_or(r, |old| old.union(r)));
                }
                egui::Shape::Text(t) => {
                    let text = t.galley.job.text.clone();
                    if text == "big.rs" || text.contains("12.4 MB") {
                        caption
                            .push((egui::Rect::from_min_size(t.pos, t.galley.rect.size()), text));
                    }
                }
                _ => {}
            }
        }
        output.drop_without_applying_deltas();

        let img = img.expect("the animation must be painted on the very first frame");
        assert_eq!(caption.len(), 2, "both caption lines must be painted");
        let title = caption.iter().find(|(_, t)| t == "big.rs").expect("title");
        let detail = caption.iter().find(|(_, t)| t != "big.rs").expect("detail");
        for (rect, text) in [title, detail] {
            assert!(
                rect.top() >= img.bottom(),
                "{text:?} at {} overlaps the animation, which ends at {}",
                rect.min,
                img.bottom()
            );
            assert!(
                (rect.center().x - screen.center().x).abs() < 1.0,
                "{text:?} must be centered, got {} vs {}",
                rect.center().x,
                screen.center().x
            );
            assert!(screen.contains_rect(*rect), "{text:?} must stay on screen");
        }
        assert!(
            detail.0.top() >= title.0.bottom(),
            "the size line must sit under the file name"
        );
    }

    #[test]
    fn the_startup_splash_is_fullscreen_and_uncaptioned() {
        let Some(ov) = LoadingOverlay::new() else {
            return;
        };
        assert!(ov.fullscreen);
        assert!(ov.caption.is_none());
    }

    #[test]
    fn the_animation_asks_for_the_next_frame_instead_of_sleeping() {
        let Some(mut ov) = LoadingOverlay::editor_area(("big.rs".into(), "4.0 MB".into())) else {
            return;
        };
        let ctx = context();
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |_ui| ov.show(&ctx, Some(screen)),
        );
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("root viewport output")
            .repaint_delay;
        output.drop_without_applying_deltas();
        assert_eq!(
            delay,
            Duration::ZERO,
            "the animation must step on the next frame; a deadline short of one \
             refresh slips a whole vsync and judders"
        );
    }

    #[test]
    fn the_animation_is_small_enough_to_upload_without_stalling() {
        let frames = loding_frames().expect("splash asset decodes");
        let bytes: usize = frames
            .iter()
            .map(|(image, _)| image.size[0] * image.size[1] * 4)
            .sum();
        for (image, _) in frames {
            assert!(
                image.size[0] <= MAX_EDGE as usize && image.size[1] <= MAX_EDGE as usize,
                "frame at {:?} is bigger than the {MAX_EDGE}px the splash draws",
                image.size
            );
        }
        assert!(
            bytes <= MAX_EDGE as usize * MAX_EDGE as usize * 4 * 32,
            "{bytes} bytes of frames is an upload stall waiting to happen"
        );
    }

    #[test]
    fn the_gif_decodes_to_usable_frames() {
        let frames = loding_frames().expect("splash asset decodes");
        assert!(!frames.is_empty(), "the GIF must have at least one frame");
        let (image, _) = &frames[0];
        assert!(image.size[0] > 0 && image.size[1] > 0);
    }
}
