use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind};
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};

use crate::style::Palette;

const FONT_SIZE: f32 = 14.0;
const BG: Color32 = Color32::from_rgb(12, 12, 12);

struct TerminalInstance {
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    parser: Arc<Mutex<vt100::Parser>>,
    rows: u16,
    cols: u16,
    spawned: bool,
    error: Option<String>,
}

impl Drop for TerminalInstance {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }
    }
}

impl Default for TerminalInstance {
    fn default() -> Self {
        Self {
            master: None,
            child: None,
            writer: Arc::new(Mutex::new(Box::new(std::io::sink()))),
            parser: Arc::new(Mutex::new(vt100::Parser::new(24, 80, 4000))),
            rows: 24,
            cols: 80,
            spawned: false,
            error: None,
        }
    }
}

pub struct TerminalPanel {
    pub visible: bool,
    sessions: Vec<TerminalInstance>,
    pub active: usize,
    focused: bool,
}

impl Default for TerminalPanel {
    fn default() -> Self {
        Self {
            visible: false,
            sessions: vec![TerminalInstance::default()],
            active: 0,
            focused: false,
        }
    }
}

impl TerminalPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn the PTY for the currently active session (idempotent).
    pub fn spawn(&mut self, ctx: &egui::Context) {
        let idx = self.active;
        if idx >= self.sessions.len() {
            return;
        }
        let s = &mut self.sessions[idx];
        if s.spawned {
            return;
        }

        let pty_system = NativePtySystem::default();
        let pair = match pty_system.openpty(PtySize {
            rows: s.rows,
            cols: s.cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) => {
                s.error = Some(format!("failed to create PTY: {e}"));
                return;
            }
        };

        // PowerShell 7 ships as pwsh; Windows PowerShell (5.1) as powershell.
        // Prefer pwsh but fall back so stripped installs still get a shell.
        let mut cmd = CommandBuilder::new("pwsh");
        cmd.arg("-NoProfile");
        let child = match pair
            .slave
            .spawn_command(cmd)
            .or_else(|_| {
                let mut fb = CommandBuilder::new("powershell");
                fb.arg("-NoProfile");
                pair.slave.spawn_command(fb)
            })
        {
            Ok(c) => Some(c),
            Err(e) => {
                s.error = Some(format!("failed to start shell (pwsh/powershell): {e}"));
                return;
            }
        };

        let writer = match pair.master.take_writer() {
            Ok(w) => w,
            Err(e) => {
                s.error = Some(format!("failed to take PTY writer: {e}"));
                return;
            }
        };
        let reader = match pair.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => {
                s.error = Some(format!("failed to clone PTY reader: {e}"));
                return;
            }
        };

        s.master = Some(pair.master);
        s.child = child;
        *s.writer.lock().unwrap_or_else(|e| e.into_inner()) = writer;
        s.spawned = true;
        s.error = None;

        let parser = Arc::clone(&s.parser);
        let writer = Arc::clone(&s.writer);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut response = Vec::new();
                        let mut filtered = Vec::with_capacity(n);
                        let data = &buf[..n];

                        // Answer cursor position reports so ConPTY/PowerShell don't hang.
                        let mut i = 0;
                        while i < data.len() {
                            if data[i] == 0x1b && data.get(i + 2) == Some(&b'n') && data.get(i + 1) == Some(&b'[') {
                                response.extend_from_slice(b"\x1b[1;1R");
                                i += 3;
                            } else {
                                filtered.push(data[i]);
                                i += 1;
                            }
                        }

                        if !response.is_empty() {
                            if let Ok(mut w) = writer.lock() {
                                let _ = w.write_all(&response);
                                let _ = w.flush();
                            }
                        }

                    let mut parser = parser.lock().unwrap_or_else(|e| e.into_inner());
                    parser.process(&filtered);
                    drop(parser);
                    ctx.request_repaint();
                    }
                    Err(_) => break,
                }
            }
        });
    }

    pub fn toggle(&mut self, ctx: &egui::Context) {
        self.visible = !self.visible;
        if self.visible {
            self.spawn(ctx);
        }
    }

    pub fn add_session(&mut self, ctx: &egui::Context) {
        self.sessions.push(TerminalInstance::default());
        self.active = self.sessions.len() - 1;
        self.spawn(ctx);
    }

    fn active(&mut self) -> &mut TerminalInstance {
        let idx = self.active.min(self.sessions.len().saturating_sub(1));
        &mut self.sessions[idx]
    }

    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette, ctx: &egui::Context) {
        if !self.visible || self.sessions.is_empty() {
            return;
        }

        // Tab strip for the multi-terminal sessions.
        let mut close: Option<usize> = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            for i in 0..self.sessions.len() {
                let is_active = i == self.active;
                let label = format!("Terminal {}", i + 1);
                let color = if is_active { palette.text } else { palette.text_muted };
                let fill = if is_active {
                    palette.panel_active
                } else {
                    egui::Color32::TRANSPARENT
                };
                let tab_frame = egui::Frame::NONE.fill(fill).corner_radius(0.0);
                let response = tab_frame
                    .show(ui, |ui| {
                        ui.add(egui::Button::new(RichText::new(&label).color(color)))
                    })
                    .inner
                    .on_hover_text("Switch terminal");
                if response.clicked() {
                    self.active = i;
                    self.spawn(ctx);
                }
                if response.middle_clicked() {
                    close = Some(i);
                }
                response.context_menu(|ui| {
                    if ui.button("Close").clicked() {
                        close = Some(i);
                        ui.close();
                    }
                });
                ui.add_space(2.0);
            }
            if ui.small_button("+").on_hover_text("New terminal").clicked() {
                self.add_session(ctx);
            }
        });
        ui.add_space(2.0);

        if let Some(i) = close {
            if self.sessions.len() > 1 {
                self.sessions.remove(i);
                if self.active >= self.sessions.len() {
                    self.active = self.sessions.len() - 1;
                }
            }
        }

        self.show_active(ui, palette);
    }

    fn show_active(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        if self.sessions.is_empty() {
            return;
        }

        let font_id = FontId::monospace(FONT_SIZE);
        let (glyph_w, row_h) = ui.fonts_mut(|f| {
            let w = f.glyph_width(&font_id, 'W');
            (w, f.row_height(&font_id))
        });

        // Reserve the full available space so we own a real, non-empty rect.
        let avail = ui.available_size();
        let avail = avail.max(egui::vec2(glyph_w * 10.0, row_h * 3.0));
        let (rect, response) = ui.allocate_exact_size(avail, Sense::click());

        // Also claim keyboard input when clicked/focused.
        if response.clicked() {
            self.focused = true;
            response.request_focus();
        }
        if response.has_focus() {
            self.focused = true;
        }

        // Capture field so we don't hold a borrow across the session borrow below.
        let focused = self.focused;
        let s = self.active();

        // Recompute grid dimensions from the real allocated rect.
        let new_cols = (rect.width() / glyph_w).floor().max(2.0) as u16;
        let new_rows = (rect.height() / row_h).floor().max(2.0) as u16;

        if new_cols != s.cols || new_rows != s.rows {
            s.cols = new_cols;
            s.rows = new_rows;
            if let Some(ref master) = s.master {
                let _ = master.resize(PtySize {
                    rows: s.rows,
                    cols: s.cols,
                    pixel_width: rect.width() as u16,
                    pixel_height: rect.height() as u16,
                });
            }
            {
                let mut parser = s.parser.lock().unwrap_or_else(|e| e.into_inner());
                parser.screen_mut().set_size(s.rows, s.cols);
            }
        }

        // Snapshot the whole screen once (no repeated locking during paint).
        let screen = {
            let parser = s.parser.lock().unwrap_or_else(|e| e.into_inner());
            parser.screen().clone()
        };
        let (cursor_row, cursor_col, screen_rows, screen_cols) = (
            screen.cursor_position().0,
            screen.cursor_position().1,
            screen.size().0,
            screen.size().1,
        );

        // Background.
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);

        if let Some(err) = &s.error {
            painter.text(
                rect.min + egui::vec2(8.0, 8.0),
                Align2::LEFT_TOP,
                err,
                FontId::proportional(FONT_SIZE),
                palette.text,
            );
            return;
        }

        let start_col = rect.min.x;
        let start_row = rect.min.y;

        // Draw each visible cell within the physical rect.
        for row in 0..s.rows {
            if row >= screen_rows {
                break;
            }
            let y = start_row + row as f32 * row_h;
            if y + row_h > rect.max.y + f32::EPSILON {
                break;
            }
            for col in 0..s.cols {
                if col >= screen_cols {
                    break;
                }
                let x = start_col + col as f32 * glyph_w;
                if x + glyph_w > rect.max.x + f32::EPSILON {
                    break;
                }
                let cell_rect =
                    Rect::from_min_size(Pos2::new(x, y), egui::vec2(glyph_w, row_h));

                let is_cursor = focused && row == cursor_row && col == cursor_col;

                let cell = match screen.cell(row as u16, col as u16) {
                    Some(c) => c,
                    None => continue,
                };

                // Background fill (respect cell bg except where it equals BG).
                let bg = if is_cursor {
                    palette.accent
                } else {
                    vt100_color_to_egui(cell.bgcolor(), palette, true)
                };
                if bg != BG {
                    painter.rect_filled(cell_rect, 0.0, bg);
                }

                let contents = cell.contents();
                if !contents.is_empty() && contents != " " {
                    let fg = if is_cursor {
                        Color32::from_rgb(12, 12, 12)
                    } else {
                        vt100_color_to_egui(cell.fgcolor(), palette, false)
                    };
                    painter.text(cell_rect.min, Align2::LEFT_TOP, contents, font_id.clone(), fg);
                }
            }
        }

        // Draw the cursor block when focused even if nothing is placed there yet.
        if focused && cursor_row < s.rows && cursor_col < s.cols {
            let cx = start_col + cursor_col as f32 * glyph_w;
            let cy = start_row + cursor_row as f32 * row_h;
            let cursor_rect = Rect::from_min_size(Pos2::new(cx, cy), egui::vec2(glyph_w, row_h));
            painter.rect_stroke(cursor_rect, 0.0, Stroke::new(1.5, palette.accent), StrokeKind::Inside);
        }

        if focused {
            ui.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => {
                            self.send_active(text.as_bytes());
                        }
                        egui::Event::Paste(text) => {
                            self.send_active(text.as_bytes());
                        }
                        egui::Event::Key {
                            key, pressed: true, ..
                        } => self.send_active_key(*key),
                        _ => {}
                    }
                }
            });
        }
    }

    fn send_active(&self, bytes: &[u8]) {
        let idx = self.active.min(self.sessions.len().saturating_sub(1));
        if let Ok(mut w) = self.sessions[idx].writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    fn send_active_key(&self, key: egui::Key) {
        let bytes: &[u8] = match key {
            egui::Key::Enter => b"\r",
            egui::Key::Backspace => b"\x7f",
            egui::Key::Delete => b"\x1b[3~",
            egui::Key::Tab => b"\t",
            egui::Key::Escape => b"\x1b",
            egui::Key::ArrowUp => b"\x1b[A",
            egui::Key::ArrowDown => b"\x1b[B",
            egui::Key::ArrowRight => b"\x1b[C",
            egui::Key::ArrowLeft => b"\x1b[D",
            egui::Key::Home => b"\x1b[H",
            egui::Key::End => b"\x1b[F",
            egui::Key::PageUp => b"\x1b[5~",
            egui::Key::PageDown => b"\x1b[6~",
            _ => return,
        };
        self.send_active(bytes);
    }
}

fn vt100_color_to_egui(color: vt100::Color, palette: &Palette, is_bg: bool) -> Color32 {
    match color {
        vt100::Color::Default => {
            if is_bg {
                Color32::from_rgb(12, 12, 12)
            } else {
                palette.text
            }
        }
        vt100::Color::Idx(idx) => ansi_idx_to_color(idx),
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

fn ansi_idx_to_color(idx: u8) -> Color32 {
    match idx {
        0 => Color32::from_rgb(0, 0, 0),
        1 => Color32::from_rgb(194, 54, 33),
        2 => Color32::from_rgb(37, 188, 36),
        3 => Color32::from_rgb(173, 173, 39),
        4 => Color32::from_rgb(73, 46, 225),
        5 => Color32::from_rgb(211, 56, 211),
        6 => Color32::from_rgb(51, 187, 200),
        7 => Color32::from_rgb(203, 204, 205),
        8 => Color32::from_rgb(129, 131, 131),
        9 => Color32::from_rgb(252, 57, 31),
        10 => Color32::from_rgb(49, 231, 34),
        11 => Color32::from_rgb(234, 236, 35),
        12 => Color32::from_rgb(88, 51, 255),
        13 => Color32::from_rgb(249, 53, 248),
        14 => Color32::from_rgb(20, 240, 240),
        15 => Color32::from_rgb(233, 235, 235),
        _ => Color32::from_rgb(203, 204, 205),
    }
}
