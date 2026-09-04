use std::path::PathBuf;

use eframe::egui;

pub const FONTS: &[(&str, &str)] = &[
    ("JetBrains Mono", "JetBrainsMono-Regular.ttf"),
    ("Fira Code", "FiraCode-Regular.ttf"),
    ("Source Code Pro", "SourceCodePro-Regular.ttf"),
    ("IBM Plex Mono", "IBMPlexMono-Regular.ttf"),
    ("Roboto Mono", "RobotoMono-Regular.ttf"),
];

fn font_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("fonts")
}

pub fn font_filename(name: &str) -> Option<&'static str> {
    FONTS.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

/// Load a coding font from assets/fonts and make it the primary Monospace
/// family so both the editor canvas and the terminal pick it up.
pub fn apply_font(ctx: &egui::Context, name: &str) -> Result<(), String> {
    let filename = font_filename(name).ok_or_else(|| format!("Unknown font: {name}"))?;
    let path = font_dir().join(filename);
    let bytes = std::fs::read(&path).map_err(|_| {
        format!("Font file not found:\n{}\n\nPlace the TTF file there.", path.display())
    })?;
    if !is_font_file(&bytes) {
        return Err(format!(
            "Not a valid font file:\n{}\n\nFile is corrupted or missing.", path.display()
        ));
    }

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "ione_code_font".to_owned(),
        egui::FontData::from_owned(bytes).into(),
    );

    let family = egui::FontFamily::Monospace;
    if let Some(list) = fonts.families.get_mut(&family) {
        // Prepend but keep the default fallbacks for missing glyphs.
        list.retain(|f| f != "ione_code_font");
        list.insert(0, "ione_code_font".to_owned());
    }
    ctx.set_fonts(fonts);
    Ok(())
}

/// Validate the file header: TrueType (`\0\1\0\0` / `true`), OpenType (`OTTO`),
/// or a TrueType Collection (`ttcf`). Covers TTF/OTF and common container forms.
fn is_font_file(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let (a, b, c, d) = (bytes[0], bytes[1], bytes[2], bytes[3]);
    (a == 0 && b == 1 && c == 0 && d == 0)
        || (a == b't' && b == b'r' && c == b'u' && d == b'e')
        || (a == b'O' && b == b'T' && c == b'T' && d == b'O')
        || (a == b't' && b == b't' && c == b'c' && d == b'f')
}
