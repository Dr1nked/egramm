use crate::addons::{decrypt_audio_frame, derive_chat_key, encrypt_audio_frame, SoundPlayer};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Rounding, Stroke, Vec2};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SERVER_URL: &str = "https://egramm.online";
pub const WS_URL: &str = "wss://egramm.online/ws";
pub const VOICE_TARGET_RATE: u32 = 16_000;
pub const MAX_CALL_PARTICIPANTS: usize = 4;
pub const VOICE_GATE_RMS: f32 = 0.008;
pub const VOICE_HANGOVER_FRAMES: u8 = 8;
pub const SCREEN_MAX_WIDTH: u32 = 1024;
pub const SCREEN_MAX_HEIGHT: u32 = 576;
pub const SCREEN_FRAME_INTERVAL_MS: u64 = 100;

pub const AUTH_BLUE: Color32 = Color32::from_rgb(43, 135, 232);
pub const AUTH_BLUE_HOVER: Color32 = Color32::from_rgb(31, 118, 214);
pub const DANGER_RED: Color32 = Color32::from_rgb(235, 95, 95);
pub const ONLINE_GREEN: Color32 = Color32::from_rgb(68, 198, 122);
pub const READ_TICK_BLUE: Color32 = Color32::from_rgb(82, 188, 255);

pub const GROUP_COLORS: [Color32; 6] = [
    Color32::from_rgb(58, 128, 210),
    Color32::from_rgb(112, 86, 198),
    Color32::from_rgb(46, 148, 130),
    Color32::from_rgb(214, 122, 54),
    Color32::from_rgb(198, 72, 116),
    Color32::from_rgb(76, 152, 216),
];

pub fn default_audio_channels() -> u16 {
    1
}

pub fn soft_clip(x: f32) -> f32 {
    if x.abs() > 0.85 {
        x.tanh()
    } else {
        x
    }
}

pub fn generate_call_id() -> String {
    let mut buf = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn call_encryption_fingerprint(chat_id: i64, call_id: &str) -> [(u8, Color32); 4] {
    let key = derive_chat_key(chat_id);
    let mut mix = [0_u8; 4];
    for (i, b) in call_id.bytes().enumerate() {
        mix[i % 4] = mix[i % 4].wrapping_add(b.rotate_left((i % 5) as u32));
    }
    let palette = [
        Color32::from_rgb(90, 196, 255),
        Color32::from_rgb(118, 224, 152),
        Color32::from_rgb(250, 202, 86),
        Color32::from_rgb(186, 146, 255),
        Color32::from_rgb(255, 138, 168),
        Color32::from_rgb(104, 226, 214),
    ];
    let mut out = [(0_u8, palette[0]); 4];
    for i in 0..4 {
        let v = key[i * 7] ^ mix[i];
        out[i] = (v % 4, palette[(v as usize) % palette.len()]);
    }
    out
}

#[derive(Clone, Copy)]
pub struct ThemePalette {
    pub auth_bg: Color32,
    pub auth_card: Color32,
    pub auth_border: Color32,
    pub auth_blue_soft: Color32,
    pub auth_text: Color32,
    pub auth_muted: Color32,

    pub bg_chat: Color32,
    pub bg_panel: Color32,
    pub bg_popup: Color32,
    pub bg_input: Color32,
    pub bg_hover: Color32,
    pub bg_selected: Color32,
    pub bubble_out: Color32,
    pub bubble_in: Color32,
    pub card_inner: Color32,
    pub border_line: Color32,
    pub popup_border: Color32,
    pub drawer_header: Color32,
    pub accent_blue: Color32,
    pub accent_blue_hover: Color32,
    pub accent_light: Color32,
    pub text_main: Color32,
    pub text_muted: Color32,
    pub text_on_selected: Color32,
}

impl ThemePalette {
    pub fn dark() -> Self {
        Self {
            auth_bg: Color32::from_rgb(14, 22, 33),
            auth_card: Color32::from_rgb(23, 33, 43),
            auth_border: Color32::from_rgb(38, 52, 68),
            auth_blue_soft: Color32::from_rgb(26, 52, 82),
            auth_text: Color32::from_rgb(245, 247, 250),
            auth_muted: Color32::from_rgb(138, 158, 178),

            bg_chat: Color32::from_rgb(14, 22, 33),
            bg_panel: Color32::from_rgb(23, 33, 43),
            bg_popup: Color32::from_rgb(23, 33, 43),
            bg_input: Color32::from_rgb(36, 47, 61),
            bg_hover: Color32::from_rgb(32, 43, 54),
            bg_selected: Color32::from_rgb(43, 82, 120),
            bubble_out: Color32::from_rgb(43, 82, 120),
            bubble_in: Color32::from_rgb(24, 37, 51),
            card_inner: Color32::from_rgb(20, 32, 46),
            border_line: Color32::from_rgb(16, 25, 33),
            popup_border: Color32::from_rgb(42, 58, 76),
            drawer_header: Color32::from_rgb(28, 41, 56),
            accent_blue: Color32::from_rgb(43, 135, 232),
            accent_blue_hover: Color32::from_rgb(60, 150, 244),
            accent_light: Color32::from_rgb(106, 181, 245),
            text_main: Color32::from_rgb(245, 245, 245),
            text_muted: Color32::from_rgb(127, 145, 164),
            text_on_selected: Color32::from_rgb(235, 244, 255),
        }
    }

    pub fn light() -> Self {
        Self {
            auth_bg: Color32::from_rgb(238, 245, 252),
            auth_card: Color32::from_rgb(255, 255, 255),
            auth_border: Color32::from_rgb(212, 228, 247),
            auth_blue_soft: Color32::from_rgb(224, 239, 255),
            auth_text: Color32::from_rgb(20, 38, 61),
            auth_muted: Color32::from_rgb(100, 124, 153),

            bg_chat: Color32::from_rgb(225, 237, 248),
            bg_panel: Color32::from_rgb(255, 255, 255),
            bg_popup: Color32::from_rgb(255, 255, 255),
            bg_input: Color32::from_rgb(240, 244, 248),
            bg_hover: Color32::from_rgb(238, 244, 250),
            bg_selected: Color32::from_rgb(65, 159, 217),
            bubble_out: Color32::from_rgb(220, 241, 255),
            bubble_in: Color32::from_rgb(255, 255, 255),
            card_inner: Color32::from_rgb(236, 245, 253),
            border_line: Color32::from_rgb(219, 228, 238),
            popup_border: Color32::from_rgb(206, 218, 232),
            drawer_header: Color32::from_rgb(238, 246, 253),
            accent_blue: Color32::from_rgb(43, 135, 232),
            accent_blue_hover: Color32::from_rgb(31, 118, 214),
            accent_light: Color32::from_rgb(33, 118, 210),
            text_main: Color32::from_rgb(22, 34, 48),
            text_muted: Color32::from_rgb(112, 129, 146),
            text_on_selected: Color32::from_rgb(255, 255, 255),
        }
    }
}

pub fn apply_theme_style(ctx: &egui::Context, p: &ThemePalette, is_dark: bool) {
    let mut style = (*ctx.style()).clone();
    style.visuals = if is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    style.visuals.panel_fill = p.bg_chat;
    style.visuals.window_fill = p.bg_panel;
    style.visuals.window_rounding = Rounding::same(14.0);
    style.visuals.window_stroke = Stroke::new(1.0, p.popup_border);
    style.visuals.extreme_bg_color = p.bg_input;

    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text_main);

    style.visuals.widgets.inactive.bg_fill = p.bg_input;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text_main);
    style.visuals.widgets.inactive.rounding = Rounding::same(14.0);

    style.visuals.widgets.hovered.bg_fill = p.bg_hover;
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text_main);
    style.visuals.widgets.hovered.rounding = Rounding::same(14.0);

    style.visuals.widgets.active.bg_fill = p.accent_blue;
    style.visuals.widgets.active.bg_stroke = Stroke::NONE;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.active.rounding = Rounding::same(14.0);

    style.visuals.selection.bg_fill = p.bg_selected;
    style.visuals.selection.stroke = Stroke::new(1.0, p.accent_light);
    ctx.set_style(style);
}

pub fn semibold_font(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("semibold".into()))
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    1.0 - (1.0 - c).powi(3)
}

pub fn ease_out_back(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    let c1 = 1.70158_f32;
    let c3 = c1 + 1.0;
    1.0 + c3 * (c - 1.0).powi(3) + c1 * (c - 1.0).powi(2)
}

pub fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    let a = (color.a() as f32 * alpha.clamp(0.0, 1.0)) as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} ГБ", b / GB)
    } else if b >= MB {
        format!("{:.1} МБ", b / MB)
    } else if b >= KB {
        format!("{:.0} КБ", b / KB)
    } else {
        format!("{} Б", bytes)
    }
}

pub fn is_image_name(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
}

pub fn get_or_load_image(
    ctx: &egui::Context,
    cache: &mut HashMap<PathBuf, Option<egui::TextureHandle>>,
    path: &PathBuf,
) -> Option<egui::TextureHandle> {
    if let Some(entry) = cache.get(path) {
        return entry.clone();
    }

    let loaded = std::fs::read(path)
        .ok()
        .and_then(|bytes| image::load_from_memory(&bytes).ok())
        .map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            ctx.load_texture(
                path.to_string_lossy().to_string(),
                color_image,
                egui::TextureOptions::LINEAR,
            )
        });

    cache.insert(path.clone(), loaded.clone());
    loaded
}

pub fn load_circular_avatar_from_bytes(
    ctx: &egui::Context,
    bytes: &[u8],
    key_tag: &str,
) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    let side = w.min(h);
    let x = (w - side) / 2;
    let y = (h - side) / 2;

    let cropped = img
        .crop_imm(x, y, side, side)
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let mut rgba = cropped.to_rgba8();

    let center = 127.5_f32;
    let radius = 127.0_f32;
    for (px, py, pixel) in rgba.enumerate_pixels_mut() {
        let dx = px as f32 - center;
        let dy = py as f32 - center;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > radius {
            pixel[3] = 0;
        } else if dist > radius - 1.8 {
            let factor = ((radius - dist) / 1.8).clamp(0.0, 1.0);
            pixel[3] = (pixel[3] as f32 * factor) as u8;
        }
    }

    let color_image = egui::ColorImage::from_rgba_unmultiplied([256, 256], rgba.as_raw());
    Some(ctx.load_texture(
        key_tag.to_string(),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}

pub fn load_circular_avatar(
    ctx: &egui::Context,
    path: &Path,
    prefix: &str,
) -> Option<egui::TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    load_circular_avatar_from_bytes(ctx, &bytes, &format!("{}_{}", prefix, path.to_string_lossy()))
}

pub fn drive_cache_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("egramm_drive_cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn notify_windows_message(sender: &str, preview: &str) {
    let clean_sender: String = sender
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let clean_body: String = preview
        .chars()
        .filter(|c| !c.is_control())
        .take(90)
        .collect();

    std::thread::spawn(move || {
        let script = r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null; $t = [System.Security.SecurityElement]::Escape($env:EGRAMM_TITLE); $b = [System.Security.SecurityElement]::Escape($env:EGRAMM_BODY); $xml = New-Object Windows.Data.Xml.Dom.XmlDocument; $xml.LoadXml("<toast><visual><binding template=`"ToastText02`"><text id=`"1`">Egramm — $t</text><text id=`"2`">$b</text></binding></visual></toast>"); $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Egramm').Show($toast);"#;
        let mut cmd = std::process::Command::new("powershell");
        cmd.env("EGRAMM_TITLE", clean_sender)
            .env("EGRAMM_BODY", clean_body)
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let _ = cmd.output();
    });
}

#[cfg(windows)]
pub fn capture_primary_screen_scaled_rgb(max_w: u32, max_h: u32) -> Option<(u32, u32, Vec<u8>)> {
    use std::ffi::c_void;

    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    const SRCCOPY: u32 = 0x00CC_0020;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct BitmapInfo {
        bmi_header: BitmapInfoHeader,
        bmi_colors: [u32; 1],
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(n_index: i32) -> i32;
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
        fn DeleteDC(hdc: *mut c_void) -> i32;
        fn CreateCompatibleBitmap(hdc: *mut c_void, cx: i32, cy: i32) -> *mut c_void;
        fn SelectObject(hdc: *mut c_void, h: *mut c_void) -> *mut c_void;
        fn DeleteObject(ho: *mut c_void) -> i32;
        fn BitBlt(
            hdc: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            hdc_src: *mut c_void,
            x1: i32,
            y1: i32,
            rop: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: *mut c_void,
            hbm: *mut c_void,
            start: u32,
            c_lines: u32,
            lpv_bits: *mut c_void,
            lpbmi: *mut BitmapInfo,
            usage: u32,
        ) -> i32;
    }

    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w <= 0 || h <= 0 {
            return None;
        }

        let hdc_screen = GetDC(std::ptr::null_mut());
        if hdc_screen.is_null() {
            return None;
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_null() {
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            return None;
        }

        let hbm = CreateCompatibleBitmap(hdc_screen, w, h);
        if hbm.is_null() {
            DeleteDC(hdc_mem);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            return None;
        }

        let old_obj = SelectObject(hdc_mem, hbm);
        let blt_ok = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, 0, 0, SRCCOPY);

        let src_w = w as usize;
        let src_h = h as usize;
        let mut bgra = vec![0_u8; src_w * src_h * 4];
        let mut bmi = BitmapInfo {
            bmi_header: BitmapInfoHeader {
                bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                bi_width: w,
                bi_height: -h,
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: BI_RGB,
                bi_size_image: 0,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            },
            bmi_colors: [0],
        };

        let lines = if blt_ok != 0 {
            GetDIBits(
                hdc_mem,
                hbm,
                0,
                h as u32,
                bgra.as_mut_ptr() as *mut c_void,
                &mut bmi,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        SelectObject(hdc_mem, old_obj);
        DeleteObject(hbm);
        DeleteDC(hdc_mem);
        ReleaseDC(std::ptr::null_mut(), hdc_screen);

        if lines == 0 {
            return None;
        }

        let scale = (max_w as f32 / src_w as f32)
            .min(max_h as f32 / src_h as f32)
            .min(1.0);
        let out_w = ((src_w as f32 * scale).round() as usize).max(2);
        let out_h = ((src_h as f32 * scale).round() as usize).max(2);

        let mut rgb = Vec::with_capacity(out_w * out_h * 3);
        let x_step = src_w as f32 / out_w as f32;
        let y_step = src_h as f32 / out_h as f32;

        for oy in 0..out_h {
            let sy = ((oy as f32 * y_step) as usize).min(src_h - 1);
            let row_off = sy * src_w;
            for ox in 0..out_w {
                let sx = ((ox as f32 * x_step) as usize).min(src_w - 1);
                let idx = (row_off + sx) * 4;
                rgb.push(bgra[idx + 2]);
                rgb.push(bgra[idx + 1]);
                rgb.push(bgra[idx]);
            }
        }

        Some((out_w as u32, out_h as u32, rgb))
    }
}

#[cfg(not(windows))]
pub fn capture_primary_screen_scaled_rgb(_max_w: u32, _max_h: u32) -> Option<(u32, u32, Vec<u8>)> {
    None
}

#[derive(Clone)]
pub struct DecodedScreenFrame {
    pub chat_id: i64,
    pub sender_email: String,
    pub sender_name: String,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    pub seq: u64,
    pub updated_at: Instant,
}

struct RawScreenPacket {
    chat_id: i64,
    sender_email: String,
    sender_name: String,
    jpeg_b64: String,
}

pub struct SharedScreenBridge {
    latest_frame: Arc<Mutex<Option<DecodedScreenFrame>>>,
    active_chat_id: Arc<Mutex<Option<i64>>>,
    my_email: Arc<Mutex<String>>,
    repaint_ctx: Arc<Mutex<Option<egui::Context>>>,
    frame_seq: Arc<AtomicU64>,
    pub is_sharing: AtomicBool,
    decode_tx: SyncSender<RawScreenPacket>,
}

impl SharedScreenBridge {
    pub fn new() -> Self {
        let latest_frame: Arc<Mutex<Option<DecodedScreenFrame>>> = Arc::new(Mutex::new(None));
        let active_chat_id: Arc<Mutex<Option<i64>>> = Arc::new(Mutex::new(None));
        let my_email: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let repaint_ctx: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
        let frame_seq = Arc::new(AtomicU64::new(0));
        let (decode_tx, decode_rx) = mpsc::sync_channel::<RawScreenPacket>(1);

        let lf_bg = latest_frame.clone();
        let ac_bg = active_chat_id.clone();
        let seq_bg = frame_seq.clone();
        let ctx_bg = repaint_ctx.clone();

        std::thread::spawn(move || {
            while let Ok(pkt) = decode_rx.recv() {
                if let Ok(active) = ac_bg.lock() {
                    if *active != Some(pkt.chat_id) {
                        continue;
                    }
                }
                let Ok(encoded_bytes) = B64.decode(&pkt.jpeg_b64) else {
                    continue;
                };
                let Some(jpeg_bytes) = decrypt_audio_frame(pkt.chat_id, &encoded_bytes) else {
                    continue;
                };
                let Ok(img) = image::load_from_memory(&jpeg_bytes) else {
                    continue;
                };
                let rgba_img = img.to_rgba8();
                let w = rgba_img.width() as usize;
                let h = rgba_img.height() as usize;
                if w == 0 || h == 0 {
                    continue;
                }
                let seq = seq_bg.fetch_add(1, Ordering::SeqCst) + 1;
                if let Ok(mut guard) = lf_bg.lock() {
                    *guard = Some(DecodedScreenFrame {
                        chat_id: pkt.chat_id,
                        sender_email: pkt.sender_email,
                        sender_name: pkt.sender_name,
                        width: w,
                        height: h,
                        rgba: rgba_img.into_raw(),
                        seq,
                        updated_at: Instant::now(),
                    });
                }
                if let Ok(c_guard) = ctx_bg.lock() {
                    if let Some(ref ctx) = *c_guard {
                        ctx.request_repaint();
                    }
                }
            }
        });

        Self {
            latest_frame,
            active_chat_id,
            my_email,
            repaint_ctx,
            frame_seq,
            is_sharing: AtomicBool::new(false),
            decode_tx,
        }
    }

    pub fn set_repaint_ctx(&self, ctx: egui::Context) {
        if let Ok(mut guard) = self.repaint_ctx.lock() {
            *guard = Some(ctx);
        }
    }

    pub fn set_my_email(&self, email: String) {
        if let Ok(mut guard) = self.my_email.lock() {
            *guard = email;
        }
    }

    pub fn set_active_chat(&self, chat_id: Option<i64>) {
        if let Ok(mut guard) = self.active_chat_id.lock() {
            *guard = chat_id;
        }
        if chat_id.is_none() {
            self.clear_all();
        }
    }

    pub fn clear_all(&self) {
        if let Ok(mut guard) = self.latest_frame.lock() {
            *guard = None;
        }
    }

    pub fn clear_sender(&self, sender_email: &str) {
        if let Ok(mut guard) = self.latest_frame.lock() {
            if let Some(ref frame) = *guard {
                if frame.sender_email.eq_ignore_ascii_case(sender_email) {
                    *guard = None;
                }
            }
        }
    }

    pub fn push_remote_frame(
        &self,
        chat_id: i64,
        sender_email: &str,
        sender_name: &str,
        jpeg_b64: &str,
    ) {
        if sender_email.is_empty() || jpeg_b64.is_empty() {
            return;
        }
        if let Ok(me) = self.my_email.lock() {
            if !me.is_empty() && me.eq_ignore_ascii_case(sender_email) {
                return;
            }
        }
        if let Ok(active) = self.active_chat_id.lock() {
            if *active != Some(chat_id) {
                return;
            }
        }
        let _ = self.decode_tx.try_send(RawScreenPacket {
            chat_id,
            sender_email: sender_email.to_string(),
            sender_name: sender_name.to_string(),
            jpeg_b64: jpeg_b64.to_string(),
        });
    }

    pub fn push_local_preview(
        &self,
        chat_id: i64,
        sender_email: &str,
        sender_name: &str,
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    ) {
        if let Ok(active) = self.active_chat_id.lock() {
            if *active != Some(chat_id) {
                return;
            }
        }
        if let Ok(mut guard) = self.latest_frame.lock() {
            if let Some(ref existing) = *guard {
                if !existing.sender_email.eq_ignore_ascii_case(sender_email)
                    && existing.updated_at.elapsed() < Duration::from_secs(3)
                {
                    return;
                }
            }
            let seq = self.frame_seq.fetch_add(1, Ordering::SeqCst) + 1;
            *guard = Some(DecodedScreenFrame {
                chat_id,
                sender_email: sender_email.to_string(),
                sender_name: sender_name.to_string(),
                width,
                height,
                rgba,
                seq,
                updated_at: Instant::now(),
            });
        }
    }

    pub fn take_if_newer(&self, last_seq: u64) -> Option<DecodedScreenFrame> {
        let guard = self.latest_frame.lock().ok()?;
        let frame = guard.as_ref()?;
        if frame.seq != last_seq {
            Some(frame.clone())
        } else {
            None
        }
    }

    pub fn is_stream_alive(&self) -> bool {
        let Ok(guard) = self.latest_frame.lock() else {
            return false;
        };
        guard
            .as_ref()
            .map(|f| f.updated_at.elapsed() < Duration::from_secs(4))
            .unwrap_or(false)
    }
}

pub struct ScreenShareEngine {
    stop_flag: Arc<AtomicBool>,
    bridge: Arc<SharedScreenBridge>,
    ws_tx: Sender<String>,
    session_token: String,
    chat_id: i64,
    call_id: String,
    sender_email: String,
}

impl ScreenShareEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        ws_tx: Sender<String>,
        session_token: String,
        chat_id: i64,
        call_id: String,
        sender_email: String,
        sender_name: String,
        bridge: Arc<SharedScreenBridge>,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        if capture_primary_screen_scaled_rgb(64, 64).is_none() {
            return Err("Не удалось получить доступ к экрану".into());
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        bridge.is_sharing.store(true, Ordering::Relaxed);
        bridge.set_active_chat(Some(chat_id));

        let stop_bg = stop_flag.clone();
        let bridge_bg = bridge.clone();
        let ws_bg = ws_tx.clone();
        let tok_bg = session_token.clone();
        let cid_bg = call_id.clone();
        let em_bg = sender_email.clone();
        let name_bg = sender_name;

        std::thread::spawn(move || {
            let interval = Duration::from_millis(SCREEN_FRAME_INTERVAL_MS);
            while !stop_bg.load(Ordering::Relaxed) {
                let tick_start = Instant::now();

                if let Some((w, h, rgb)) =
                    capture_primary_screen_scaled_rgb(SCREEN_MAX_WIDTH, SCREEN_MAX_HEIGHT)
                {
                    let mut rgba = Vec::with_capacity((w as usize) * (h as usize) * 4);
                    for px in rgb.chunks_exact(3) {
                        rgba.push(px[0]);
                        rgba.push(px[1]);
                        rgba.push(px[2]);
                        rgba.push(255);
                    }
                    bridge_bg.push_local_preview(
                        chat_id,
                        &em_bg,
                        &name_bg,
                        w as usize,
                        h as usize,
                        rgba,
                    );
                    ctx.request_repaint();

                    if let Some(rgb_img) = image::RgbImage::from_raw(w, h, rgb) {
                        let dyn_img = image::DynamicImage::ImageRgb8(rgb_img);
                        let mut jpeg_buf = Vec::with_capacity(64 * 1024);
                        let enc_ok =
                            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg_buf, 60)
                                .encode_image(&dyn_img)
                                .is_ok();

                        if enc_ok {
                            if jpeg_buf.len() > 320_000 {
                                jpeg_buf.clear();
                                let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(
                                    &mut jpeg_buf,
                                    38,
                                )
                                .encode_image(&dyn_img);
                            }

                            if !jpeg_buf.is_empty() && jpeg_buf.len() <= 350_000 {
                                let encrypted = encrypt_audio_frame(chat_id, &jpeg_buf);
                                let wire = WsWireEvent::CallScreenFrame {
                                    session_token: tok_bg.clone(),
                                    chat_id,
                                    call_id: cid_bg.clone(),
                                    sender_email: em_bg.clone(),
                                    sender_name: name_bg.clone(),
                                    width: w,
                                    height: h,
                                    jpeg_b64: B64.encode(&encrypted),
                                };
                                if let Ok(json) = serde_json::to_string(&wire) {
                                    if ws_bg.send(json).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                let spent = tick_start.elapsed();
                if spent < interval {
                    std::thread::sleep(interval - spent);
                }
            }

            bridge_bg.is_sharing.store(false, Ordering::Relaxed);
        });

        Ok(Self {
            stop_flag,
            bridge,
            ws_tx,
            session_token,
            chat_id,
            call_id,
            sender_email,
        })
    }

    pub fn stop(&self) {
        if !self.stop_flag.swap(true, Ordering::Relaxed) {
            self.bridge.is_sharing.store(false, Ordering::Relaxed);
            self.bridge.clear_sender(&self.sender_email);
            let wire = WsWireEvent::CallScreenStop {
                session_token: self.session_token.clone(),
                chat_id: self.chat_id,
                call_id: self.call_id.clone(),
                sender_email: self.sender_email.clone(),
            };
            if let Ok(json) = serde_json::to_string(&wire) {
                let _ = self.ws_tx.send(json);
            }
        }
    }
}

impl Drop for ScreenShareEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone, PartialEq)]
pub enum CallStatus {
    Idle,
    Outgoing {
        chat_id: i64,
        call_id: String,
        peer_name: String,
        peer_tag: String,
        peer_color: Color32,
        started_at: Instant,
    },
    Incoming {
        chat_id: i64,
        call_id: String,
        caller_name: String,
        caller_email: String,
        peer_color: Color32,
        received_at: Instant,
    },
    Connected {
        chat_id: i64,
        call_id: String,
        peer_name: String,
        peer_tag: String,
        peer_color: Color32,
        started_at: Instant,
        handshaking_until: Instant,
        participants: Vec<String>,
    },
}

pub struct RemoteVoiceStream {
    pub samples: VecDeque<f32>,
    pub last_packet: Instant,
}

pub struct SharedAudioBridge {
    pub remote_streams: Mutex<HashMap<String, RemoteVoiceStream>>,
    pub active_chat_id: Mutex<Option<i64>>,
    pub my_email: Mutex<String>,
    pub out_sample_rate: AtomicU32,
    pub speaker_volume: AtomicU32,
    pub mic_gain: AtomicU32,
    pub mic_muted: AtomicBool,
    pub speaker_muted: AtomicBool,
    pub remote_level: AtomicU32,
    pub mic_level: AtomicU32,
}

impl SharedAudioBridge {
    pub fn new() -> Self {
        Self {
            remote_streams: Mutex::new(HashMap::new()),
            active_chat_id: Mutex::new(None),
            my_email: Mutex::new(String::new()),
            out_sample_rate: AtomicU32::new(48_000),
            speaker_volume: AtomicU32::new(1.0_f32.to_bits()),
            mic_gain: AtomicU32::new(1.25_f32.to_bits()),
            mic_muted: AtomicBool::new(false),
            speaker_muted: AtomicBool::new(false),
            remote_level: AtomicU32::new(0.0_f32.to_bits()),
            mic_level: AtomicU32::new(0.0_f32.to_bits()),
        }
    }

    pub fn set_my_email(&self, email: String) {
        if let Ok(mut guard) = self.my_email.lock() {
            *guard = email;
        }
    }

    pub fn remove_sender(&self, email: &str) {
        if let Ok(mut streams) = self.remote_streams.lock() {
            streams.remove(email);
        }
    }

    pub fn set_active_chat(&self, chat_id: Option<i64>) {
        if let Ok(mut guard) = self.active_chat_id.lock() {
            *guard = chat_id;
        }
        if chat_id.is_none() {
            if let Ok(mut streams) = self.remote_streams.lock() {
                streams.clear();
            }
            self.remote_level.store(0.0_f32.to_bits(), Ordering::Relaxed);
            self.mic_level.store(0.0_f32.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn speaker_volume_f32(&self) -> f32 {
        f32::from_bits(self.speaker_volume.load(Ordering::Relaxed))
    }

    pub fn set_speaker_volume(&self, v: f32) {
        self.speaker_volume
            .store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    pub fn mic_gain_f32(&self) -> f32 {
        f32::from_bits(self.mic_gain.load(Ordering::Relaxed))
    }

    pub fn set_mic_gain(&self, v: f32) {
        self.mic_gain
            .store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    pub fn remote_level_f32(&self) -> f32 {
        f32::from_bits(self.remote_level.load(Ordering::Relaxed))
    }

    pub fn mic_level_f32(&self) -> f32 {
        f32::from_bits(self.mic_level.load(Ordering::Relaxed))
    }

    pub fn mix_next_sample(streams: &mut HashMap<String, RemoteVoiceStream>) -> f32 {
        let mut sum = 0.0_f32;
        let mut active = 0_usize;
        for st in streams.values_mut() {
            if let Some(s) = st.samples.pop_front() {
                sum += s;
                active += 1;
            }
        }
        if active > 1 {
            sum / (active as f32).sqrt()
        } else {
            sum
        }
    }

    pub fn push_remote_audio(&self, chat_id: i64, sender_email: &str, src_rate: u32, pcm_b64: &str) {
        if sender_email.is_empty() {
            return;
        }
        if let Ok(me) = self.my_email.lock() {
            if !me.is_empty() && *me == sender_email {
                return;
            }
        }
        if let Ok(active) = self.active_chat_id.lock() {
            if *active != Some(chat_id) {
                return;
            }
        }

        let Ok(encoded_bytes) = B64.decode(pcm_b64) else {
            return;
        };
        let Some(raw_bytes) = decrypt_audio_frame(chat_id, &encoded_bytes) else {
            return;
        };
        if raw_bytes.len() < 2 {
            return;
        }

        let mut mono = Vec::with_capacity(raw_bytes.len() / 2);
        let mut energy = 0.0_f32;
        for chunk in raw_bytes.chunks_exact(2) {
            let v = i16::from_le_bytes([chunk[0], chunk[1]]);
            let f = v as f32 / i16::MAX as f32;
            energy += f * f;
            mono.push(f);
        }
        if mono.is_empty() {
            return;
        }

        let rms = (energy / mono.len() as f32).sqrt().min(1.0);
        let prev = self.remote_level_f32();
        let smoothed = prev * 0.45 + (rms * 3.2).min(1.0) * 0.55;
        self.remote_level
            .store(smoothed.to_bits(), Ordering::Relaxed);

        let src_rate = src_rate.max(8_000);
        let dst_rate = self.out_sample_rate.load(Ordering::Relaxed).max(8_000);
        let now = Instant::now();

        if let Ok(mut streams) = self.remote_streams.lock() {
            streams.retain(|_, st| now.duration_since(st.last_packet) < Duration::from_millis(1500));

            if !streams.contains_key(sender_email) && streams.len() >= MAX_CALL_PARTICIPANTS {
                return;
            }

            let entry = streams
                .entry(sender_email.to_string())
                .or_insert_with(|| RemoteVoiceStream {
                    samples: VecDeque::with_capacity(16_000),
                    last_packet: now,
                });
            entry.last_packet = now;

            let max_samples = (dst_rate as usize) / 4;
            if entry.samples.len() > max_samples {
                let drop_count = entry.samples.len() - max_samples / 2;
                entry.samples.drain(..drop_count);
            }

            if src_rate == dst_rate {
                entry.samples.extend(mono);
            } else {
                let ratio = src_rate as f64 / dst_rate as f64;
                let out_len = ((mono.len() as f64) / ratio).floor() as usize;
                for i in 0..out_len {
                    let pos = i as f64 * ratio;
                    let idx = pos.floor() as usize;
                    let frac = (pos - idx as f64) as f32;
                    let s0 = mono[idx.min(mono.len() - 1)];
                    let s1 = mono[(idx + 1).min(mono.len() - 1)];
                    entry.samples.push_back(s0 + (s1 - s0) * frac);
                }
            }
        }
    }
}

pub struct VoiceEngine {
    stop_flag: Arc<AtomicBool>,
    bridge: Arc<SharedAudioBridge>,
}

impl VoiceEngine {
    pub fn start(
        ws_tx: Sender<String>,
        session_token: String,
        chat_id: i64,
        call_id: String,
        sender_email: String,
        bridge: Arc<SharedAudioBridge>,
    ) -> Result<Self, String> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        bridge.set_active_chat(Some(chat_id));

        let host = cpal::default_host();
        let in_device = host
            .default_input_device()
            .ok_or_else(|| "Микрофон не найден".to_string())?;
        let out_device = host
            .default_output_device()
            .ok_or_else(|| "Динамики/наушники не найдены".to_string())?;

        let in_config = in_device
            .default_input_config()
            .map_err(|e| format!("Ошибка конфига микрофона: {}", e))?;
        let out_config = out_device
            .default_output_config()
            .map_err(|e| format!("Ошибка конфига вывода звука: {}", e))?;

        bridge
            .out_sample_rate
            .store(out_config.sample_rate().0.max(8_000), Ordering::Relaxed);

        let stop_in_cb = stop_flag.clone();
        let stop_in_loop = stop_flag.clone();
        let bridge_in = bridge.clone();

        std::thread::spawn(move || {
            let in_rate = in_config.sample_rate().0.max(8_000);
            let in_channels = in_config.channels().max(1) as usize;
            let sample_format = in_config.sample_format();
            let stream_cfg: cpal::StreamConfig = in_config.into();

            let mut mono_accum: Vec<f32> = Vec::with_capacity(4096);
            let mut last_send = Instant::now();
            let mut gate_hangover: u8 = 0;

            let mut push_mono_samples = move |mono: &[f32]| {
                if stop_in_cb.load(Ordering::Relaxed) {
                    return;
                }
                if bridge_in.mic_muted.load(Ordering::Relaxed) {
                    mono_accum.clear();
                    gate_hangover = 0;
                    bridge_in
                        .mic_level
                        .store(0.0_f32.to_bits(), Ordering::Relaxed);
                    return;
                }

                let gain = bridge_in.mic_gain_f32();
                if in_rate == VOICE_TARGET_RATE {
                    for &s in mono {
                        mono_accum.push((s * gain).clamp(-1.0, 1.0));
                    }
                } else if !mono.is_empty() {
                    let ratio = in_rate as f64 / VOICE_TARGET_RATE as f64;
                    let out_len = ((mono.len() as f64) / ratio).floor() as usize;
                    for i in 0..out_len {
                        let pos = i as f64 * ratio;
                        let idx = pos.floor() as usize;
                        let frac = (pos - idx as f64) as f32;
                        let s0 = mono[idx.min(mono.len() - 1)];
                        let s1 = mono[(idx + 1).min(mono.len() - 1)];
                        let val = ((s0 + (s1 - s0) * frac) * gain).clamp(-1.0, 1.0);
                        mono_accum.push(val);
                    }
                }

                while mono_accum.len() >= 640 {
                    let chunk: Vec<f32> = mono_accum.drain(..640).collect();
                    let mut energy = 0.0_f32;
                    for &s in &chunk {
                        energy += s * s;
                    }

                    let rms = (energy / 640.0_f32).sqrt().min(1.0);
                    let prev = bridge_in.mic_level_f32();
                    let smoothed = prev * 0.4 + (rms * 3.2).min(1.0) * 0.6;
                    bridge_in
                        .mic_level
                        .store(smoothed.to_bits(), Ordering::Relaxed);

                    if rms >= VOICE_GATE_RMS {
                        gate_hangover = VOICE_HANGOVER_FRAMES;
                    } else if gate_hangover > 0 {
                        gate_hangover -= 1;
                    } else {
                        continue;
                    }

                    let mut pcm_bytes = Vec::with_capacity(chunk.len() * 2);
                    for s in chunk {
                        let val = (s * i16::MAX as f32) as i16;
                        pcm_bytes.extend_from_slice(&val.to_le_bytes());
                    }

                    let encrypted_pcm = encrypt_audio_frame(chat_id, &pcm_bytes);

                    let wire = WsWireEvent::CallAudio {
                        session_token: session_token.clone(),
                        chat_id,
                        call_id: call_id.clone(),
                        sender_email: sender_email.clone(),
                        sample_rate: VOICE_TARGET_RATE,
                        channels: 1,
                        pcm_b64: B64.encode(&encrypted_pcm),
                    };
                    if let Ok(json) = serde_json::to_string(&wire) {
                        let _ = ws_tx.send(json);
                    }
                    last_send = Instant::now();
                }

                if last_send.elapsed().as_secs() > 30 {
                    mono_accum.clear();
                }
            };

            let err_fn = |err| eprintln!("cpal input stream error: {}", err);

            let stream_res = match sample_format {
                cpal::SampleFormat::F32 => in_device.build_input_stream(
                    &stream_cfg,
                    move |data: &[f32], _| {
                        let mut mono = Vec::with_capacity(data.len() / in_channels + 1);
                        for frame in data.chunks(in_channels) {
                            let sum: f32 = frame.iter().copied().sum();
                            mono.push(sum / in_channels as f32);
                        }
                        push_mono_samples(&mono);
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => in_device.build_input_stream(
                    &stream_cfg,
                    move |data: &[i16], _| {
                        let mut mono = Vec::with_capacity(data.len() / in_channels + 1);
                        for frame in data.chunks(in_channels) {
                            let sum: f32 = frame
                                .iter()
                                .map(|&v| v as f32 / i16::MAX as f32)
                                .sum();
                            mono.push(sum / in_channels as f32);
                        }
                        push_mono_samples(&mono);
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => in_device.build_input_stream(
                    &stream_cfg,
                    move |data: &[u16], _| {
                        let mut mono = Vec::with_capacity(data.len() / in_channels + 1);
                        for frame in data.chunks(in_channels) {
                            let sum: f32 = frame
                                .iter()
                                .map(|&v| (v as f32 - 32768.0) / 32768.0)
                                .sum();
                            mono.push(sum / in_channels as f32);
                        }
                        push_mono_samples(&mono);
                    },
                    err_fn,
                    None,
                ),
                _ => return,
            };

            if let Ok(stream) = stream_res {
                let _ = stream.play();
                while !stop_in_loop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });

        let stop_out_loop = stop_flag.clone();
        let bridge_out = bridge.clone();
        std::thread::spawn(move || {
            let out_channels = out_config.channels().max(1) as usize;
            let sample_format = out_config.sample_format();
            let stream_cfg: cpal::StreamConfig = out_config.into();
            let err_fn = |err| eprintln!("cpal output stream error: {}", err);

            let stream_res = match sample_format {
                cpal::SampleFormat::F32 => {
                    let b = bridge_out.clone();
                    out_device.build_output_stream(
                        &stream_cfg,
                        move |data: &mut [f32], _| {
                            let muted = b.speaker_muted.load(Ordering::Relaxed);
                            let vol = if muted { 0.0 } else { b.speaker_volume_f32() };
                            let Ok(mut streams) = b.remote_streams.lock() else {
                                data.fill(0.0);
                                return;
                            };
                            let now = Instant::now();
                            streams.retain(|_, st| {
                                !st.samples.is_empty()
                                    || now.duration_since(st.last_packet)
                                        < Duration::from_millis(900)
                            });
                            if streams.is_empty() {
                                let decayed = b.remote_level_f32() * 0.85;
                                b.remote_level.store(decayed.to_bits(), Ordering::Relaxed);
                            }
                            for frame in data.chunks_mut(out_channels) {
                                let s = soft_clip(
                                    SharedAudioBridge::mix_next_sample(&mut streams) * vol,
                                )
                                .clamp(-1.0, 1.0);
                                for ch in frame.iter_mut() {
                                    *ch = s;
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                }
                cpal::SampleFormat::I16 => {
                    let b = bridge_out.clone();
                    out_device.build_output_stream(
                        &stream_cfg,
                        move |data: &mut [i16], _| {
                            let muted = b.speaker_muted.load(Ordering::Relaxed);
                            let vol = if muted { 0.0 } else { b.speaker_volume_f32() };
                            let Ok(mut streams) = b.remote_streams.lock() else {
                                data.fill(0);
                                return;
                            };
                            let now = Instant::now();
                            streams.retain(|_, st| {
                                !st.samples.is_empty()
                                    || now.duration_since(st.last_packet)
                                        < Duration::from_millis(900)
                            });
                            if streams.is_empty() {
                                let decayed = b.remote_level_f32() * 0.85;
                                b.remote_level.store(decayed.to_bits(), Ordering::Relaxed);
                            }
                            for frame in data.chunks_mut(out_channels) {
                                let s = soft_clip(
                                    SharedAudioBridge::mix_next_sample(&mut streams) * vol,
                                )
                                .clamp(-1.0, 1.0);
                                let val = (s * i16::MAX as f32) as i16;
                                for ch in frame.iter_mut() {
                                    *ch = val;
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                }
                cpal::SampleFormat::U16 => {
                    let b = bridge_out.clone();
                    out_device.build_output_stream(
                        &stream_cfg,
                        move |data: &mut [u16], _| {
                            let muted = b.speaker_muted.load(Ordering::Relaxed);
                            let vol = if muted { 0.0 } else { b.speaker_volume_f32() };
                            let Ok(mut streams) = b.remote_streams.lock() else {
                                data.fill(32768);
                                return;
                            };
                            let now = Instant::now();
                            streams.retain(|_, st| {
                                !st.samples.is_empty()
                                    || now.duration_since(st.last_packet)
                                        < Duration::from_millis(900)
                            });
                            if streams.is_empty() {
                                let decayed = b.remote_level_f32() * 0.85;
                                b.remote_level.store(decayed.to_bits(), Ordering::Relaxed);
                            }
                            for frame in data.chunks_mut(out_channels) {
                                let s = soft_clip(
                                    SharedAudioBridge::mix_next_sample(&mut streams) * vol,
                                )
                                .clamp(-1.0, 1.0);
                                let val = ((s * 32767.0) + 32768.0) as u16;
                                for ch in frame.iter_mut() {
                                    *ch = val;
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                }
                _ => return,
            };

            if let Ok(stream) = stream_res {
                let _ = stream.play();
                while !stop_out_loop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        });

        Ok(Self { stop_flag, bridge })
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.bridge.set_active_chat(None);
    }
}

impl Drop for VoiceEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Deserialize)]
pub struct StartAuthResponse {
    pub state: String,
    pub auth_url: String,
}

#[derive(Deserialize)]
pub struct PollResponse {
    pub status: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub session_token: Option<String>,
    pub avatar_file_id: Option<String>,
    pub bio: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredAttachment {
    pub kind: String,
    pub title: String,
    pub subtitle: String,
    pub drive_file_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ReactionDto {
    pub emoji: String,
    pub users: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessageDto {
    pub id: i64,
    pub chat_id: i64,
    pub sender_email: String,
    pub sender_name: String,
    pub text: String,
    pub time: String,
    pub encrypted: bool,
    pub is_read: bool,
    #[serde(default)]
    pub reply_to_id: Option<i64>,
    #[serde(default)]
    pub reply_to_sender: Option<String>,
    #[serde(default)]
    pub reply_to_preview: Option<String>,
    #[serde(default)]
    pub reactions: Vec<ReactionDto>,
    pub attachment: Option<StoredAttachment>,
}

#[derive(Clone)]
pub struct ReplyTarget {
    pub message_id: Option<i64>,
    pub sender: String,
    pub preview: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GroupMemberDto {
    pub name: String,
    pub tag: String,
    pub email: String,
    pub avatar_file_id: Option<String>,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub online: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatDto {
    pub id: i64,
    #[serde(default)]
    pub invite_code: String,
    pub title: String,
    pub subtitle: String,
    pub avatar_tag: String,
    pub is_group: bool,
    pub description: String,
    pub avatar_file_id: Option<String>,
    pub peer_email: Option<String>,
    pub peer_avatar_id: Option<String>,
    #[serde(default)]
    pub peer_bio: String,
    #[serde(default)]
    pub peer_online: bool,
    #[serde(default)]
    pub creator_email: Option<String>,
    #[serde(default)]
    pub members: Vec<GroupMemberDto>,
    pub messages: Vec<ChatMessageDto>,
}

#[derive(Clone, Deserialize)]
pub struct UserSearchDto {
    pub name: String,
    pub tag: String,
    pub email: String,
    pub avatar_file_id: Option<String>,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub online: bool,
}

#[derive(Deserialize)]
pub struct SearchUsersResponse {
    pub users: Vec<UserSearchDto>,
    #[serde(default)]
    pub groups: Vec<ChatDto>,
}

#[derive(Serialize)]
pub struct CreateChatRequest {
    pub session_token: String,
    pub title: String,
    pub description: String,
    pub avatar_tag: String,
    pub avatar_file_id: Option<String>,
    pub member_emails: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct InviteMemberRequest {
    pub session_token: String,
    pub chat_id: i64,
    pub target_query: String,
}

#[derive(Deserialize)]
pub struct InviteMemberResponse {
    pub chat: ChatDto,
    pub added_member: GroupMemberDto,
}

#[derive(Serialize)]
pub struct JoinChatRequest {
    pub session_token: String,
    pub chat_id: i64,
}

#[derive(Serialize)]
pub struct OpenDmRequest {
    pub session_token: String,
    pub target_email: String,
}

#[derive(Deserialize)]
pub struct SyncResponse {
    pub chats: Vec<ChatDto>,
    #[serde(default)]
    pub online_emails: Vec<String>,
}

#[derive(Deserialize)]
pub struct UploadResponse {
    pub drive_file_id: String,
    pub title: String,
    pub subtitle: String,
    pub kind: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsWireEvent {
    #[serde(rename = "auth_ws")]
    AuthWs { session_token: String },
    #[serde(rename = "presence_update")]
    PresenceUpdate {
        #[serde(default)]
        email: String,
        #[serde(default)]
        online: bool,
        #[serde(default)]
        online_emails: Vec<String>,
    },
    #[serde(rename = "profile_updated")]
    ProfileUpdated {
        #[serde(default)]
        session_token: String,
        #[serde(default)]
        email: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        tag: String,
        #[serde(default)]
        bio: String,
    },
    #[serde(rename = "send_message")]
    SendMessage {
        session_token: String,
        chat_id: i64,
        text: String,
        encrypted: Option<bool>,
        #[serde(default)]
        reply_to_id: Option<i64>,
        #[serde(default)]
        reply_to_sender: Option<String>,
        #[serde(default)]
        reply_to_preview: Option<String>,
        attachment: Option<StoredAttachment>,
    },
    #[serde(rename = "new_message")]
    NewMessage { message: ChatMessageDto },
    #[serde(rename = "toggle_reaction")]
    ToggleReaction {
        session_token: String,
        chat_id: i64,
        message_id: i64,
        emoji: String,
    },
    #[serde(rename = "reaction_updated")]
    ReactionUpdated {
        chat_id: i64,
        message_id: i64,
        reactions: Vec<ReactionDto>,
    },
    #[serde(rename = "new_chat")]
    NewChat {
        chat: ChatDto,
        allowed_emails: Vec<String>,
    },
    #[serde(rename = "delete_group")]
    DeleteGroup {
        session_token: String,
        chat_id: i64,
    },
    #[serde(rename = "chat_deleted")]
    ChatDeleted { chat_id: i64 },
    #[serde(rename = "mark_read")]
    MarkRead {
        session_token: String,
        chat_id: i64,
    },
    #[serde(rename = "messages_read")]
    MessagesRead {
        chat_id: i64,
        reader_email: String,
    },
    #[serde(rename = "delete_message")]
    DeleteMessage {
        session_token: String,
        chat_id: i64,
        message_id: i64,
        for_everyone: bool,
    },
    #[serde(rename = "message_deleted")]
    MessageDeleted { chat_id: i64, message_id: i64 },
    #[serde(rename = "avatar_updated")]
    AvatarUpdated {
        #[serde(default)]
        session_token: String,
        #[serde(default)]
        email: String,
        avatar_file_id: Option<String>,
    },
    #[serde(rename = "group_avatar_updated")]
    GroupAvatarUpdated {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        avatar_file_id: Option<String>,
    },
    #[serde(rename = "call_invite")]
    CallInvite {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        caller_name: String,
        #[serde(default)]
        caller_email: String,
        #[serde(default)]
        participants: Vec<String>,
    },
    #[serde(rename = "call_accept")]
    CallAccept {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        peer_email: String,
        #[serde(default)]
        participants: Vec<String>,
    },
    #[serde(rename = "call_hangup")]
    CallHangup {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        peer_email: String,
        #[serde(default)]
        actor_email: String,
        #[serde(default)]
        participants: Vec<String>,
    },
    #[serde(rename = "call_rejected")]
    CallRejected { chat_id: i64, reason: String },
    #[serde(rename = "call_audio")]
    CallAudio {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        sender_email: String,
        sample_rate: u32,
        #[serde(default = "default_audio_channels")]
        channels: u16,
        pcm_b64: String,
    },
    #[serde(rename = "call_screen_frame")]
    CallScreenFrame {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        sender_email: String,
        #[serde(default)]
        sender_name: String,
        width: u32,
        height: u32,
        jpeg_b64: String,
    },
    #[serde(rename = "call_screen_stop")]
    CallScreenStop {
        #[serde(default)]
        session_token: String,
        chat_id: i64,
        #[serde(default)]
        call_id: String,
        #[serde(default)]
        sender_email: String,
    },
}

pub enum NetEvent {
    AuthUrlReady(String),
    AuthSuccess {
        auth_attempt: u64,
        name: String,
        email: String,
        token: String,
        avatar_file_id: Option<String>,
        bio: String,
    },
    AuthError {
        auth_attempt: u64,
        msg: String,
    },
    InitialSync {
        chats: Vec<ChatDto>,
        online_emails: Vec<String>,
    },
    PresenceUpdate(HashSet<String>),
    ProfileUpdated {
        email: String,
        name: String,
        tag: String,
        bio: String,
    },
    WsStatus(bool),
    IncomingMessage(ChatMessageDto),
    ReactionUpdated {
        chat_id: i64,
        message_id: i64,
        reactions: Vec<ReactionDto>,
    },
    IncomingChat {
        chat: ChatDto,
        allowed_emails: Vec<String>,
    },
    ChatDeleted(i64),
    ChatOpened(ChatDto),
    MemberInvited {
        chat: ChatDto,
        added_member: GroupMemberDto,
    },
    MessagesRead {
        chat_id: i64,
        reader_email: String,
    },
    SearchResults {
        users: Vec<UserSearchDto>,
        groups: Vec<ChatDto>,
    },
    MessageDeleted {
        chat_id: i64,
        message_id: i64,
    },
    FileReady {
        drive_file_id: String,
        local_path: PathBuf,
        open_after: bool,
    },
    AvatarFileReady {
        file_id: String,
        bytes: Vec<u8>,
    },
    UserAvatarUpdated {
        email: String,
        avatar_file_id: Option<String>,
    },
    GroupAvatarUpdated {
        chat_id: i64,
        avatar_file_id: Option<String>,
    },
    CallInvite {
        chat_id: i64,
        call_id: String,
        caller_name: String,
        caller_email: String,
    },
    CallAccept {
        chat_id: i64,
        call_id: String,
        peer_email: String,
        participants: Vec<String>,
    },
    CallHangup {
        chat_id: i64,
        from_email: String,
        participants: Vec<String>,
    },
    CallRejected {
        chat_id: i64,
        reason: String,
    },
    CallScreenStop {
        chat_id: i64,
        sender_email: String,
    },
    Toast(String),
}

#[derive(Clone)]
pub struct ViewedProfile {
    pub name: String,
    pub tag: String,
    pub email: String,
    pub bio: String,
    pub online: bool,
    pub avatar_file_id: Option<String>,
    pub avatar_tex: Option<egui::TextureHandle>,
    pub avatar_color: Color32,
    pub is_me: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum AttachmentKind {
    Media,
    Document,
    Poll,
    Article,
    Location,
    Music,
}

impl AttachmentKind {
    pub fn from_server_str(s: &str) -> Self {
        match s {
            "media" => Self::Media,
            "poll" => Self::Poll,
            "article" => Self::Article,
            "location" => Self::Location,
            "music" => Self::Music,
            _ => Self::Document,
        }
    }

    pub fn to_server_str(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Document => "document",
            Self::Poll => "poll",
            Self::Article => "article",
            Self::Location => "location",
            Self::Music => "music",
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Self::Media => "МЕДИА",
            Self::Document => "ФАЙЛ",
            Self::Poll => "ОПРОС",
            Self::Article => "СТАТЬЯ",
            Self::Location => "ГЕО",
            Self::Music => "АУДИО",
        }
    }

    pub fn badge_color(self) -> Color32 {
        match self {
            Self::Media => Color32::from_rgb(56, 138, 224),
            Self::Document => Color32::from_rgb(70, 118, 198),
            Self::Poll => Color32::from_rgb(214, 140, 48),
            Self::Article => Color32::from_rgb(122, 96, 206),
            Self::Location => Color32::from_rgb(52, 168, 118),
            Self::Music => Color32::from_rgb(206, 76, 128),
        }
    }
}

#[derive(Clone)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub title: String,
    pub subtitle: String,
    pub local_path: Option<PathBuf>,
    pub drive_file_id: Option<String>,
}

impl Attachment {
    pub fn from_path(path: PathBuf, forced_kind: Option<AttachmentKind>) -> Self {
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        let size_str = std::fs::metadata(&path)
            .map(|m| format_file_size(m.len()))
            .unwrap_or_else(|_| "Файл".into());

        let kind = forced_kind.unwrap_or_else(|| {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            match ext.as_str() {
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "mp4" | "mov" | "mkv" => {
                    AttachmentKind::Media
                }
                "mp3" | "wav" | "flac" | "ogg" | "m4a" => AttachmentKind::Music,
                _ => AttachmentKind::Document,
            }
        });

        Self {
            kind,
            title,
            subtitle: size_str,
            local_path: Some(path),
            drive_file_id: None,
        }
    }

    pub fn from_server(s: StoredAttachment) -> Self {
        let cached_path = s.drive_file_id.as_ref().and_then(|id| {
            let safe_name = s.title.replace(['/', '\\', ':'], "_");
            let p = drive_cache_dir().join(format!("{}_{}", id, safe_name));
            if p.exists() {
                Some(p)
            } else {
                None
            }
        });

        Self {
            kind: AttachmentKind::from_server_str(&s.kind),
            title: s.title,
            subtitle: s.subtitle,
            local_path: cached_path,
            drive_file_id: s.drive_file_id,
        }
    }

    pub fn to_stored(&self, chat_id: i64, encrypt: bool) -> StoredAttachment {
        let (title, subtitle) = if encrypt {
            (
                crate::addons::encrypt_payload(chat_id, &self.title),
                crate::addons::encrypt_payload(chat_id, &self.subtitle),
            )
        } else {
            (self.title.clone(), self.subtitle.clone())
        };
        StoredAttachment {
            kind: self.kind.to_server_str().to_string(),
            title,
            subtitle,
            drive_file_id: self.drive_file_id.clone(),
        }
    }
}

#[derive(Clone)]
pub struct Message {
    pub server_id: Option<i64>,
    pub sender: String,
    pub text: String,
    pub time: String,
    pub is_me: bool,
    pub encrypted: bool,
    pub is_read: bool,
    pub created_at: Instant,
    pub attachment: Option<Attachment>,
    pub reply_to_id: Option<i64>,
    pub reply_to_sender: Option<String>,
    pub reply_to_preview: Option<String>,
    pub reactions: Vec<ReactionDto>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum GroupRole {
    Owner,
    Admin,
    Member,
}

impl GroupRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Owner => "Владелец",
            Self::Admin => "Администратор",
            Self::Member => "Участник",
        }
    }
}

#[derive(Clone, Copy)]
pub struct AdminRights {
    pub change_info: bool,
    pub delete_messages: bool,
    pub ban_users: bool,
    pub invite_users: bool,
    pub pin_messages: bool,
    pub manage_voice: bool,
    pub add_admins: bool,
}

impl AdminRights {
    pub fn all() -> Self {
        Self {
            change_info: true,
            delete_messages: true,
            ban_users: true,
            invite_users: true,
            pin_messages: true,
            manage_voice: true,
            add_admins: true,
        }
    }

    pub fn default_admin() -> Self {
        Self {
            change_info: true,
            delete_messages: true,
            ban_users: true,
            invite_users: true,
            pin_messages: true,
            manage_voice: true,
            add_admins: false,
        }
    }

    pub fn none() -> Self {
        Self {
            change_info: false,
            delete_messages: false,
            ban_users: false,
            invite_users: false,
            pin_messages: false,
            manage_voice: false,
            add_admins: false,
        }
    }
}

#[derive(Clone)]
pub struct GroupMember {
    pub name: String,
    pub tag: String,
    pub email: Option<String>,
    pub bio: String,
    pub avatar_file_id: Option<String>,
    pub avatar_tex: Option<egui::TextureHandle>,
    pub role: GroupRole,
    pub custom_title: String,
    pub avatar_color: Color32,
    pub online: bool,
    pub rights: AdminRights,
}

#[derive(Clone, Copy)]
pub struct GroupPermissions {
    pub send_messages: bool,
    pub send_media: bool,
    pub send_polls: bool,
    pub invite_members: bool,
    pub slow_mode_secs: u32,
}

impl Default for GroupPermissions {
    fn default() -> Self {
        Self {
            send_messages: true,
            send_media: true,
            send_polls: true,
            invite_members: true,
            slow_mode_secs: 0,
        }
    }
}

pub struct Chat {
    pub server_chat_id: Option<i64>,
    pub invite_code: String,
    pub title: String,
    pub subtitle: String,
    pub avatar_tag: String,
    pub avatar_color: Color32,
    pub avatar_tex: Option<egui::TextureHandle>,
    pub avatar_file_id: Option<String>,
    pub peer_avatar_id: Option<String>,
    pub peer_bio: String,
    pub peer_online: bool,
    pub unread: u32,
    pub is_group: bool,
    pub encrypted: bool,
    pub peer_email: Option<String>,
    pub description: String,
    pub is_public: bool,
    pub invite_link: String,
    pub members: Vec<GroupMember>,
    pub permissions: GroupPermissions,
    pub messages: Vec<Message>,
}

impl Chat {
    pub fn refresh_group_subtitle(&mut self) {
        if self.is_group && !self.members.is_empty() {
            let total = self.members.len();
            let online = self.members.iter().filter(|m| m.online).count();
            self.subtitle = format!("{} участн., {} в сети", total, online);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum GroupTab {
    Info,
    Members,
    Admins,
    Permissions,
}

#[derive(PartialEq)]
pub enum ActiveModal {
    None,
    Profile,
    UserProfile,
    CreateGroup,
    ManageGroup(usize),
    CreatePoll,
    ForwardMessage { chat_idx: usize, msg_idx: usize },
}

pub struct EgrammApp {
    pub reply_target: Option<ReplyTarget>,
    pub is_dark_mode: bool,
    pub theme_anim: f32,

    pub is_logged_in: bool,
    pub auth_anim_start: Instant,
    pub is_authenticating: bool,
    pub auth_attempt: u64,
    pub auth_url: Option<String>,
    pub auth_error: Option<String>,
    pub session_token: Option<String>,

    pub net_tx: Sender<NetEvent>,
    pub net_rx: Receiver<NetEvent>,
    pub ws_out_tx: Option<Sender<String>>,
    pub ws_connected: bool,
    pub ws_loop_running: Arc<AtomicBool>,
    pub online_emails: HashSet<String>,
    pub downloading_drive_ids: HashSet<String>,
    pub fetching_avatars: HashSet<String>,
    pub avatar_tex_cache: HashMap<String, egui::TextureHandle>,

    pub call_status: CallStatus,
    pub call_window_expanded: bool,
    pub audio_bridge: Arc<SharedAudioBridge>,
    pub voice_engine: Option<VoiceEngine>,
    pub screen_bridge: Arc<SharedScreenBridge>,
    pub screen_engine: Option<ScreenShareEngine>,
    pub screen_tex: Option<egui::TextureHandle>,
    pub screen_last_seq: u64,
    pub screen_streamer_email: String,
    pub screen_streamer_name: String,
    pub screen_frame_size: (usize, usize),
    pub screen_watching: bool,
    pub screen_fullscreen: bool,
    pub sound_player: SoundPlayer,

    pub has_played_intro: bool,
    pub intro_start: Option<Instant>,

    pub username: String,
    pub user_tag: String,
    pub user_email: String,
    pub user_bio: String,
    pub user_avatar: Option<egui::TextureHandle>,
    pub user_avatar_id: Option<String>,
    pub profile_editing: bool,
    pub viewed_profile: Option<ViewedProfile>,

    pub chats: Vec<Chat>,
    pub selected_chat: Option<usize>,
    pub chat_switch_time: Instant,
    pub search_query: String,
    pub last_searched_query: String,
    pub user_search_results: Vec<UserSearchDto>,
    pub group_search_results: Vec<ChatDto>,
    pub input_text: String,

    pub attach_menu_open: bool,
    pub attach_btn_rect: Rect,
    pub pending_attachment: Option<Attachment>,

    pub image_cache: HashMap<PathBuf, Option<egui::TextureHandle>>,
    pub msg_context_menu: Option<(usize, usize, Pos2)>,

    pub lightbox_image: Option<PathBuf>,
    pub lightbox_zoom: f32,
    pub lightbox_offset: Vec2,
    pub lightbox_open_time: Instant,

    pub toast: Option<(String, Instant)>,

    pub menu_open: bool,
    pub active_modal: ActiveModal,

    pub new_group_name: String,
    pub new_group_desc: String,
    pub new_group_public: bool,
    pub new_group_link: String,
    pub new_group_color_idx: usize,
    pub new_group_avatar: Option<egui::TextureHandle>,
    pub new_group_avatar_path: Option<PathBuf>,

    pub group_tab: GroupTab,
    pub editing_admin_idx: Option<usize>,
    pub add_member_input: String,

    pub poll_question: String,
    pub poll_option_1: String,
    pub poll_option_2: String,
}