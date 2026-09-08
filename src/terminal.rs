use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
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
    sel_anchor: Option<(u16, u16)>,
    sel_active: Option<(u16, u16)>,
    scroll_offset: usize,
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
            sel_anchor: None,
            sel_active: None,
            scroll_offset: 0,
        }
    }
}

pub struct TerminalPanel {
    pub visible: bool,
    pub cwd: Option<PathBuf>,
    sessions: Vec<TerminalInstance>,
    pub active: usize,
    focused: bool,
    /// Read by the PTY reader thread: while the terminal is hidden it must not
    /// force repaints (a busy background shell would otherwise make the whole
    /// app redraw at full rate while the user is typing in the editor).
    repaint: Arc<AtomicBool>,
}

impl Default for TerminalPanel {
    fn default() -> Self {
        Self {
            visible: false,
            cwd: None,
            sessions: vec![TerminalInstance::default()],
            active: 0,
            focused: false,
            repaint: Arc::new(AtomicBool::new(false)),
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
        // Spawn inside the workspace root so the shell starts where the user works.
        let mut cmd = CommandBuilder::new("pwsh");
        cmd.arg("-NoProfile");
        if let Some(dir) = &self.cwd {
            cmd.cwd(dir);
        }
        let child = match pair
            .slave
            .spawn_command(cmd)
            .or_else(|_| {
                let mut fb = CommandBuilder::new("powershell");
                fb.arg("-NoProfile");
                if let Some(dir) = &self.cwd {
                    fb.cwd(dir);
                }
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
        let repaint = Arc::clone(&self.repaint);
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
                    if repaint.load(Ordering::Relaxed) {
                        ctx.request_repaint();
                    }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    pub fn toggle(&mut self, ctx: &egui::Context) {
        self.visible = !self.visible;
        self.repaint.store(self.visible, Ordering::SeqCst);
        if self.visible {
            self.spawn(ctx);
        }
    }

    pub fn add_session(&mut self, ctx: &egui::Context) {
        self.sessions.push(TerminalInstance::default());
        self.active = self.sessions.len() - 1;
        self.repaint.store(true, Ordering::SeqCst);
        self.spawn(ctx);
    }

    /// Copy the current selection as plain text, trailing spaces stripped per line.
    pub fn copy_selection(&self) -> Option<String> {
        let idx = self.active.min(self.sessions.len().saturating_sub(1));
        let s = &self.sessions[idx];
        let (anchor, active) = (s.sel_anchor?, s.sel_active?);
        let screen = {
            let parser = s.parser.lock().unwrap_or_else(|e| e.into_inner());
            parser.screen().clone()
        };
        let (r1, c1) = (anchor.0.min(active.0), anchor.1.min(active.1));
        let (r2, c2) = (anchor.0.max(active.0), anchor.1.max(active.1));
        let mut lines = Vec::new();
        for row in r1..=r2 {
            let mut line = String::new();
            for col in c1..=c2 {
                if let Some(cell) = screen.cell(row, col) {
                    line.push_str(cell.contents());
                }
            }
            lines.push(line.trim_end().to_string());
        }
        Some(lines.join("\n"))
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
            s.scroll_offset = 0;
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

        let scrolled = s.scroll_offset > 0;

        // Snapshot the whole screen once (no repeated locking during paint).
        let screen = {
            let mut parser = s.parser.lock().unwrap_or_else(|e| e.into_inner());
            parser.screen_mut().set_scrollback(s.scroll_offset);
            parser.screen().clone()
        };
        let (cursor_row, cursor_col, screen_rows, screen_cols) = (
            screen.cursor_position().0,
            screen.cursor_position().1,
            screen.size().0,
            screen.size().1,
        );
        let mouse_mode = screen.mouse_protocol_mode();

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

        let sel_allowed = mouse_mode == vt100::MouseProtocolMode::None;
        let sel = (s.sel_anchor, s.sel_active);

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

                let is_cursor = focused && !scrolled && row == cursor_row && col == cursor_col;
                let selected = sel_allowed && in_selection(sel, row as u16, col as u16);

                let cell = match screen.cell(row as u16, col as u16) {
                    Some(c) => c,
                    None => continue,
                };

                // Background fill (respect cell bg except where it equals BG).
                let bg = if is_cursor {
                    palette.accent
                } else if selected {
                    vt100_color_to_egui(cell.fgcolor(), palette, false)
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
                    } else if selected {
                        vt100_color_to_egui(cell.bgcolor(), palette, true)
                    } else {
                        vt100_color_to_egui(cell.fgcolor(), palette, false)
                    };
                    painter.text(cell_rect.min, Align2::LEFT_TOP, contents, font_id.clone(), fg);
                }
            }
        }

        // Draw the cursor block when focused even if nothing is placed there yet.
        if focused && !scrolled && cursor_row < s.rows && cursor_col < s.cols {
            let cx = start_col + cursor_col as f32 * glyph_w;
            let cy = start_row + cursor_row as f32 * row_h;
            let cursor_rect = Rect::from_min_size(Pos2::new(cx, cy), egui::vec2(glyph_w, row_h));
            painter.rect_stroke(cursor_rect, 0.0, Stroke::new(1.5, palette.accent), StrokeKind::Inside);
        }

        let cols = s.cols;
        let rows = s.rows;

        if focused {
            let (clear_sel, snap_bottom) = ui.input(|i| {
                let mut clear = false;
                let mut snap = false;
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) => {
                            snap = true;
                            self.send_active(text.as_bytes());
                        }
                        egui::Event::Paste(text) => {
                            snap = true;
                            self.send_active(text.as_bytes());
                        }
                        egui::Event::Key {
                            key, pressed: true, ..
                        } => {
                            snap = true;
                            self.send_active_key(*key);
                        }
                        egui::Event::PointerButton {
                            pos, button, pressed: true, ..
                        } if mouse_mode != vt100::MouseProtocolMode::None && rect.contains(*pos) => {
                            clear = true;
                            let (col, row) = pos_to_cell(*pos, rect, glyph_w, row_h, cols, rows);
                            self.send_active(&format!(
                                "\x1b[<{};{};{}M",
                                mouse_button_code(*button),
                                col + 1,
                                row + 1
                            )
                            .into_bytes());
                        }
                        egui::Event::PointerButton {
                            pos, button, pressed: false, ..
                        } if mouse_mode != vt100::MouseProtocolMode::None
                            && mouse_mode != vt100::MouseProtocolMode::Press
                            && rect.contains(*pos) =>
                        {
                            let (col, row) = pos_to_cell(*pos, rect, glyph_w, row_h, cols, rows);
                            self.send_active(&format!(
                                "\x1b[<{};{};{}m",
                                mouse_button_code(*button),
                                col + 1,
                                row + 1
                            )
                            .into_bytes());
                        }
                        egui::Event::PointerMoved(pos) => {
                            let motion =
                                (mouse_mode == vt100::MouseProtocolMode::ButtonMotion
                                    && i.pointer.primary_down())
                                    || mouse_mode == vt100::MouseProtocolMode::AnyMotion;
                            if motion {
                                let (col, row) =
                                    pos_to_cell(*pos, rect, glyph_w, row_h, cols, rows);
                                let btn = if i.pointer.primary_down() { 32 } else { 35 };
                                self.send_active(
                                    &format!("\x1b[<{btn};{};{}M", col + 1, row + 1).into_bytes(),
                                );
                            }
                        }
                        egui::Event::MouseWheel { delta, .. }
                            if mouse_mode != vt100::MouseProtocolMode::None
                                && i
                                    .pointer
                                    .latest_pos()
                                    .map(|p| rect.contains(p))
                                    .unwrap_or(false) =>
                        {
                            if let Some(pos) = i.pointer.latest_pos() {
                                let (col, row) =
                                    pos_to_cell(pos, rect, glyph_w, row_h, cols, rows);
                                let btn = if delta.y > 0.0 { 64 } else { 65 };
                                self.send_active(
                                    &format!("\x1b[<{btn};{};{}M", col + 1, row + 1).into_bytes(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                (clear, snap)
            });
            if clear_sel {
                let s = self.active();
                s.sel_anchor = None;
                s.sel_active = None;
            }
            if snap_bottom {
                let s = self.active();
                s.scroll_offset = 0;
            }
        }

        // Text selection: egui owns the pointer when the app inside doesn't
        // want mouse mode.
        if sel_allowed {
            ui.input(|i| {
                let s = self.active();
                for event in &i.events {
                    match event {
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            ..
                        } if rect.contains(*pos) => {
                            s.scroll_offset = 0;
                            let (col, row) = pos_to_cell(*pos, rect, glyph_w, row_h, cols, rows);
                            s.sel_anchor = Some((row, col));
                            s.sel_active = Some((row, col));
                        }
                        egui::Event::PointerMoved(pos) if i.pointer.primary_down() => {
                            let (col, row) = pos_to_cell(*pos, rect, glyph_w, row_h, cols, rows);
                            s.sel_active = Some((row, col));
                        }
                        _ => {}
                    }
                }
            });
        }

        // Scroll the scrollback wheel over the terminal; egui owns the wheel
        // when the app doesn't want mouse mode. Scrolls back up (y>0) and
        // down to the live prompt (y<0).
        if sel_allowed {
            ui.input(|i| {
                let mut delta_y = 0.0;
                for event in &i.events {
                    if let egui::Event::MouseWheel { delta, .. } = event {
                        if delta.y != 0.0
                            && i
                                .pointer
                                .latest_pos()
                                .map(|p| rect.contains(p))
                                .unwrap_or(false)
                        {
                            delta_y += delta.y;
                        }
                    }
                }
                if delta_y != 0.0 {
                    let s = self.active();
                    let step = delta_y.signum() as isize;
                    s.scroll_offset = (s.scroll_offset as isize + step * 3).max(0) as usize;
                }
            });
        }

        // Middle-click pastes the OS clipboard into the active session.
        let middle_paste = ui.input(|i| {
            i.events.iter().any(|e| match e {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Middle,
                    pressed: false,
                    ..
                } => rect.contains(*pos),
                _ => false,
            })
        });
        if middle_paste {
            self.focused = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestPaste);
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

fn pos_to_cell(pos: Pos2, rect: Rect, glyph_w: f32, row_h: f32, cols: u16, rows: u16) -> (u16, u16) {
    let col = ((pos.x - rect.min.x) / glyph_w).floor().clamp(0.0, (cols - 1) as f32) as u16;
    let row = ((pos.y - rect.min.y) / row_h).floor().clamp(0.0, (rows - 1) as f32) as u16;
    (col, row)
}

fn mouse_button_code(button: egui::PointerButton) -> u8 {
    match button {
        egui::PointerButton::Primary => 0,
        egui::PointerButton::Middle => 1,
        _ => 2,
    }
}

fn in_selection(sel: (Option<(u16, u16)>, Option<(u16, u16)>), row: u16, col: u16) -> bool {
    let (a, b) = match sel {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    let (r1, r2) = (a.0.min(b.0), a.0.max(b.0));
    let (c1, c2) = (a.1.min(b.1), a.1.max(b.1));
    row >= r1 && row <= r2 && col >= c1 && col <= c2
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_mapping_clamps_to_grid() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(80.0, 40.0));
        assert_eq!(
            pos_to_cell(Pos2::new(50.0, 40.0), rect, 8.0, 10.0, 80, 24),
            (5, 2)
        );
        assert_eq!(
            pos_to_cell(Pos2::new(9999.0, 9999.0), rect, 8.0, 10.0, 80, 24),
            (79, 23)
        );
        assert_eq!(
            pos_to_cell(Pos2::new(0.0, 0.0), rect, 8.0, 10.0, 80, 24),
            (0, 0)
        );
        assert_eq!(mouse_button_code(egui::PointerButton::Primary), 0);
        assert_eq!(mouse_button_code(egui::PointerButton::Middle), 1);
        assert_eq!(mouse_button_code(egui::PointerButton::Secondary), 2);
        assert!(in_selection((Some((1, 2)), Some((3, 4))), 2, 3));
        assert!(!in_selection((Some((1, 2)), Some((3, 4))), 4, 1));
        assert!(!in_selection((None, Some((3, 4))), 2, 3));
    }
}
