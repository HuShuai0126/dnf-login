use anyhow::Result;
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::{
    config::{AppConfig, BgFillMode},
    game::DnfLauncher,
    i18n::{Language, translations},
    network::{DnfClient, LoginResponse, RegisterResponse, SimpleResponse, md5_hash},
    platform,
    storage::CredentialStorage,
};

const BG_IMAGES: &[(&str, &[u8])] = &[
    ("bg1", include_bytes!("../assets/bg1.jpg")),
    ("bg2", include_bytes!("../assets/bg2.jpg")),
    ("bg3", include_bytes!("../assets/bg3.jpg")),
    ("bg4", include_bytes!("../assets/bg4.jpg")),
    ("bg5", include_bytes!("../assets/bg5.jpg")),
    ("bg6", include_bytes!("../assets/bg6.jpg")),
    ("bg7", include_bytes!("../assets/bg7.jpg")),
    ("bg8", include_bytes!("../assets/bg8.jpg")),
    ("bg9", include_bytes!("../assets/bg9.jpg")),
    ("bg10", include_bytes!("../assets/bg10.jpg")),
    ("bg11", include_bytes!("../assets/bg11.jpg")),
    ("bg12", include_bytes!("../assets/bg12.jpg")),
    ("bg13", include_bytes!("../assets/bg13.jpg")),
    ("bg14", include_bytes!("../assets/bg14.jpg")),
    ("bg15", include_bytes!("../assets/bg15.jpg")),
    ("bg16", include_bytes!("../assets/bg16.jpg")),
    ("bg17", include_bytes!("../assets/bg17.jpg")),
    ("bg18", include_bytes!("../assets/bg18.jpg")),
];

const THUMB_W: u32 = 64;
const THUMB_H: u32 = 36;

/// Decoded pixel data for a single background, produced by worker threads and
/// consumed on the main thread to upload GPU textures.
struct BgImageData {
    index: usize,
    full_image: egui::ColorImage,
    thumb_image: egui::ColorImage,
}

#[derive(PartialEq, Clone, Copy)]
enum AppState {
    Login,
    Register,
    ChangePassword,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TaskType {
    Login,
    Register,
    ChangePassword,
}

enum TaskResult {
    Login(Result<LoginResponse>),
    Register(Result<RegisterResponse>),
    ChangePassword(Result<SimpleResponse>),
}

pub struct DnfLoginApp {
    config: AppConfig,
    client: Option<DnfClient>,
    storage: CredentialStorage,
    runtime: tokio::runtime::Runtime,
    task_rx: Receiver<TaskResult>,
    task_tx: Sender<TaskResult>,

    /// Translation table for the active language; refreshed on language change.
    tr: crate::i18n::Tr,
    /// Key string for the saved-config display; refreshed when settings are saved.
    cached_key_preview: String,
    /// Application icon texture decoded from the embedded ICO file.
    app_icon: Option<egui::TextureHandle>,

    /// Full-resolution background textures scaled to the window size.
    bgs: Vec<Option<egui::TextureHandle>>,
    /// Thumbnail textures for the background switcher strip.
    bg_thumbs: Vec<Option<egui::TextureHandle>>,
    /// Index of the currently displayed background.
    current_bg: usize,
    /// Horizontal scroll offset (pixels) of the thumbnail strip.
    thumb_scroll_offset: f32,
    /// Receives decoded background images from worker threads for GPU upload.
    img_rx: Receiver<BgImageData>,
    /// Set to true after the first frame triggers background loading.
    bg_loading_started: bool,

    state: AppState,
    current_task: Option<TaskType>,

    settings_server_url: String,
    settings_aes_key: String,
    settings_bg_path: String,
    settings_plugins_dir: String,

    username: String,
    password: String,
    remember_password: bool,

    register_username: String,
    register_password: String,
    register_password_confirm: String,
    register_qq: String,

    changepwd_username: String,
    changepwd_old_password: String,
    changepwd_new_password: String,
    changepwd_confirm: String,

    message: Option<String>,
    message_is_error: bool,

    logged_in_user: Option<String>,
    login_token: Option<String>,
}

// Color palette
impl DnfLoginApp {
    fn c_bg() -> egui::Color32 {
        egui::Color32::from_rgb(10, 10, 10)
    }
    fn c_card() -> egui::Color32 {
        egui::Color32::from_rgb(22, 22, 24)
    }
    fn c_input_bg() -> egui::Color32 {
        egui::Color32::from_rgb(14, 14, 16)
    }
    fn c_border() -> egui::Color32 {
        egui::Color32::from_rgb(52, 52, 58)
    }
    fn c_border_dim() -> egui::Color32 {
        egui::Color32::from_rgb(36, 36, 42)
    }
    fn c_accent() -> egui::Color32 {
        egui::Color32::from_rgb(59, 130, 246)
    }
    fn c_accent_hover() -> egui::Color32 {
        egui::Color32::from_rgb(96, 165, 250)
    }
    fn c_accent_press() -> egui::Color32 {
        egui::Color32::from_rgb(37, 99, 235)
    }
    fn c_accent_faint() -> egui::Color32 {
        egui::Color32::from_rgb(10, 28, 65)
    }
    fn c_text() -> egui::Color32 {
        egui::Color32::from_rgb(238, 240, 250)
    }
    fn c_text2() -> egui::Color32 {
        egui::Color32::from_rgb(205, 212, 232)
    }
    fn c_text3() -> egui::Color32 {
        egui::Color32::from_rgb(155, 166, 192)
    }
    fn c_success() -> egui::Color32 {
        egui::Color32::from_rgb(100, 170, 250)
    }
    fn c_error() -> egui::Color32 {
        egui::Color32::from_rgb(220, 110, 140)
    }
    fn c_success_bg() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(6, 18, 45, 215)
    }
    fn c_error_bg() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(38, 8, 20, 215)
    }
    fn c_warn_bg() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(20, 12, 48, 220)
    }
    fn c_warn_border() -> egui::Color32 {
        egui::Color32::from_rgb(100, 70, 180)
    }
    fn c_warn_text() -> egui::Color32 {
        egui::Color32::from_rgb(180, 155, 235)
    }
    fn c_glass_fill() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(6, 9, 22, 218)
    }
    fn c_glass_border() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(18, 28, 70, 55)
    }
    fn c_overlay() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(0, 0, 6, 75)
    }
    fn c_thumb_active() -> egui::Color32 {
        egui::Color32::from_rgb(96, 165, 250)
    }
    fn c_thumb_inactive() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(30, 40, 80, 90)
    }
    fn c_thumb_hover() -> egui::Color32 {
        egui::Color32::from_rgba_premultiplied(60, 90, 160, 120)
    }
}

// Setup
impl DnfLoginApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::try_load_cjk_font(&cc.egui_ctx);

        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(14.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(14.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(22.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(11.5, egui::FontFamily::Proportional),
        );
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        style.spacing.button_padding = egui::vec2(14.0, 6.0);
        style.spacing.text_edit_width = 220.0;
        cc.egui_ctx.set_style(style);

        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);

        let mut v = egui::Visuals::dark();
        v.panel_fill = Self::c_bg();
        v.window_fill = Self::c_bg();
        v.extreme_bg_color = Self::c_input_bg();
        v.faint_bg_color = Self::c_card();

        v.widgets.noninteractive.bg_fill = Self::c_card();
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, Self::c_border());
        v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, Self::c_text2());

        v.widgets.inactive.bg_fill = Self::c_input_bg();
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Self::c_border_dim());
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, Self::c_text2());

        v.widgets.hovered.bg_fill = egui::Color32::from_rgb(26, 30, 40);
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, Self::c_accent());
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, Self::c_text());

        v.widgets.active.bg_fill = Self::c_accent_faint();
        v.widgets.active.bg_stroke = egui::Stroke::new(1.5, Self::c_accent());
        v.widgets.active.fg_stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);

        v.selection.bg_fill = Self::c_accent_faint();
        v.selection.stroke = egui::Stroke::new(1.0, Self::c_accent());

        v.hyperlink_color = Self::c_accent();
        v.text_cursor.stroke.color = Self::c_accent();
        // Explicitly target the dark theme style so the override survives system-theme events.
        cc.egui_ctx.set_visuals_of(egui::Theme::Dark, v);

        let config = AppConfig::load().unwrap_or_else(|e| {
            tracing::warn!("Failed to load config, using default: {}", e);
            AppConfig::default()
        });

        let client: Option<DnfClient> = if config.is_configured() {
            match config.get_aes_key_bytes() {
                Ok(key) => DnfClient::new(config.server_url.clone(), &key).ok(),
                Err(e) => {
                    tracing::error!("AES key parse error: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let storage = CredentialStorage::new().expect("Failed to create credential storage");
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        let (task_tx, task_rx) = channel();

        let (username, password, remember_password) = if storage.has_saved_credentials() {
            if let Ok((u, p)) = storage.load() {
                (u, p, true)
            } else {
                (String::new(), String::new(), false)
            }
        } else {
            (String::new(), String::new(), false)
        };

        let settings_server_url = config.server_url.clone();
        let settings_aes_key = config.aes_key.clone();
        let settings_bg_path = config.bg_custom_path.clone();
        let settings_plugins_dir = config.plugins_dir.clone();
        let tr = translations(config.language);
        let cached_key_preview = Self::make_key_preview(&config.aes_key);
        let app_icon = Self::load_app_icon(&cc.egui_ctx);

        let n = BG_IMAGES.len();
        let bgs = vec![None; n];
        let bg_thumbs = vec![None; n];
        // Sender is dropped immediately, leaving a disconnected receiver.
        // start_bg_loading() replaces it on the first update() frame.
        let (_, img_rx) = channel::<BgImageData>();

        Self {
            config,
            client,
            storage,
            runtime,
            task_tx,
            task_rx,
            tr,
            cached_key_preview,
            app_icon,
            bgs,
            bg_thumbs,
            current_bg: 0,
            thumb_scroll_offset: 0.0,
            img_rx,
            bg_loading_started: false,
            state: AppState::Login,
            current_task: None,
            settings_server_url,
            settings_aes_key,
            settings_bg_path,
            settings_plugins_dir,
            username,
            password,
            remember_password,
            register_username: String::new(),
            register_password: String::new(),
            register_password_confirm: String::new(),
            register_qq: String::new(),
            changepwd_username: String::new(),
            changepwd_old_password: String::new(),
            changepwd_new_password: String::new(),
            changepwd_confirm: String::new(),
            message: None,
            message_is_error: false,
            logged_in_user: None,
            login_token: None,
        }
    }

    /// Decodes the embedded ICO file and registers it as an egui texture.
    /// Returns `None` if the ICO data cannot be decoded.
    fn load_app_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
        const ICON_BYTES: &[u8] = include_bytes!("../resources/DNF.ico");
        let dir = ico::IconDir::read(std::io::Cursor::new(ICON_BYTES)).ok()?;
        let entry = dir
            .entries()
            .iter()
            .filter(|e| e.width() >= 32)
            .max_by_key(|e| e.width())
            .or_else(|| dir.entries().first())?;
        let image = entry.decode().ok()?;
        let w = image.width() as usize;
        let h = image.height() as usize;
        let rgba = image.rgba_data().to_vec();
        let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        Some(ctx.load_texture("app_icon", color_image, egui::TextureOptions::LINEAR))
    }

    /// Decodes one background JPEG and produces both the full-size and thumbnail variants,
    /// sharing the single decompressed image to avoid redundant decode work.
    ///
    /// Thumbnails use a two-stage resize: `thumbnail()` (O(output) box filter) pre-reduces to
    /// 4× the target size, then Lanczos3 runs on the small intermediate. This limits the kernel
    /// support width that Lanczos3 would otherwise expand at large reduction ratios.
    fn decode_bg_pair(
        bytes: &[u8],
        max_w: u32,
        max_h: u32,
        thumb_w: u32,
        thumb_h: u32,
    ) -> Option<(egui::ColorImage, egui::ColorImage)> {
        let img = image::load_from_memory(bytes).ok()?;

        // Preserve the original aspect ratio; the fill mode handles layout at render time.
        let scaled = img.thumbnail(max_w, max_h);
        let (sw, sh) = (scaled.width(), scaled.height());
        let bg_pixels = scaled.to_rgba8().into_raw();
        let full_image =
            egui::ColorImage::from_rgba_unmultiplied([sw as usize, sh as usize], &bg_pixels);

        let pre = img.thumbnail(thumb_w * 4, thumb_h * 4);
        let thumb_pixels = pre
            .resize_to_fill(thumb_w, thumb_h, image::imageops::FilterType::Lanczos3)
            .to_rgba8()
            .into_raw();
        let thumb_image = egui::ColorImage::from_rgba_unmultiplied(
            [thumb_w as usize, thumb_h as usize],
            &thumb_pixels,
        );

        Some((full_image, thumb_image))
    }

    /// Scans `dir` for JPG/JPEG files and returns their paths sorted by filename.
    /// Returns an empty list if the directory does not exist or cannot be read.
    fn scan_custom_bg_dir(dir: &str) -> Vec<std::path::PathBuf> {
        let path = std::path::Path::new(dir);
        if !path.is_dir() {
            return Vec::new();
        }
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg")
                    })
            })
            .collect();
        entries.sort();
        entries
    }

    /// Spawns background decode tasks for all images. Replacing `img_rx` disconnects
    /// the old channel so any stale in-flight tasks discard their results silently.
    ///
    /// A `Semaphore` limits concurrent CPU-bound tasks to `max(1, cpu_count − 1)`,
    /// leaving at least one core available for the render thread during loading.
    fn start_bg_loading(&mut self) {
        let custom_paths = Self::scan_custom_bg_dir(&self.config.bg_custom_path);
        let n_builtin = BG_IMAGES.len();
        let n_custom = custom_paths.len();
        let n_total = n_builtin + n_custom;

        self.bgs = vec![None; n_total];
        self.bg_thumbs = vec![None; n_total];
        if self.current_bg >= n_total {
            self.current_bg = 0;
        }

        let (img_tx, img_rx) = channel::<BgImageData>();
        self.img_rx = img_rx;

        let bg_w = 960u32;
        let bg_h = 540u32;
        let thumb_w = THUMB_W;
        let thumb_h = THUMB_H;
        let prepend = self.config.bg_custom_prepend;

        let parallelism = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        let max_jobs = parallelism.saturating_sub(1).max(1);
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(max_jobs));

        // Built-in images (embedded at compile time).
        for (i, (_, bytes)) in BG_IMAGES.iter().enumerate() {
            let final_index = if prepend { n_custom + i } else { i };
            let tx = img_tx.clone();
            let bytes: &'static [u8] = bytes;
            let sem = sem.clone();
            self.runtime.spawn(async move {
                let _permit = sem.acquire_owned().await.ok();
                let result = tokio::task::spawn_blocking(move || {
                    Self::decode_bg_pair(bytes, bg_w, bg_h, thumb_w, thumb_h).map(
                        |(full_image, thumb_image)| BgImageData {
                            index: final_index,
                            full_image,
                            thumb_image,
                        },
                    )
                })
                .await;
                if let Ok(Some(data)) = result {
                    let _ = tx.send(data);
                }
            });
        }

        // Custom images loaded from the filesystem at runtime.
        for (i, path) in custom_paths.into_iter().enumerate() {
            let final_index = if prepend { i } else { n_builtin + i };
            let tx = img_tx.clone();
            let sem = sem.clone();
            self.runtime.spawn(async move {
                let _permit = sem.acquire_owned().await.ok();
                let result = tokio::task::spawn_blocking(move || {
                    let bytes = std::fs::read(&path).ok()?;
                    Self::decode_bg_pair(&bytes, bg_w, bg_h, thumb_w, thumb_h).map(
                        |(full_image, thumb_image)| BgImageData {
                            index: final_index,
                            full_image,
                            thumb_image,
                        },
                    )
                })
                .await;
                if let Ok(Some(data)) = result {
                    let _ = tx.send(data);
                }
            });
        }
    }

    /// Loads a CJK fallback font from Windows system fonts, if available.
    fn try_load_cjk_font(ctx: &egui::Context) {
        let candidates = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyhbd.ttc",
            r"C:\Windows\Fonts\simsun.ttc",
        ];

        let mut fonts = egui::FontDefinitions::default();
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "cjk".to_owned(),
                    std::sync::Arc::new(egui::FontData::from_owned(data)),
                );
                if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    family.push("cjk".to_owned());
                }
                tracing::info!("CJK font loaded: {}", path);
                ctx.set_fonts(fonts);
                return;
            }
        }
    }
}

// App icon
impl DnfLoginApp {
    /// Draws the application icon. Falls back to a painted shape if no texture is loaded.
    fn draw_app_icon(ui: &mut egui::Ui, icon_texture: Option<&egui::TextureHandle>) {
        let size = 100.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }

        if let Some(tex) = icon_texture {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
            return;
        }

        let p = ui.painter();
        let c = rect.center();
        let acc = Self::c_accent();
        let hi = Self::c_accent_hover();

        p.rect_filled(
            rect,
            egui::CornerRadius::same(16),
            egui::Color32::from_rgb(10, 22, 52),
        );
        p.rect_stroke(
            rect.shrink(1.0),
            egui::CornerRadius::same(15),
            egui::Stroke::new(1.5, acc),
            egui::StrokeKind::Inside,
        );
        p.rect_filled(
            egui::Rect::from_center_size(c + egui::vec2(0.0, -10.0), egui::vec2(5.0, 20.0)),
            egui::CornerRadius {
                nw: 3,
                ne: 3,
                sw: 0,
                se: 0,
            },
            acc,
        );
        p.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(c.x - 2.5, c.y - 20.0),
                egui::pos2(c.x + 2.5, c.y - 20.0),
                egui::pos2(c.x, c.y - 30.0),
            ],
            acc,
            egui::Stroke::NONE,
        ));
        p.rect_filled(
            egui::Rect::from_center_size(c, egui::vec2(26.0, 4.5)),
            egui::CornerRadius::same(2),
            acc,
        );
        p.circle_filled(c + egui::vec2(-13.0, 0.0), 3.5, hi);
        p.circle_filled(c + egui::vec2(13.0, 0.0), 3.5, hi);
        p.rect_filled(
            egui::Rect::from_center_size(c + egui::vec2(0.0, 11.5), egui::vec2(4.0, 15.0)),
            egui::CornerRadius::same(2),
            hi,
        );
        p.circle_filled(c + egui::vec2(0.0, 20.5), 5.0, acc);
    }
}

// Background & thumbnail rendering
impl DnfLoginApp {
    /// Paints the current background image (or a solid fallback) plus a dim overlay.
    /// Rendering follows `self.config.bg_fill_mode`.
    fn paint_background(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let p = ui.painter();
        if let Some(Some(bg)) = self.bgs.get(self.current_bg) {
            let [tw, th] = bg.size();
            let tex_w = tw as f32;
            let tex_h = th as f32;
            let scr_w = rect.width();
            let scr_h = rect.height();
            let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

            match &self.config.bg_fill_mode {
                BgFillMode::Stretch => {
                    p.image(bg.id(), rect, full_uv, egui::Color32::WHITE);
                }
                BgFillMode::Fill => {
                    let img_ar = tex_w / tex_h;
                    let scr_ar = scr_w / scr_h;
                    let (u0, v0, u1, v1) = if img_ar > scr_ar {
                        // Wider than screen: crop the left/right sides.
                        let visible_w = tex_h * scr_ar;
                        let crop = (tex_w - visible_w) / 2.0;
                        (crop / tex_w, 0.0, (crop + visible_w) / tex_w, 1.0)
                    } else {
                        // Taller than screen: crop the top/bottom.
                        let visible_h = tex_w / scr_ar;
                        let crop = (tex_h - visible_h) / 2.0;
                        (0.0, crop / tex_h, 1.0, (crop + visible_h) / tex_h)
                    };
                    let uv = egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1));
                    p.image(bg.id(), rect, uv, egui::Color32::WHITE);
                }
                BgFillMode::Fit => {
                    p.rect_filled(rect, 0.0, Self::c_bg());
                    let img_ar = tex_w / tex_h;
                    let scr_ar = scr_w / scr_h;
                    let (dest_w, dest_h) = if img_ar > scr_ar {
                        (scr_w, scr_w / img_ar)
                    } else {
                        (scr_h * img_ar, scr_h)
                    };
                    let dest =
                        egui::Rect::from_center_size(rect.center(), egui::vec2(dest_w, dest_h));
                    p.image(bg.id(), dest, full_uv, egui::Color32::WHITE);
                }
                BgFillMode::Center => {
                    p.rect_filled(rect, 0.0, Self::c_bg());
                    let dest_w = tex_w.min(scr_w);
                    let dest_h = tex_h.min(scr_h);
                    let dest =
                        egui::Rect::from_center_size(rect.center(), egui::vec2(dest_w, dest_h));
                    // Crop UV when the image is larger than the screen.
                    let u0 = if tex_w > scr_w {
                        (tex_w - scr_w) / 2.0 / tex_w
                    } else {
                        0.0
                    };
                    let v0 = if tex_h > scr_h {
                        (tex_h - scr_h) / 2.0 / tex_h
                    } else {
                        0.0
                    };
                    let u1 = 1.0 - u0;
                    let v1 = 1.0 - v0;
                    let uv = egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1));
                    p.image(bg.id(), dest, uv, egui::Color32::WHITE);
                }
                BgFillMode::Tile => {
                    let mut y = rect.min.y;
                    while y < rect.max.y {
                        let tile_h = tex_h.min(rect.max.y - y);
                        let v1 = tile_h / tex_h;
                        let mut x = rect.min.x;
                        while x < rect.max.x {
                            let tile_w = tex_w.min(rect.max.x - x);
                            let u1 = tile_w / tex_w;
                            let tile_rect = egui::Rect::from_min_size(
                                egui::pos2(x, y),
                                egui::vec2(tile_w, tile_h),
                            );
                            let uv =
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(u1, v1));
                            p.image(bg.id(), tile_rect, uv, egui::Color32::WHITE);
                            x += tex_w;
                        }
                        y += tex_h;
                    }
                }
            }
        } else {
            p.rect_filled(rect, 0.0, Self::c_bg());
        }
        // Dark overlay to improve card legibility over background images.
        p.rect_filled(rect, 0.0, Self::c_overlay());
    }

    /// Draws the background thumbnail switcher strip along the bottom edge.
    /// Supports horizontal scroll when thumbnails exceed the available width.
    fn draw_thumbnail_strip(&mut self, ui: &mut egui::Ui, screen: egui::Rect) {
        let tw = THUMB_W as f32;
        let th = THUMB_H as f32;
        let gap = 7.0;
        let pad_x = 16.0;
        let pad_y = 12.0;
        // Space reserved at the right edge for the version label.
        let version_reserve = 58.0;
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let n = self.bg_thumbs.len();

        let strip_x = screen.min.x + pad_x;
        let strip_y = screen.max.y - th - pad_y;
        let strip_w = screen.width() - pad_x - version_reserve - pad_x;
        let total_w = n as f32 * (tw + gap) - gap;
        let max_scroll = (total_w - strip_w).max(0.0);

        let strip_sense = egui::Rect::from_min_max(
            egui::pos2(strip_x, strip_y - 4.0),
            egui::pos2(strip_x + strip_w, strip_y + th + 14.0),
        );
        let in_strip = ui.input(|i| {
            i.pointer
                .latest_pos()
                .is_some_and(|p| strip_sense.contains(p))
        });
        if in_strip {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            self.thumb_scroll_offset -= delta.x + delta.y;
            self.thumb_scroll_offset = self.thumb_scroll_offset.clamp(0.0, max_scroll);
        }

        let clip_rect = egui::Rect::from_min_max(
            egui::pos2(strip_x - 1.0, strip_y - 4.0),
            egui::pos2(strip_x + strip_w + 1.0, strip_y + th + 14.0),
        );
        let painter = ui.painter().with_clip_rect(clip_rect);

        for i in 0..n {
            let x = strip_x + i as f32 * (tw + gap) - self.thumb_scroll_offset;
            let r = egui::Rect::from_min_size(egui::pos2(x, strip_y), egui::vec2(tw, th));

            if r.max.x <= clip_rect.min.x || r.min.x >= clip_rect.max.x {
                continue;
            }

            // Use the clipped rect for interaction so partially visible thumbnails respond correctly.
            let visible = egui::Rect::from_min_max(
                egui::pos2(r.min.x.max(clip_rect.min.x), r.min.y),
                egui::pos2(r.max.x.min(clip_rect.max.x), r.max.y),
            );
            let resp = ui.allocate_rect(visible, egui::Sense::click());

            if let Some(Some(thumb)) = self.bg_thumbs.get(i) {
                painter.image(thumb.id(), r, uv, egui::Color32::WHITE);
            } else {
                painter.rect_filled(r, egui::CornerRadius::same(4), Self::c_card());
            }

            let is_active = i == self.current_bg;
            let border_color = if is_active {
                Self::c_thumb_active()
            } else if resp.hovered() {
                Self::c_thumb_hover()
            } else {
                Self::c_thumb_inactive()
            };
            let border_w = if is_active { 2.0 } else { 1.0 };
            painter.rect_stroke(
                r,
                egui::CornerRadius::same(4),
                egui::Stroke::new(border_w, border_color),
                egui::StrokeKind::Outside,
            );

            if is_active {
                painter.circle_filled(
                    egui::pos2(r.center().x, r.max.y + 5.0),
                    2.0,
                    Self::c_thumb_active(),
                );
            }

            if resp.clicked() {
                self.current_bg = i;
                // Scroll the selected thumbnail into view.
                let thumb_left = i as f32 * (tw + gap);
                let thumb_right = thumb_left + tw;
                if thumb_left < self.thumb_scroll_offset {
                    self.thumb_scroll_offset = thumb_left;
                } else if thumb_right > self.thumb_scroll_offset + strip_w {
                    self.thumb_scroll_offset = (thumb_right - strip_w).clamp(0.0, max_scroll);
                }
            }
        }

        // Scroll progress bar (only shown when content overflows).
        if max_scroll > 0.0 {
            let bar_y = strip_y + th + 9.0;
            let knob_w = (strip_w / total_w * strip_w).max(20.0);
            let knob_x = strip_x + (self.thumb_scroll_offset / max_scroll) * (strip_w - knob_w);
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(strip_x, bar_y), egui::vec2(strip_w, 2.0)),
                0.0,
                Self::c_thumb_inactive(),
            );
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(knob_x, bar_y), egui::vec2(knob_w, 2.0)),
                0.0,
                Self::c_thumb_active(),
            );
        }

        ui.painter().text(
            screen.max - egui::vec2(12.0, 8.0),
            egui::Align2::RIGHT_BOTTOM,
            concat!("v", env!("CARGO_PKG_VERSION")),
            egui::FontId::proportional(10.5),
            Self::c_text3(),
        );
    }
}

// Translations helper
impl DnfLoginApp {
    fn t(&self) -> crate::i18n::Tr {
        self.tr
    }

    /// Produces a partial preview of the AES key: first 24 chars, ellipsis, last 24 chars.
    /// The lengths are chosen to fill the available width of the saved-config display row.
    fn make_key_preview(key: &str) -> String {
        const HEAD: usize = 24;
        const TAIL: usize = 24;
        let chars: Vec<char> = key.chars().collect();
        if chars.len() > HEAD + TAIL {
            let head: String = chars[..HEAD].iter().collect();
            let tail: String = chars[chars.len() - TAIL..].iter().collect();
            format!("{}…{}", head, tail)
        } else {
            key.to_owned()
        }
    }
}

// UI components
impl DnfLoginApp {
    fn text_input<'a>(label: &str, value: &'a mut String, hint: &'a str) -> impl egui::Widget + 'a {
        let label = label.to_string();
        move |ui: &mut egui::Ui| {
            ui.label(
                egui::RichText::new(&label)
                    .size(12.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .font(egui::TextStyle::Body),
            )
        }
    }

    fn password_input<'a>(
        label: &str,
        value: &'a mut String,
        hint: &'a str,
    ) -> impl egui::Widget + 'a {
        let label = label.to_string();
        move |ui: &mut egui::Ui| {
            ui.label(
                egui::RichText::new(&label)
                    .size(12.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(value)
                    .password(true)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .font(egui::TextStyle::Body),
            )
        }
    }

    fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
        let h = 44.0;
        let w = ui.available_width();
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), sense);

        if ui.is_rect_visible(rect) {
            let fill = if !enabled {
                Self::c_border_dim()
            } else if response.is_pointer_button_down_on() {
                Self::c_accent_press()
            } else if response.hovered() {
                Self::c_accent_hover()
            } else {
                Self::c_accent()
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(8), fill);
            let text_col = if enabled {
                egui::Color32::WHITE
            } else {
                Self::c_text3()
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(14.5),
                text_col,
            );
        }
        response.clicked()
    }

    fn primary_button_slim(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
        let h = 36.0;
        let w = ui.available_width();
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), sense);

        if ui.is_rect_visible(rect) {
            let fill = if !enabled {
                Self::c_border_dim()
            } else if response.is_pointer_button_down_on() {
                Self::c_accent_press()
            } else if response.hovered() {
                Self::c_accent_hover()
            } else {
                Self::c_accent()
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), fill);
            let text_col = if enabled {
                egui::Color32::WHITE
            } else {
                Self::c_text3()
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(13.5),
                text_col,
            );
        }
        response.clicked()
    }

    fn secondary_button(label: &str) -> egui::Button<'static> {
        egui::Button::new(
            egui::RichText::new(label.to_string())
                .size(13.0)
                .color(Self::c_text2()),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, Self::c_border()))
        .corner_radius(egui::CornerRadius::same(6))
        .min_size(egui::vec2(0.0, 36.0))
    }

    fn warning_box(ui: &mut egui::Ui, text: &str) {
        egui::Frame::new()
            .fill(Self::c_warn_bg())
            .stroke(egui::Stroke::new(1.0, Self::c_warn_border()))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::vec2(12.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .size(12.0)
                            .color(Self::c_warn_text()),
                    );
                });
            });
    }

    fn status_box(ui: &mut egui::Ui, text: &str, is_error: bool) {
        let (bg, border, color) = if is_error {
            (Self::c_error_bg(), Self::c_error(), Self::c_error())
        } else {
            (Self::c_success_bg(), Self::c_success(), Self::c_success())
        };
        egui::Frame::new()
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::vec2(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(text).size(13.0).color(color));
                });
            });
    }
}

// Login Screen
impl DnfLoginApp {
    fn show_login_screen(&mut self, ui: &mut egui::Ui) {
        let tr = self.t();

        ui.vertical_centered(|ui| {
            Self::draw_app_icon(ui, self.app_icon.as_ref());
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(tr.app_title)
                    .size(21.0)
                    .strong()
                    .color(Self::c_text()),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(tr.app_subtitle)
                    .size(10.0)
                    .color(Self::c_text3()),
            );
        });

        ui.add_space(16.0);

        if !self.config.is_configured() {
            Self::warning_box(ui, tr.warn_server_not_configured);
            ui.add_space(12.0);
        }

        ui.add(Self::text_input(
            tr.username,
            &mut self.username,
            tr.hint_username,
        ));
        ui.add_space(12.0);

        ui.add(Self::password_input(
            tr.password,
            &mut self.password,
            tr.hint_password,
        ));
        ui.add_space(9.0);

        ui.checkbox(
            &mut self.remember_password,
            egui::RichText::new(tr.remember_password)
                .size(12.5)
                .color(Self::c_text2()),
        );
        ui.add_space(16.0);

        let login_enabled =
            !self.username.is_empty() && !self.password.is_empty() && self.current_task.is_none();

        if Self::primary_button(ui, tr.enter_game, login_enabled) {
            self.handle_login();
        }

        if matches!(self.current_task, Some(TaskType::Login)) {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        egui::RichText::new(tr.signing_in)
                            .size(12.5)
                            .color(Self::c_text2()),
                    );
                });
            });
        }

        ui.add_space(16.0);
        let sep = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0));
        ui.painter()
            .rect_filled(sep, egui::CornerRadius::ZERO, Self::c_border());
        ui.add_space(1.0);
        ui.add_space(10.0);

        ui.columns(3, |cols| {
            cols[0].vertical_centered(|ui| {
                if ui
                    .link(
                        egui::RichText::new(tr.register_link)
                            .size(12.5)
                            .color(Self::c_text2()),
                    )
                    .clicked()
                {
                    self.state = AppState::Register;
                    self.message = None;
                    self.message_is_error = false;
                }
            });
            cols[1].vertical_centered(|ui| {
                if ui
                    .link(
                        egui::RichText::new(tr.change_password_link)
                            .size(12.5)
                            .color(Self::c_text2()),
                    )
                    .clicked()
                {
                    self.state = AppState::ChangePassword;
                    self.message = None;
                    self.message_is_error = false;
                }
            });
            cols[2].vertical_centered(|ui| {
                if ui
                    .link(
                        egui::RichText::new(tr.settings_link)
                            .size(12.5)
                            .color(Self::c_text2()),
                    )
                    .clicked()
                {
                    self.state = AppState::Settings;
                    self.message = None;
                    self.message_is_error = false;
                    self.settings_server_url = self.config.server_url.clone();
                    self.settings_aes_key = self.config.aes_key.clone();
                    self.settings_bg_path = self.config.bg_custom_path.clone();
                }
            });
        });
    }
}

// Register Screen
impl DnfLoginApp {
    fn show_register_screen(&mut self, ui: &mut egui::Ui) {
        let tr = self.t();

        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(tr.create_account_title)
                    .size(19.0)
                    .strong()
                    .color(Self::c_text()),
            );
        });
        ui.add_space(14.0);

        ui.add(Self::text_input(
            tr.username,
            &mut self.register_username,
            tr.hint_choose_username,
        ));
        ui.add_space(12.0);

        ui.add(Self::password_input(
            tr.password,
            &mut self.register_password,
            tr.hint_choose_password,
        ));
        ui.add_space(12.0);

        ui.add(Self::password_input(
            tr.confirm_password,
            &mut self.register_password_confirm,
            tr.hint_re_enter_password,
        ));
        ui.add_space(12.0);

        ui.add(Self::text_input(
            tr.qq_optional,
            &mut self.register_qq,
            tr.hint_qq,
        ));
        ui.add_space(16.0);

        let sep = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0));
        ui.painter()
            .rect_filled(sep, egui::CornerRadius::ZERO, Self::c_border());
        ui.add_space(1.0);
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.add(Self::secondary_button(tr.back)).clicked() {
                self.state = AppState::Login;
                self.message = None;
                self.message_is_error = false;
            }
            ui.add_space(10.0);

            let enabled = !self.register_username.is_empty()
                && !self.register_password.is_empty()
                && !self.register_password_confirm.is_empty()
                && self.current_task.is_none();

            if Self::primary_button_slim(ui, tr.register_btn, enabled) {
                self.handle_register();
            }

            if matches!(self.current_task, Some(TaskType::Register)) {
                ui.add_space(8.0);
                ui.spinner();
            }
        });
    }
}

// Change Password Screen
impl DnfLoginApp {
    fn show_change_password_screen(&mut self, ui: &mut egui::Ui) {
        let tr = self.t();

        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(tr.change_password_title)
                    .size(19.0)
                    .strong()
                    .color(Self::c_text()),
            );
        });
        ui.add_space(14.0);

        ui.add(Self::text_input(
            tr.username,
            &mut self.changepwd_username,
            tr.hint_username,
        ));
        ui.add_space(12.0);

        ui.add(Self::password_input(
            tr.current_password,
            &mut self.changepwd_old_password,
            tr.hint_current_password,
        ));
        ui.add_space(12.0);

        ui.add(Self::password_input(
            tr.new_password,
            &mut self.changepwd_new_password,
            tr.hint_enter_new_password,
        ));
        ui.add_space(12.0);

        ui.add(Self::password_input(
            tr.confirm_new_password,
            &mut self.changepwd_confirm,
            tr.hint_confirm_new_password,
        ));
        ui.add_space(16.0);

        let sep = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0));
        ui.painter()
            .rect_filled(sep, egui::CornerRadius::ZERO, Self::c_border());
        ui.add_space(1.0);
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.add(Self::secondary_button(tr.back)).clicked() {
                self.state = AppState::Login;
                self.message = None;
                self.message_is_error = false;
            }
            ui.add_space(10.0);

            let enabled = !self.changepwd_username.is_empty()
                && !self.changepwd_old_password.is_empty()
                && !self.changepwd_new_password.is_empty()
                && !self.changepwd_confirm.is_empty()
                && self.current_task.is_none();

            if Self::primary_button_slim(ui, tr.change_password_btn, enabled) {
                self.handle_change_password();
            }

            if matches!(self.current_task, Some(TaskType::ChangePassword)) {
                ui.add_space(8.0);
                ui.spinner();
            }
        });
    }
}

// Settings Screen
impl DnfLoginApp {
    fn show_settings_screen(&mut self, ui: &mut egui::Ui) {
        let tr = self.t();

        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(tr.settings_title)
                    .size(19.0)
                    .strong()
                    .color(Self::c_text()),
            );
        });
        ui.add_space(12.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            if !self.config.is_configured() {
                Self::warning_box(ui, tr.warn_first_launch);
                ui.add_space(12.0);
            }

            ui.label(
                egui::RichText::new(tr.language_label)
                    .size(12.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            let old_lang = self.config.language;
            let avail = ui.available_width();
            egui::ComboBox::from_id_salt("language_select")
                .selected_text(self.config.language.label())
                .width(avail)
                .show_ui(ui, |ui| {
                    for &lang in Language::all() {
                        ui.selectable_value(&mut self.config.language, lang, lang.label());
                    }
                });
            if self.config.language != old_lang {
                self.tr = translations(self.config.language);
                let _ = self.config.save();
            }
            ui.add_space(14.0);

            ui.add(Self::text_input(
                tr.server_url_label,
                &mut self.settings_server_url,
                tr.server_url_hint,
            ));
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(tr.server_url_help)
                    .size(11.0)
                    .color(Self::c_text3()),
            );
            ui.add_space(14.0);

            ui.add(Self::text_input(
                tr.aes_key_label,
                &mut self.settings_aes_key,
                tr.aes_key_hint,
            ));
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(tr.aes_key_help)
                    .size(11.0)
                    .color(Self::c_text3()),
            );
            ui.add_space(14.0);

            let sep =
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0));
            ui.painter()
                .rect_filled(sep, egui::CornerRadius::ZERO, Self::c_border());
            ui.add_space(1.0);
            ui.add_space(12.0);

            ui.add(Self::text_input(
                tr.bg_custom_path_label,
                &mut self.settings_bg_path,
                tr.bg_custom_path_hint,
            ));
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(tr.bg_custom_path_help)
                    .size(11.0)
                    .color(Self::c_text3()),
            );
            ui.add_space(10.0);

            ui.label(
                egui::RichText::new(tr.bg_position_label)
                    .size(12.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let old_prepend = self.config.bg_custom_prepend;
                ui.radio_value(
                    &mut self.config.bg_custom_prepend,
                    false,
                    egui::RichText::new(tr.bg_position_append).size(13.0),
                );
                ui.add_space(8.0);
                ui.radio_value(
                    &mut self.config.bg_custom_prepend,
                    true,
                    egui::RichText::new(tr.bg_position_prepend).size(13.0),
                );
                if self.config.bg_custom_prepend != old_prepend {
                    let _ = self.config.save();
                }
            });
            ui.add_space(10.0);

            if Self::primary_button_slim(ui, tr.bg_reload_btn, true) {
                self.config.bg_custom_path = self.settings_bg_path.trim().to_string();
                let _ = self.config.save();
                self.start_bg_loading();
            }

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(tr.bg_fill_mode_label)
                    .size(12.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            let fill_mode_label = match &self.config.bg_fill_mode {
                BgFillMode::Tile => tr.bg_fill_tile,
                BgFillMode::Stretch => tr.bg_fill_stretch,
                BgFillMode::Fill => tr.bg_fill_fill,
                BgFillMode::Center => tr.bg_fill_center,
                BgFillMode::Fit => tr.bg_fill_fit,
            };
            let avail = ui.available_width();
            let old_mode = self.config.bg_fill_mode;
            egui::ComboBox::from_id_salt("bg_fill_mode")
                .selected_text(fill_mode_label)
                .width(avail)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.config.bg_fill_mode,
                        BgFillMode::Fill,
                        tr.bg_fill_fill,
                    );
                    ui.selectable_value(
                        &mut self.config.bg_fill_mode,
                        BgFillMode::Fit,
                        tr.bg_fill_fit,
                    );
                    ui.selectable_value(
                        &mut self.config.bg_fill_mode,
                        BgFillMode::Stretch,
                        tr.bg_fill_stretch,
                    );
                    ui.selectable_value(
                        &mut self.config.bg_fill_mode,
                        BgFillMode::Center,
                        tr.bg_fill_center,
                    );
                    ui.selectable_value(
                        &mut self.config.bg_fill_mode,
                        BgFillMode::Tile,
                        tr.bg_fill_tile,
                    );
                });
            if self.config.bg_fill_mode != old_mode {
                let _ = self.config.save();
            }

            ui.add_space(14.0);

            let sep =
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0));
            ui.painter()
                .rect_filled(sep, egui::CornerRadius::ZERO, Self::c_border());
            ui.add_space(1.0);
            ui.add_space(12.0);

            ui.add(Self::text_input(
                tr.plugins_dir_label,
                &mut self.settings_plugins_dir,
                tr.plugins_dir_hint,
            ));
            ui.add_space(3.0);
            ui.label(
                egui::RichText::new(tr.plugins_dir_help)
                    .size(11.0)
                    .color(Self::c_text3()),
            );
            ui.add_space(10.0);

            if Self::primary_button_slim(ui, tr.save_btn, true) {
                self.config.plugins_dir = self.settings_plugins_dir.trim().to_string();
                let _ = self.config.save();
            }

            if let Some(msg) = &self.message {
                ui.add_space(12.0);
                Self::status_box(ui, msg, self.message_is_error);
            }

            ui.add_space(16.0);

            let sep =
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 1.0));
            ui.painter()
                .rect_filled(sep, egui::CornerRadius::ZERO, Self::c_border());
            ui.add_space(1.0);
            ui.add_space(10.0);

            ui.label(
                egui::RichText::new(tr.saved_config_label)
                    .size(11.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(5.0);

            if self.config.is_configured() {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_premultiplied(5, 5, 12, 200))
                    .stroke(egui::Stroke::new(1.0, Self::c_border()))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::vec2(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let label_w = 32.0;
                        let row_h = 15.0;
                        ui.horizontal(|ui| {
                            let (lr, _) = ui.allocate_exact_size(
                                egui::vec2(label_w, row_h),
                                egui::Sense::hover(),
                            );
                            ui.painter().text(
                                lr.left_center(),
                                egui::Align2::LEFT_CENTER,
                                "URL",
                                egui::FontId::proportional(11.5),
                                Self::c_text3(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.config.server_url)
                                        .size(11.5)
                                        .color(Self::c_text()),
                                )
                                .truncate(),
                            );
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let (lr, _) = ui.allocate_exact_size(
                                egui::vec2(label_w, row_h),
                                egui::Sense::hover(),
                            );
                            ui.painter().text(
                                lr.left_center(),
                                egui::Align2::LEFT_CENTER,
                                "KEY",
                                egui::FontId::proportional(11.5),
                                Self::c_text3(),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.cached_key_preview)
                                        .size(11.5)
                                        .color(Self::c_text()),
                                )
                                .truncate(),
                            );
                        });
                    });
            } else {
                ui.label(
                    egui::RichText::new(tr.not_configured)
                        .size(12.5)
                        .color(Self::c_text3()),
                );
            }

            ui.add_space(16.0);

            ui.horizontal(|ui| {
                if ui.add(Self::secondary_button(tr.back)).clicked() {
                    self.state = AppState::Login;
                    self.message = None;
                    self.message_is_error = false;
                }
                ui.add_space(6.0);
                if ui.add(Self::secondary_button(tr.clear_btn)).clicked() {
                    self.settings_server_url.clear();
                    self.settings_aes_key.clear();
                }
                ui.add_space(6.0);
                if Self::primary_button_slim(ui, tr.save_btn, true) {
                    self.handle_save_settings();
                }
            });
        });
    }
}

// Global message overlay
impl DnfLoginApp {
    fn show_message(&self, ui: &mut egui::Ui) {
        if self.state == AppState::Settings {
            return;
        }

        if let Some(msg) = &self.message {
            ui.add_space(10.0);
            let text_color = if self.message_is_error {
                Self::c_error()
            } else {
                Self::c_success()
            };
            egui::Frame::new()
                .fill(if self.message_is_error {
                    Self::c_error_bg()
                } else {
                    Self::c_success_bg()
                })
                .stroke(egui::Stroke::new(1.0, text_color))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::vec2(16.0, 10.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(msg.as_str())
                                .size(13.0)
                                .color(text_color),
                        );
                    });
                });
        }
    }

    fn set_error(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_is_error = true;
    }

    fn set_success(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
        self.message_is_error = false;
    }
}

// Action handlers
impl DnfLoginApp {
    fn handle_login(&mut self) {
        self.message = None;
        self.message_is_error = false;
        let tr = self.t();

        if !self.config.is_configured() {
            self.set_error(tr.err_server_not_configured);
            return;
        }
        if self.username.is_empty() || self.password.is_empty() {
            self.set_error(tr.err_enter_username_password);
            return;
        }

        let mac_address = match platform::get_mac_address() {
            Ok(mac) => mac,
            Err(e) => {
                self.set_error(format!("{}: {}", tr.err_mac_prefix, e));
                return;
            }
        };

        let password_md5 = md5_hash(&self.password);

        let client = match self.client.clone() {
            Some(c) => c,
            None => {
                self.set_error(tr.err_client_not_init);
                return;
            }
        };

        self.current_task = Some(TaskType::Login);

        let username = self.username.clone();
        let tx = self.task_tx.clone();

        tracing::info!(
            "Login task started: user={}, mac={}",
            self.username,
            mac_address
        );

        self.runtime.spawn(async move {
            let result = client.login(&username, &password_md5, &mac_address).await;
            let _ = tx.send(TaskResult::Login(result));
        });
    }

    fn handle_register(&mut self) {
        self.message = None;
        self.message_is_error = false;
        let tr = self.t();

        if !self.config.is_configured() {
            self.set_error(tr.err_server_not_configured);
            return;
        }
        if self.register_username.is_empty() {
            self.set_error(tr.err_enter_username);
            return;
        }
        if self.register_password.is_empty() {
            self.set_error(tr.err_enter_password);
            return;
        }
        if self.register_password != self.register_password_confirm {
            self.set_error(tr.err_passwords_no_match);
            return;
        }

        let password_md5 = md5_hash(&self.register_password);
        let qq = if self.register_qq.is_empty() {
            None
        } else {
            Some(self.register_qq.clone())
        };

        let client = match self.client.clone() {
            Some(c) => c,
            None => {
                self.set_error(tr.err_client_not_init);
                return;
            }
        };

        self.current_task = Some(TaskType::Register);

        let username = self.register_username.clone();
        let tx = self.task_tx.clone();

        self.runtime.spawn(async move {
            let result = client.register(&username, &password_md5, qq).await;
            let _ = tx.send(TaskResult::Register(result));
        });

        tracing::info!("Register task started: user={}", self.register_username);
    }

    fn handle_change_password(&mut self) {
        self.message = None;
        self.message_is_error = false;
        let tr = self.t();

        if !self.config.is_configured() {
            self.set_error(tr.err_server_not_configured);
            return;
        }
        if self.changepwd_username.is_empty() {
            self.set_error(tr.err_enter_username);
            return;
        }
        if self.changepwd_old_password.is_empty() {
            self.set_error(tr.err_enter_old_password);
            return;
        }
        if self.changepwd_new_password.is_empty() {
            self.set_error(tr.err_enter_new_password);
            return;
        }
        if self.changepwd_new_password != self.changepwd_confirm {
            self.set_error(tr.err_passwords_no_match);
            return;
        }

        let old_md5 = md5_hash(&self.changepwd_old_password);
        let new_md5 = md5_hash(&self.changepwd_new_password);

        let client = match self.client.clone() {
            Some(c) => c,
            None => {
                self.set_error(tr.err_client_not_init);
                return;
            }
        };

        self.current_task = Some(TaskType::ChangePassword);

        let username = self.changepwd_username.clone();
        let tx = self.task_tx.clone();

        self.runtime.spawn(async move {
            let result = client.change_password(&username, &old_md5, &new_md5).await;
            let _ = tx.send(TaskResult::ChangePassword(result));
        });

        tracing::info!(
            "Change password task started: user={}",
            self.changepwd_username
        );
    }

    fn handle_save_settings(&mut self) {
        self.message = None;
        self.message_is_error = false;
        let tr = self.t();

        let new_config = AppConfig {
            server_url: self.settings_server_url.trim().to_string(),
            aes_key: self.settings_aes_key.trim().to_string(),
            ..self.config.clone()
        };

        if let Err(e) = new_config.validate() {
            self.set_error(format!("{}: {}", tr.err_config_prefix, e));
            return;
        }
        if let Err(e) = new_config.save() {
            self.set_error(format!("{}: {}", tr.err_save_prefix, e));
            return;
        }

        self.config = new_config;
        self.settings_server_url = self.config.server_url.clone();
        self.settings_aes_key = self.config.aes_key.clone();
        self.tr = translations(self.config.language);
        self.cached_key_preview = Self::make_key_preview(&self.config.aes_key);

        match self.config.get_aes_key_bytes() {
            Ok(key) => match DnfClient::new(self.config.server_url.clone(), &key) {
                Ok(client) => {
                    self.client = Some(client);
                    self.set_success(tr.settings_saved);
                }
                Err(e) => self.set_error(format!("Failed to initialize network client: {}", e)),
            },
            Err(e) => self.set_error(format!("Failed to parse AES key: {}", e)),
        }
    }
}

// eframe::App
impl eframe::App for DnfLoginApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(result) = self.task_rx.try_recv() {
            self.handle_task_result(result);
        }

        // Kick off background loading once on the first frame.
        if !self.bg_loading_started {
            self.bg_loading_started = true;
            self.start_bg_loading();
        }

        // Upload background textures decoded by worker threads.
        let mut loaded_any = false;
        while let Ok(data) = self.img_rx.try_recv() {
            let i = data.index;
            if i < self.bgs.len() {
                self.bgs[i] = Some(ctx.load_texture(
                    format!("bg_{i}"),
                    data.full_image,
                    egui::TextureOptions::LINEAR,
                ));
                self.bg_thumbs[i] = Some(ctx.load_texture(
                    format!("thumb_{i}"),
                    data.thumb_image,
                    egui::TextureOptions::LINEAR,
                ));
                loaded_any = true;
            }
        }
        // Keep repainting until all backgrounds are loaded.
        if loaded_any || self.bgs.iter().any(|b| b.is_none()) {
            ctx.request_repaint();
        }

        let screen = ctx.viewport_rect();

        // Background image and thumbnail strip.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                self.paint_background(ui, screen);
                self.draw_thumbnail_strip(ui, screen);
            });

        // Glass card floated above the background panel.
        let glass = egui::Frame::new()
            .fill(Self::c_glass_fill())
            .stroke(egui::Stroke::new(1.0, Self::c_glass_border()))
            .corner_radius(egui::CornerRadius::same(14))
            .inner_margin(egui::vec2(24.0, 18.0));

        let max_card_h = screen.height() - 68.0;

        egui::Window::new("dnf_card")
            .title_bar(false)
            .resizable(false)
            .movable(false)
            .min_width(400.0)
            .max_width(400.0)
            .max_height(max_card_h)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -22.0))
            .frame(glass)
            .show(ctx, |ui| {
                match self.state {
                    AppState::Login => self.show_login_screen(ui),
                    AppState::Register => self.show_register_screen(ui),
                    AppState::ChangePassword => self.show_change_password_screen(ui),
                    AppState::Settings => self.show_settings_screen(ui),
                }
                self.show_message(ui);
            });

        if self.current_task.is_some() {
            ctx.request_repaint();
        }
    }
}

impl DnfLoginApp {
    fn handle_task_result(&mut self, result: TaskResult) {
        let tr = self.t();
        match result {
            TaskResult::Login(res) => {
                self.current_task = None;
                match res {
                    Ok(response) => {
                        if response.success {
                            if let Some(token) = response.token {
                                if let Err(e) = self.storage.save(
                                    &self.username,
                                    &self.password,
                                    self.remember_password,
                                ) {
                                    tracing::warn!("Failed to save credentials: {}", e);
                                }
                                // Clear the in-memory plaintext password.
                                self.password = String::new();
                                // Restore from storage now so re-launch works regardless of
                                // whether the launch call below succeeds or fails.
                                if self.remember_password
                                    && let Ok((_, p)) = self.storage.load()
                                {
                                    self.password = p;
                                }
                                self.set_success(tr.login_success);
                                self.login_token = Some(token.clone());
                                self.logged_in_user = Some(self.username.clone());

                                let plugins_dir = self.config.plugins_dir.clone();
                                if let Err(e) = DnfLauncher::launch_with_token(&token, &plugins_dir)
                                {
                                    self.set_error(format!("{}: {}", tr.err_launch_prefix, e));
                                } else {
                                    tracing::info!("Game launched: user={}", self.username);
                                    self.message = None;
                                }
                            } else {
                                // Server reported success but returned no token; treat as an error.
                                tracing::error!("Login response: success=true but token is absent");
                                self.set_error(tr.err_network_prefix.to_string());
                            }
                        } else {
                            self.set_error(
                                response.error.unwrap_or_else(|| "Login failed".to_string()),
                            );
                        }
                    }
                    Err(e) => self.set_error(format!("{}: {}", tr.err_network_prefix, e)),
                }
            }
            TaskResult::Register(res) => {
                self.current_task = None;
                match res {
                    Ok(response) => {
                        if response.success {
                            self.set_success(tr.register_success);
                            self.register_username.clear();
                            self.register_password.clear();
                            self.register_password_confirm.clear();
                            self.register_qq.clear();
                        } else {
                            self.set_error(
                                response
                                    .error
                                    .unwrap_or_else(|| "Registration failed".to_string()),
                            );
                        }
                    }
                    Err(e) => self.set_error(format!("{}: {}", tr.err_network_prefix, e)),
                }
            }
            TaskResult::ChangePassword(res) => {
                self.current_task = None;
                match res {
                    Ok(response) => {
                        if response.success {
                            self.set_success(tr.change_password_success);
                            self.changepwd_username.clear();
                            self.changepwd_old_password.clear();
                            self.changepwd_new_password.clear();
                            self.changepwd_confirm.clear();
                        } else {
                            self.set_error(
                                response
                                    .error
                                    .unwrap_or_else(|| "Password change failed".to_string()),
                            );
                        }
                    }
                    Err(e) => self.set_error(format!("{}: {}", tr.err_network_prefix, e)),
                }
            }
        }
    }
}
