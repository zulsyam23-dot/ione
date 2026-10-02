use std::{borrow::Cow, collections::HashMap};

use eframe::egui::{self, Color32, Vec2, load::Bytes};

#[allow(dead_code)]
pub enum Icon {
    Document,
    FileText,
    FilePlus,
    Folder,
    FolderPlus,
    Save,
    SaveAs,
    Search,
    Close,
    Settings,
    Filter,
    Terminal,
    Help,
    Info,
    Refresh,
    Copy,
    Git,
    Code,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    ArrowLeft,
    ArrowRight,
    Book,
    LangRust,
    LangPython,
    LangLua,
    LangShell,
    LangSql,
    LangC,
    LangCss,
    LangHtml,
    LangJs,
    LangJson,
    LangToml,
    LangYaml,
    LangMarkdown,
    LangConfig,
    LangPlain,
}

impl Icon {
    fn svg(&self) -> &'static str {
        match self {
            Icon::Document => include_str!("../assets/icons/explorer/document.svg"),
            Icon::FileText => include_str!("../assets/icons/explorer/file-text.svg"),
            Icon::FilePlus => include_str!("../assets/icons/explorer/file-plus.svg"),
            Icon::Folder => include_str!("../assets/icons/explorer/folder.svg"),
            Icon::FolderPlus => include_str!("../assets/icons/explorer/folder-plus.svg"),
            Icon::Save => include_str!("../assets/icons/ui/save.svg"),
            Icon::SaveAs => include_str!("../assets/icons/ui/save-as.svg"),
            Icon::Search => include_str!("../assets/icons/ui/search.svg"),
            Icon::Close => include_str!("../assets/icons/ui/x.svg"),
            Icon::Settings => include_str!("../assets/icons/ui/settings.svg"),
            Icon::Filter => include_str!("../assets/icons/ui/filter.svg"),
            Icon::Terminal => include_str!("../assets/icons/ui/terminal.svg"),
            Icon::Help => include_str!("../assets/icons/ui/help.svg"),
            Icon::Info => include_str!("../assets/icons/ui/info.svg"),
            Icon::Refresh => include_str!("../assets/icons/ui/refresh.svg"),
            Icon::Copy => include_str!("../assets/icons/ui/copy.svg"),
            Icon::Git => include_str!("../assets/icons/ui/git.svg"),
            Icon::Code => include_str!("../assets/icons/ui/code.svg"),
            Icon::ChevronDown => include_str!("../assets/icons/ui/chevron-down.svg"),
            Icon::ChevronRight => include_str!("../assets/icons/ui/chevron-right.svg"),
            Icon::ChevronUp => include_str!("../assets/icons/ui/chevron-up.svg"),
            Icon::ArrowLeft => include_str!("../assets/icons/ui/arrow-left.svg"),
            Icon::ArrowRight => include_str!("../assets/icons/ui/arrow-right.svg"),
            Icon::Book => include_str!("../assets/icons/ui/book.svg"),
            Icon::LangRust => include_str!("../assets/icons/lang/lang-rust.svg"),
            Icon::LangPython => include_str!("../assets/icons/lang/lang-python.svg"),
            Icon::LangLua => include_str!("../assets/icons/lang/lang-lua.svg"),
            Icon::LangShell => include_str!("../assets/icons/lang/lang-shell.svg"),
            Icon::LangSql => include_str!("../assets/icons/lang/lang-sql.svg"),
            Icon::LangC => include_str!("../assets/icons/lang/lang-c.svg"),
            Icon::LangCss => include_str!("../assets/icons/lang/lang-css.svg"),
            Icon::LangHtml => include_str!("../assets/icons/lang/lang-html.svg"),
            Icon::LangJs => include_str!("../assets/icons/lang/lang-js.svg"),
            Icon::LangJson => include_str!("../assets/icons/lang/lang-json.svg"),
            Icon::LangToml => include_str!("../assets/icons/lang/lang-toml.svg"),
            Icon::LangYaml => include_str!("../assets/icons/lang/lang-yaml.svg"),
            Icon::LangMarkdown => include_str!("../assets/icons/lang/lang-markdown.svg"),
            Icon::LangConfig => include_str!("../assets/icons/lang/lang-config.svg"),
            Icon::LangPlain => include_str!("../assets/icons/lang/lang-plain.svg"),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Icon::Document => "document",
            Icon::FileText => "file-text",
            Icon::FilePlus => "file-plus",
            Icon::Folder => "folder",
            Icon::FolderPlus => "folder-plus",
            Icon::Save => "save",
            Icon::SaveAs => "save-as",
            Icon::Search => "search",
            Icon::Close => "x",
            Icon::Settings => "settings",
            Icon::Filter => "filter",
            Icon::Terminal => "terminal",
            Icon::Help => "help",
            Icon::Info => "info",
            Icon::Refresh => "refresh",
            Icon::Copy => "copy",
            Icon::Git => "git",
            Icon::Code => "code",
            Icon::ChevronDown => "chevron-down",
            Icon::ChevronRight => "chevron-right",
            Icon::ChevronUp => "chevron-up",
            Icon::ArrowLeft => "arrow-left",
            Icon::ArrowRight => "arrow-right",
            Icon::Book => "book",
            Icon::LangRust => "lang-rust",
            Icon::LangPython => "lang-python",
            Icon::LangLua => "lang-lua",
            Icon::LangShell => "lang-shell",
            Icon::LangSql => "lang-sql",
            Icon::LangC => "lang-c",
            Icon::LangCss => "lang-css",
            Icon::LangHtml => "lang-html",
            Icon::LangJs => "lang-js",
            Icon::LangJson => "lang-json",
            Icon::LangToml => "lang-toml",
            Icon::LangYaml => "lang-yaml",
            Icon::LangMarkdown => "lang-markdown",
            Icon::LangConfig => "lang-config",
            Icon::LangPlain => "lang-plain",
        }
    }
}

/// Caches the tinted SVG of each icon. The cache is dropped when the icon color
/// changes; the previous entries then linger in egui's loader cache, bounded by
/// the number of palettes the user cycles through.
pub struct Icons {
    color: Color32,
    cache: HashMap<&'static str, egui::ImageSource<'static>>,
}

impl Icons {
    pub fn new() -> Self {
        Self {
            color: Color32::GRAY,
            cache: HashMap::new(),
        }
    }

    /// Switch the icon color (e.g. on theme change) and drop cached sources.
    pub fn set_color(&mut self, color: Color32) {
        if color != self.color {
            self.color = color;
            self.cache.clear();
        }
    }

    /// The icon as an image source with `currentColor` resolved to the palette
    /// color.
    ///
    /// The bytes are handed to egui instead of being rasterized here on purpose:
    /// egui renders an SVG at exactly the pixel size it is painted at and caches
    /// one texture per (uri, size), so a downscaled texture never has to be
    /// minified by the GPU. Rasterizing at a fixed larger size instead leaves
    /// thin strokes subject to bilinear minification without mipmaps, which
    /// breaks diagonal lines into dashes.
    pub fn source(&mut self, icon: Icon) -> egui::ImageSource<'static> {
        let name = icon.name();
        if let Some(src) = self.cache.get(name) {
            return src.clone();
        }
        let svg = icon.svg().replace("currentColor", &hex(self.color));
        let src = egui::ImageSource::Bytes {
            // The `.svg` suffix is required: both egui and egui_extras only feed
            // SVGs to their loaders, and only those are cached per size.
            uri: Cow::Owned(format!("bytes://ione/{name}-{}.svg", hex(self.color))),
            bytes: Bytes::from(svg.into_bytes()),
        };
        self.cache.insert(name, src.clone());
        src
    }

    /// Render an icon as a clickable button with a hover tooltip.
    pub fn image_button(
        &mut self,
        ui: &mut egui::Ui,
        icon: Icon,
        size: f32,
        tooltip: &str,
    ) -> egui::Response {
        let src = self.source(icon);
        ui.add(
            egui::Image::new(src)
                .fit_to_exact_size(Vec2::splat(size))
                .sense(egui::Sense::click()),
        )
        .on_hover_text(tooltip)
    }
}

fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::load::{ImagePoll, SizeHint, TexturePoll};

    /// Icons must reach egui as SVG bytes, so that egui rasterizes them per
    /// painted size, and that raster must come out 1:1 with the painted pixels.
    ///
    /// Rasterizing once at a larger size and letting the GPU minify the texture
    /// (bilinear, no mipmaps) is what tore thin strokes into dashes.
    #[test]
    fn icons_rasterize_at_the_painted_pixel_size() {
        for ppp in [1.0_f32, 1.5, 2.0] {
            for size in [12.0_f32, 16.0] {
                let ctx = egui::Context::default();
                ctx.set_fonts(egui::FontDefinitions::empty());
                ctx.set_pixels_per_point(ppp);
                egui_extras::install_image_loaders(&ctx);

                let mut icons = Icons::new();
                icons.set_color(Color32::WHITE);
                let src = icons.source(Icon::Git);
                let uri = match &src {
                    egui::ImageSource::Bytes { uri, .. } => uri.to_string(),
                    other => panic!("icons must load from SVG bytes, got {other:?}"),
                };
                // Both loaders reject anything else, and only SVGs get a
                // texture per size.
                assert!(uri.ends_with(".svg"), "uri must be an svg: {uri}");

                let want = (size * ppp).round() as u32;
                let hint = SizeHint::Size {
                    width: want,
                    height: want,
                    maintain_aspect_ratio: true,
                };
                // Registers the bytes with egui, then inspect what it rasterized.
                assert!(
                    matches!(
                        src.load(&ctx, egui::TextureOptions::LINEAR, hint),
                        Ok(TexturePoll::Ready { .. })
                    ),
                    "icon failed to load (ppp={ppp}, size={size})"
                );
                match ctx.try_load_image(&uri, hint) {
                    Ok(ImagePoll::Ready { image }) => assert_eq!(
                        image.size,
                        [want as usize, want as usize],
                        "raster must match the painted pixels (ppp={ppp}, size={size})"
                    ),
                    _ => panic!("icon image should be ready (ppp={ppp}, size={size})"),
                }
            }
        }
    }

    /// The palette color is baked into the SVG, so it also has to be part of the
    /// cache key or the wrong color keeps being painted.
    #[test]
    fn color_change_produces_a_new_source() {
        let mut icons = Icons::new();
        icons.set_color(Color32::WHITE);
        let white = icons.source(Icon::Git);
        icons.set_color(Color32::from_gray(64));
        let gray = icons.source(Icon::Git);

        let uri = |src: &egui::ImageSource<'static>| match src {
            egui::ImageSource::Bytes { uri, .. } => uri.to_string(),
            other => panic!("expected SVG bytes, got {other:?}"),
        };
        assert_ne!(uri(&white), uri(&gray));
        assert!(!uri(&gray).contains("#ffffff"), "{}", uri(&gray));
    }
}
