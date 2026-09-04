use std::collections::HashMap;

use eframe::egui::{self, Color32, Vec2};

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
            Icon::Document => include_str!("../assets/icons/document.svg"),
            Icon::FileText => include_str!("../assets/icons/file-text.svg"),
            Icon::FilePlus => include_str!("../assets/icons/file-plus.svg"),
            Icon::Folder => include_str!("../assets/icons/folder.svg"),
            Icon::FolderPlus => include_str!("../assets/icons/folder-plus.svg"),
            Icon::Save => include_str!("../assets/icons/save.svg"),
            Icon::SaveAs => include_str!("../assets/icons/save-as.svg"),
            Icon::Search => include_str!("../assets/icons/search.svg"),
            Icon::Close => include_str!("../assets/icons/x.svg"),
            Icon::Settings => include_str!("../assets/icons/settings.svg"),
            Icon::Filter => include_str!("../assets/icons/filter.svg"),
            Icon::Terminal => include_str!("../assets/icons/terminal.svg"),
            Icon::Help => include_str!("../assets/icons/help.svg"),
            Icon::Info => include_str!("../assets/icons/info.svg"),
            Icon::Refresh => include_str!("../assets/icons/refresh.svg"),
            Icon::Copy => include_str!("../assets/icons/copy.svg"),
            Icon::Code => include_str!("../assets/icons/code.svg"),
            Icon::ChevronDown => include_str!("../assets/icons/chevron-down.svg"),
            Icon::ChevronRight => include_str!("../assets/icons/chevron-right.svg"),
            Icon::ChevronUp => include_str!("../assets/icons/chevron-up.svg"),
            Icon::ArrowLeft => include_str!("../assets/icons/arrow-left.svg"),
            Icon::ArrowRight => include_str!("../assets/icons/arrow-right.svg"),
            Icon::Book => include_str!("../assets/icons/book.svg"),
            Icon::LangRust => include_str!("../assets/icons/lang-rust.svg"),
            Icon::LangPython => include_str!("../assets/icons/lang-python.svg"),
            Icon::LangLua => include_str!("../assets/icons/lang-lua.svg"),
            Icon::LangShell => include_str!("../assets/icons/lang-shell.svg"),
            Icon::LangSql => include_str!("../assets/icons/lang-sql.svg"),
            Icon::LangC => include_str!("../assets/icons/lang-c.svg"),
            Icon::LangCss => include_str!("../assets/icons/lang-css.svg"),
            Icon::LangHtml => include_str!("../assets/icons/lang-html.svg"),
            Icon::LangJs => include_str!("../assets/icons/lang-js.svg"),
            Icon::LangJson => include_str!("../assets/icons/lang-json.svg"),
            Icon::LangToml => include_str!("../assets/icons/lang-toml.svg"),
            Icon::LangYaml => include_str!("../assets/icons/lang-yaml.svg"),
            Icon::LangMarkdown => include_str!("../assets/icons/lang-markdown.svg"),
            Icon::LangConfig => include_str!("../assets/icons/lang-config.svg"),
            Icon::LangPlain => include_str!("../assets/icons/lang-plain.svg"),
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

/// Caches rasterized SVG icons on demand. Clears when the icon color changes.
pub struct Icons {
    color: Color32,
    cache: HashMap<&'static str, egui::TextureHandle>,
}

impl Icons {
    pub fn new() -> Self {
        Self {
            color: Color32::GRAY,
            cache: HashMap::new(),
        }
    }

    /// Switch the icon color (e.g. on theme change) and drop cached textures.
    pub fn set_color(&mut self, color: Color32) {
        if color != self.color {
            self.color = color;
            self.cache.clear();
        }
    }

    fn rasterize(&self, icon: Icon) -> egui::ColorImage {
        let svg = icon.svg().replace("currentColor", &hex(self.color));
        let options = resvg::usvg::Options::default();
        let hint = egui::load::SizeHint::Size {
            width: 128,
            height: 128,
            maintain_aspect_ratio: true,
        };
        egui_extras::image::load_svg_bytes_with_size(svg.as_bytes(), hint, &options).unwrap_or_else(
            |_| egui::ColorImage::filled([128, 128], self.color),
        )
    }

    /// Return the texture for an icon, rasterized at natural size (24px).
    pub fn texture(&mut self, ctx: &egui::Context, icon: Icon) -> egui::TextureHandle {
        let name = icon.name();
        if let Some(tex) = self.cache.get(name) {
            return tex.clone();
        }
        let image = self.rasterize(icon);
        let tex = ctx.load_texture(format!("icon_{name}"), image, egui::TextureOptions::LINEAR);
        self.cache.insert(name, tex.clone());
        tex
    }

    /// Render an icon as a clickable button with a hover tooltip.
    pub fn image_button(
        &mut self,
        ui: &mut egui::Ui,
        icon: Icon,
        size: f32,
        tooltip: &str,
    ) -> egui::Response {
        let tex = self.texture(ui.ctx(), icon);
        let st = egui::load::SizedTexture::new(tex.id(), tex.size_vec2());
        ui.add(
            egui::Image::new(egui::ImageSource::Texture(st))
                .fit_to_exact_size(Vec2::splat(size))
                .sense(egui::Sense::click()),
        )
        .on_hover_text(tooltip)
    }
}

fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}
