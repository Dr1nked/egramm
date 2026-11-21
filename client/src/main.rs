#![windows_subsystem = "windows"]

mod addons;
use addons::{
    decrypt_audio_frame, decrypt_payload, derive_chat_key, encrypt_audio_frame, encrypt_payload,
    SoundPlayer,
};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Rounding, Stroke,
    Vec2,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SERVER_URL: &str = "https://egramm.online";
const WS_URL: &str = "wss://egramm.online/ws";
const VOICE_TARGET_RATE: u32 = 16_000;
const MAX_CALL_PARTICIPANTS: usize = 4;
const VOICE_GATE_RMS: f32 = 0.008;
const VOICE_HANGOVER_FRAMES: u8 = 8;

const AUTH_BLUE: Color32 = Color32::from_rgb(43, 135, 232);
const AUTH_BLUE_HOVER: Color32 = Color32::from_rgb(31, 118, 214);
const DANGER_RED: Color32 = Color32::from_rgb(235, 95, 95);
const ONLINE_GREEN: Color32 = Color32::from_rgb(68, 198, 122);
const READ_TICK_BLUE: Color32 = Color32::from_rgb(82, 188, 255);

const GROUP_COLORS: [Color32; 6] = [
    Color32::from_rgb(58, 128, 210),
    Color32::from_rgb(112, 86, 198),
    Color32::from_rgb(46, 148, 130),
    Color32::from_rgb(214, 122, 54),
    Color32::from_rgb(198, 72, 116),
    Color32::from_rgb(76, 152, 216),
];

fn default_audio_channels() -> u16 {
    1
}

fn soft_clip(x: f32) -> f32 {
    if x.abs() > 0.85 {
        x.tanh()
    } else {
        x
    }
}

fn generate_call_id() -> String {
    let mut buf = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

fn call_encryption_fingerprint(chat_id: i64, call_id: &str) -> [(u8, Color32); 4] {
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
struct ThemePalette {
    auth_bg: Color32,
    auth_card: Color32,
    auth_border: Color32,
    auth_blue_soft: Color32,
    auth_text: Color32,
    auth_muted: Color32,

    bg_chat: Color32,
    bg_panel: Color32,
    bg_popup: Color32,
    bg_input: Color32,
    bg_hover: Color32,
    bg_selected: Color32,
    bubble_out: Color32,
    bubble_in: Color32,
    card_inner: Color32,
    border_line: Color32,
    popup_border: Color32,
    drawer_header: Color32,
    accent_blue: Color32,
    accent_blue_hover: Color32,
    accent_light: Color32,
    text_main: Color32,
    text_muted: Color32,
    text_on_selected: Color32,
}

impl ThemePalette {
    fn dark() -> Self {
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

    fn light() -> Self {
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

fn apply_theme_style(ctx: &egui::Context, p: &ThemePalette, is_dark: bool) {
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

fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

fn ease_out_cubic(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    1.0 - (1.0 - c).powi(3)
}

fn ease_out_back(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    let c1 = 1.70158_f32;
    let c3 = c1 + 1.0;
    1.0 + c3 * (c - 1.0).powi(3) + c1 * (c - 1.0).powi(2)
}

fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    let a = (color.a() as f32 * alpha.clamp(0.0, 1.0)) as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

fn format_file_size(bytes: u64) -> String {
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

fn is_image_name(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
}

fn get_or_load_image(
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

fn load_circular_avatar_from_bytes(
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

fn load_circular_avatar(
    ctx: &egui::Context,
    path: &Path,
    prefix: &str,
) -> Option<egui::TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    load_circular_avatar_from_bytes(ctx, &bytes, &format!("{}_{}", prefix, path.to_string_lossy()))
}

fn drive_cache_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("egramm_drive_cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn draw_read_ticks(
    p: &egui::Painter,
    rect: Rect,
    is_read: bool,
    muted_color: Color32,
    alpha: f32,
) {
    let base_col = if is_read {
        READ_TICK_BLUE
    } else {
        muted_color
    };
    let stroke = Stroke::new(1.8, with_alpha(base_col, alpha));
    let cy = rect.center().y;

    if is_read {
        let x1 = rect.min.x + 2.0;
        p.line_segment([Pos2::new(x1, cy), Pos2::new(x1 + 3.5, cy + 3.5)], stroke);
        p.line_segment(
            [Pos2::new(x1 + 3.5, cy + 3.5), Pos2::new(x1 + 9.0, cy - 3.5)],
            stroke,
        );

        let x2 = rect.min.x + 6.5;
        p.line_segment([Pos2::new(x2, cy), Pos2::new(x2 + 3.5, cy + 3.5)], stroke);
        p.line_segment(
            [Pos2::new(x2 + 3.5, cy + 3.5), Pos2::new(x2 + 9.0, cy - 3.5)],
            stroke,
        );
    } else {
        let x1 = rect.min.x + 4.0;
        p.line_segment([Pos2::new(x1, cy), Pos2::new(x1 + 3.8, cy + 3.8)], stroke);
        p.line_segment(
            [Pos2::new(x1 + 3.8, cy + 3.8), Pos2::new(x1 + 9.5, cy - 3.8)],
            stroke,
        );
    }
}

fn notify_windows_message(sender: &str, preview: &str) {
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

#[derive(Clone, PartialEq)]
enum CallStatus {
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

struct RemoteVoiceStream {
    samples: VecDeque<f32>,
    last_packet: Instant,
}

struct SharedAudioBridge {
    remote_streams: Mutex<HashMap<String, RemoteVoiceStream>>,
    active_chat_id: Mutex<Option<i64>>,
    my_email: Mutex<String>,
    out_sample_rate: AtomicU32,
    speaker_volume: AtomicU32,
    mic_gain: AtomicU32,
    mic_muted: AtomicBool,
    speaker_muted: AtomicBool,
    remote_level: AtomicU32,
    mic_level: AtomicU32,
}

impl SharedAudioBridge {
    fn new() -> Self {
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

    fn set_my_email(&self, email: String) {
        if let Ok(mut guard) = self.my_email.lock() {
            *guard = email;
        }
    }

    fn remove_sender(&self, email: &str) {
        if let Ok(mut streams) = self.remote_streams.lock() {
            streams.remove(email);
        }
    }

    fn set_active_chat(&self, chat_id: Option<i64>) {
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

    fn speaker_volume_f32(&self) -> f32 {
        f32::from_bits(self.speaker_volume.load(Ordering::Relaxed))
    }

    fn set_speaker_volume(&self, v: f32) {
        self.speaker_volume
            .store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    fn mic_gain_f32(&self) -> f32 {
        f32::from_bits(self.mic_gain.load(Ordering::Relaxed))
    }

    fn set_mic_gain(&self, v: f32) {
        self.mic_gain
            .store(v.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    fn remote_level_f32(&self) -> f32 {
        f32::from_bits(self.remote_level.load(Ordering::Relaxed))
    }

    fn mic_level_f32(&self) -> f32 {
        f32::from_bits(self.mic_level.load(Ordering::Relaxed))
    }

    fn mix_next_sample(streams: &mut HashMap<String, RemoteVoiceStream>) -> f32 {
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

    fn push_remote_audio(&self, chat_id: i64, sender_email: &str, src_rate: u32, pcm_b64: &str) {
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

struct VoiceEngine {
    stop_flag: Arc<AtomicBool>,
    bridge: Arc<SharedAudioBridge>,
}

impl VoiceEngine {
    fn start(
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

    fn stop(&self) {
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
struct StartAuthResponse {
    state: String,
    auth_url: String,
}

#[derive(Deserialize)]
struct PollResponse {
    status: String,
    name: Option<String>,
    email: Option<String>,
    session_token: Option<String>,
    avatar_file_id: Option<String>,
    bio: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredAttachment {
    kind: String,
    title: String,
    subtitle: String,
    drive_file_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ChatMessageDto {
    id: i64,
    chat_id: i64,
    sender_email: String,
    sender_name: String,
    text: String,
    time: String,
    encrypted: bool,
    is_read: bool,
    attachment: Option<StoredAttachment>,
}

#[derive(Clone, Serialize, Deserialize)]
struct GroupMemberDto {
    name: String,
    tag: String,
    email: String,
    avatar_file_id: Option<String>,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    online: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct ChatDto {
    id: i64,
    #[serde(default)]
    invite_code: String,
    title: String,
    subtitle: String,
    avatar_tag: String,
    is_group: bool,
    description: String,
    avatar_file_id: Option<String>,
    peer_email: Option<String>,
    peer_avatar_id: Option<String>,
    #[serde(default)]
    peer_bio: String,
    #[serde(default)]
    peer_online: bool,
    #[serde(default)]
    members: Vec<GroupMemberDto>,
    messages: Vec<ChatMessageDto>,
}

#[derive(Clone, Deserialize)]
struct UserSearchDto {
    name: String,
    tag: String,
    email: String,
    avatar_file_id: Option<String>,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    online: bool,
}

#[derive(Deserialize)]
struct SearchUsersResponse {
    users: Vec<UserSearchDto>,
    #[serde(default)]
    groups: Vec<ChatDto>,
}

#[derive(Serialize)]
struct CreateChatRequest {
    session_token: String,
    title: String,
    description: String,
    avatar_tag: String,
    avatar_file_id: Option<String>,
    member_emails: Option<Vec<String>>,
}

#[derive(Serialize)]
struct InviteMemberRequest {
    session_token: String,
    chat_id: i64,
    target_query: String,
}

#[derive(Deserialize)]
struct InviteMemberResponse {
    chat: ChatDto,
    added_member: GroupMemberDto,
}

#[derive(Serialize)]
struct JoinChatRequest {
    session_token: String,
    chat_id: i64,
}

#[derive(Serialize)]
struct OpenDmRequest {
    session_token: String,
    target_email: String,
}

#[derive(Deserialize)]
struct SyncResponse {
    chats: Vec<ChatDto>,
    #[serde(default)]
    online_emails: Vec<String>,
}

#[derive(Deserialize)]
struct UploadResponse {
    drive_file_id: String,
    title: String,
    subtitle: String,
    kind: String,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum WsWireEvent {
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
        attachment: Option<StoredAttachment>,
    },
    #[serde(rename = "new_message")]
    NewMessage { message: ChatMessageDto },
    #[serde(rename = "new_chat")]
    NewChat {
        chat: ChatDto,
        allowed_emails: Vec<String>,
    },
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
}

enum NetEvent {
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
    IncomingChat {
        chat: ChatDto,
        allowed_emails: Vec<String>,
    },
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
    Toast(String),
}

#[derive(Clone)]
struct ViewedProfile {
    name: String,
    tag: String,
    email: String,
    bio: String,
    online: bool,
    avatar_file_id: Option<String>,
    avatar_tex: Option<egui::TextureHandle>,
    avatar_color: Color32,
    is_me: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum AttachmentKind {
    Media,
    Document,
    Poll,
    Article,
    Location,
    Music,
}

impl AttachmentKind {
    fn from_server_str(s: &str) -> Self {
        match s {
            "media" => Self::Media,
            "poll" => Self::Poll,
            "article" => Self::Article,
            "location" => Self::Location,
            "music" => Self::Music,
            _ => Self::Document,
        }
    }

    fn to_server_str(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Document => "document",
            Self::Poll => "poll",
            Self::Article => "article",
            Self::Location => "location",
            Self::Music => "music",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Media => "МЕДИА",
            Self::Document => "ФАЙЛ",
            Self::Poll => "ОПРОС",
            Self::Article => "СТАТЬЯ",
            Self::Location => "ГЕО",
            Self::Music => "АУДИО",
        }
    }

    fn badge_color(self) -> Color32 {
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
struct Attachment {
    kind: AttachmentKind,
    title: String,
    subtitle: String,
    local_path: Option<PathBuf>,
    drive_file_id: Option<String>,
}

impl Attachment {
    fn from_path(path: PathBuf, forced_kind: Option<AttachmentKind>) -> Self {
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

    fn from_server(s: StoredAttachment) -> Self {
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

    fn to_stored(&self, chat_id: i64, encrypt: bool) -> StoredAttachment {
        let (title, subtitle) = if encrypt {
            (
                encrypt_payload(chat_id, &self.title),
                encrypt_payload(chat_id, &self.subtitle),
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
struct Message {
    server_id: Option<i64>,
    sender: String,
    text: String,
    time: String,
    is_me: bool,
    encrypted: bool,
    is_read: bool,
    created_at: Instant,
    attachment: Option<Attachment>,
}

#[derive(Clone, Copy, PartialEq)]
enum GroupRole {
    Owner,
    Admin,
    Member,
}

impl GroupRole {
    fn label(self) -> &'static str {
        match self {
            Self::Owner => "Владелец",
            Self::Admin => "Администратор",
            Self::Member => "Участник",
        }
    }
}

#[derive(Clone, Copy)]
struct AdminRights {
    change_info: bool,
    delete_messages: bool,
    ban_users: bool,
    invite_users: bool,
    pin_messages: bool,
    manage_voice: bool,
    add_admins: bool,
}

impl AdminRights {
    fn all() -> Self {
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

    fn default_admin() -> Self {
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

    fn none() -> Self {
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
struct GroupMember {
    name: String,
    tag: String,
    email: Option<String>,
    bio: String,
    avatar_file_id: Option<String>,
    avatar_tex: Option<egui::TextureHandle>,
    role: GroupRole,
    custom_title: String,
    avatar_color: Color32,
    online: bool,
    rights: AdminRights,
}

#[derive(Clone, Copy)]
struct GroupPermissions {
    send_messages: bool,
    send_media: bool,
    send_polls: bool,
    invite_members: bool,
    slow_mode_secs: u32,
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

struct Chat {
    server_chat_id: Option<i64>,
    invite_code: String,
    title: String,
    subtitle: String,
    avatar_tag: String,
    avatar_color: Color32,
    avatar_tex: Option<egui::TextureHandle>,
    avatar_file_id: Option<String>,
    peer_avatar_id: Option<String>,
    peer_bio: String,
    peer_online: bool,
    unread: u32,
    is_group: bool,
    encrypted: bool,
    peer_email: Option<String>,
    description: String,
    is_public: bool,
    invite_link: String,
    members: Vec<GroupMember>,
    permissions: GroupPermissions,
    messages: Vec<Message>,
}

impl Chat {
    fn refresh_group_subtitle(&mut self) {
        if self.is_group && !self.members.is_empty() {
            let total = self.members.len();
            let online = self.members.iter().filter(|m| m.online).count();
            self.subtitle = format!("{} участн., {} в сети", total, online);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GroupTab {
    Info,
    Members,
    Admins,
    Permissions,
}

#[derive(PartialEq)]
enum ActiveModal {
    None,
    Profile,
    UserProfile,
    CreateGroup,
    ManageGroup(usize),
    CreatePoll,
    ForwardMessage { chat_idx: usize, msg_idx: usize },
}

struct EgrammApp {
    is_dark_mode: bool,
    theme_anim: f32,

    is_logged_in: bool,
    auth_anim_start: Instant,
    is_authenticating: bool,
    auth_attempt: u64,
    auth_url: Option<String>,
    auth_error: Option<String>,
    session_token: Option<String>,

    net_tx: Sender<NetEvent>,
    net_rx: Receiver<NetEvent>,
    ws_out_tx: Option<Sender<String>>,
    ws_connected: bool,
    ws_loop_running: Arc<AtomicBool>,
    online_emails: HashSet<String>,
    downloading_drive_ids: HashSet<String>,
    fetching_avatars: HashSet<String>,
    avatar_tex_cache: HashMap<String, egui::TextureHandle>,

    call_status: CallStatus,
    call_window_expanded: bool,
    audio_bridge: Arc<SharedAudioBridge>,
    voice_engine: Option<VoiceEngine>,
    sound_player: SoundPlayer,

    has_played_intro: bool,
    intro_start: Option<Instant>,

    username: String,
    user_tag: String,
    user_email: String,
    user_bio: String,
    user_avatar: Option<egui::TextureHandle>,
    user_avatar_id: Option<String>,
    profile_editing: bool,
    viewed_profile: Option<ViewedProfile>,

    chats: Vec<Chat>,
    selected_chat: Option<usize>,
    chat_switch_time: Instant,
    search_query: String,
    last_searched_query: String,
    user_search_results: Vec<UserSearchDto>,
    group_search_results: Vec<ChatDto>,
    input_text: String,

    attach_menu_open: bool,
    attach_btn_rect: Rect,
    pending_attachment: Option<Attachment>,

    image_cache: HashMap<PathBuf, Option<egui::TextureHandle>>,
    msg_context_menu: Option<(usize, usize, Pos2)>,

    lightbox_image: Option<PathBuf>,
    lightbox_zoom: f32,
    lightbox_offset: Vec2,
    lightbox_open_time: Instant,

    toast: Option<(String, Instant)>,

    menu_open: bool,
    active_modal: ActiveModal,

    new_group_name: String,
    new_group_desc: String,
    new_group_public: bool,
    new_group_link: String,
    new_group_color_idx: usize,
    new_group_avatar: Option<egui::TextureHandle>,
    new_group_avatar_path: Option<PathBuf>,

    group_tab: GroupTab,
    editing_admin_idx: Option<usize>,
    add_member_input: String,

    poll_question: String,
    poll_option_1: String,
    poll_option_2: String,
}

impl Default for EgrammApp {
    fn default() -> Self {
        let now = Instant::now();
        let (net_tx, net_rx) = mpsc::channel();

        Self {
            is_dark_mode: true,
            theme_anim: 1.0,

            is_logged_in: false,
            auth_anim_start: now,
            is_authenticating: false,
            auth_attempt: 0,
            auth_url: None,
            auth_error: None,
            session_token: None,

            net_tx,
            net_rx,
            ws_out_tx: None,
            ws_connected: false,
            ws_loop_running: Arc::new(AtomicBool::new(false)),
            online_emails: HashSet::new(),
            downloading_drive_ids: HashSet::new(),
            fetching_avatars: HashSet::new(),
            avatar_tex_cache: HashMap::new(),

            call_status: CallStatus::Idle,
            call_window_expanded: true,
            audio_bridge: Arc::new(SharedAudioBridge::new()),
            voice_engine: None,
            sound_player: SoundPlayer::new(),

            has_played_intro: false,
            intro_start: None,

            username: "Creator".into(),
            user_tag: "@egramm_dev".into(),
            user_email: String::new(),
            user_bio: String::new(),
            user_avatar: None,
            user_avatar_id: None,
            profile_editing: false,
            viewed_profile: None,

            chats: vec![Chat {
                server_chat_id: None,
                invite_code: String::new(),
                title: "Избранное".into(),
                subtitle: String::new(),
                avatar_tag: "★".into(),
                avatar_color: Color32::from_rgb(58, 128, 210),
                avatar_tex: None,
                avatar_file_id: None,
                peer_avatar_id: None,
                peer_bio: String::new(),
                peer_online: false,
                unread: 0,
                is_group: false,
                encrypted: false,
                peer_email: None,
                description: String::new(),
                is_public: false,
                invite_link: String::new(),
                members: vec![],
                permissions: GroupPermissions::default(),
                messages: Vec::new(),
            }],
            selected_chat: Some(0),
            chat_switch_time: now,
            search_query: String::new(),
            last_searched_query: String::new(),
            user_search_results: Vec::new(),
            group_search_results: Vec::new(),
            input_text: String::new(),

            attach_menu_open: false,
            attach_btn_rect: Rect::NOTHING,
            pending_attachment: None,

            image_cache: HashMap::new(),
            msg_context_menu: None,

            lightbox_image: None,
            lightbox_zoom: 1.0,
            lightbox_offset: Vec2::ZERO,
            lightbox_open_time: now,

            toast: None,

            menu_open: false,
            active_modal: ActiveModal::None,

            new_group_name: String::new(),
            new_group_desc: String::new(),
            new_group_public: false,
            new_group_link: String::new(),
            new_group_color_idx: 0,
            new_group_avatar: None,
            new_group_avatar_path: None,

            group_tab: GroupTab::Info,
            editing_admin_idx: None,
            add_member_input: String::new(),

            poll_question: String::new(),
            poll_option_1: String::new(),
            poll_option_2: String::new(),
        }
    }
}
impl EgrammApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = FontDefinitions::default();

        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf")
            .or_else(|_| std::fs::read("C:\\Windows\\Fonts\\arial.ttf"))
        {
            fonts.font_data.insert(
                "egramm_regular".to_owned(),
                FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "egramm_regular".to_owned());
        }

        let mut semibold_list = fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();

        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\segoeuisb.ttf")
            .or_else(|_| std::fs::read("C:\\Windows\\Fonts\\segoeuib.ttf"))
        {
            fonts.font_data.insert(
                "egramm_semibold".to_owned(),
                FontData::from_owned(bytes).into(),
            );
            semibold_list.insert(0, "egramm_semibold".to_owned());
        }
        fonts
            .families
            .insert(FontFamily::Name("semibold".into()), semibold_list);

        cc.egui_ctx.set_fonts(fonts);

        let app = Self::default();
        let palette = app.palette();
        apply_theme_style(&cc.egui_ctx, &palette, app.is_dark_mode);
        app
    }

    fn palette(&self) -> ThemePalette {
        if self.is_dark_mode {
            ThemePalette::dark()
        } else {
            ThemePalette::light()
        }
    }

    fn show_toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn apply_presence_set(&mut self, online: &HashSet<String>) {
        self.online_emails = online.clone();
        for chat in &mut self.chats {
            if chat.is_group {
                for m in &mut chat.members {
                    if let Some(em) = &m.email {
                        m.online = online.contains(em);
                    }
                }
                chat.refresh_group_subtitle();
            } else if let Some(peer_em) = &chat.peer_email {
                let is_on = online.contains(peer_em);
                chat.peer_online = is_on;
                if chat.server_chat_id.is_some() {
                    chat.subtitle = if is_on {
                        "в сети".into()
                    } else {
                        "не в сети".into()
                    };
                }
            }
        }
        for u in &mut self.user_search_results {
            u.online = online.contains(&u.email);
        }
        if let Some(vp) = &mut self.viewed_profile {
            if vp.is_me {
                vp.online = true;
            } else if vp.email.contains('@') {
                vp.online = online.contains(&vp.email);
            }
        }
    }

    fn publish_profile_update(&self) {
        let (Some(token), Some(ws_tx)) = (&self.session_token, &self.ws_out_tx) else {
            return;
        };
        let wire = WsWireEvent::ProfileUpdated {
            session_token: token.clone(),
            email: self.user_email.clone(),
            name: self.username.clone(),
            tag: self.user_tag.clone(),
            bio: self.user_bio.clone(),
        };
        if let Ok(json) = serde_json::to_string(&wire) {
            let _ = ws_tx.send(json);
        }
    }

    fn request_avatar_download(&mut self, ctx: &egui::Context, file_id: &str) {
        if file_id.is_empty()
            || self.avatar_tex_cache.contains_key(file_id)
            || self.fetching_avatars.contains(file_id)
        {
            return;
        }
        let Some(token) = self.session_token.clone() else {
            return;
        };
        self.fetching_avatars.insert(file_id.to_string());
        let fid = file_id.to_string();
        let tx = self.net_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let cached_file = drive_cache_dir().join(format!("avatar_{}", fid));
            if let Ok(bytes) = std::fs::read(&cached_file) {
                let _ = tx.send(NetEvent::AvatarFileReady {
                    file_id: fid,
                    bytes,
                });
                ctx_clone.request_repaint();
                return;
            }

            if let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
            {
                let url = format!("{}/files/{}", SERVER_URL, fid);
                if let Ok(resp) = client
                    .get(&url)
                    .header("x-session-token", &token)
                    .send()
                {
                    if resp.status().is_success() {
                        if let Ok(bytes) = resp.bytes() {
                            let vec_bytes = bytes.to_vec();
                            let _ = std::fs::write(&cached_file, &vec_bytes);
                            let _ = tx.send(NetEvent::AvatarFileReady {
                                file_id: fid,
                                bytes: vec_bytes,
                            });
                            ctx_clone.request_repaint();
                        }
                    }
                }
            }
        });
    }

    fn apply_avatar_textures_to_state(&mut self, ctx: &egui::Context) {
        if let Some(fid) = self.user_avatar_id.clone() {
            self.request_avatar_download(ctx, &fid);
            if let Some(tex) = self.avatar_tex_cache.get(&fid).cloned() {
                self.user_avatar = Some(tex);
            }
        }

        let mut needed: Vec<String> = Vec::new();
        for chat in &self.chats {
            if let Some(fid) = &chat.avatar_file_id {
                needed.push(fid.clone());
            }
            if let Some(fid) = &chat.peer_avatar_id {
                needed.push(fid.clone());
            }
            for m in &chat.members {
                if let Some(fid) = &m.avatar_file_id {
                    needed.push(fid.clone());
                }
            }
        }
        for fid in needed {
            self.request_avatar_download(ctx, &fid);
        }

        for chat in &mut self.chats {
            if let Some(fid) = &chat.avatar_file_id {
                if let Some(tex) = self.avatar_tex_cache.get(fid).cloned() {
                    chat.avatar_tex = Some(tex);
                }
            } else if let Some(fid) = &chat.peer_avatar_id {
                if let Some(tex) = self.avatar_tex_cache.get(fid).cloned() {
                    chat.avatar_tex = Some(tex);
                }
            }
            for m in &mut chat.members {
                if let Some(fid) = &m.avatar_file_id {
                    if let Some(tex) = self.avatar_tex_cache.get(fid).cloned() {
                        m.avatar_tex = Some(tex);
                    }
                }
            }
        }
    }

    fn upload_and_publish_user_avatar(&mut self, ctx: &egui::Context, path: PathBuf) {
        let Some(tex) = load_circular_avatar(ctx, &path, "user_avatar") else {
            self.show_toast("Не удалось открыть изображение");
            return;
        };
        self.user_avatar = Some(tex);
        self.show_toast("Аватарка обновляется на сервере...");

        let Some(token) = self.session_token.clone() else {
            return;
        };
        let my_email = self.user_email.clone();
        let tx = self.net_tx.clone();
        let ws_tx = self.ws_out_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
            else {
                return;
            };

            let form = match reqwest::blocking::multipart::Form::new().file("file", &path) {
                Ok(f) => f,
                Err(_) => return,
            };

            if let Ok(resp) = client
                .post(format!("{}/files/upload", SERVER_URL))
                .header("x-session-token", &token)
                .multipart(form)
                .send()
            {
                if resp.status().is_success() {
                    if let Ok(up) = resp.json::<UploadResponse>() {
                        if let Ok(bytes) = std::fs::read(&path) {
                            let cached =
                                drive_cache_dir().join(format!("avatar_{}", up.drive_file_id));
                            let _ = std::fs::write(&cached, &bytes);
                            let _ = tx.send(NetEvent::AvatarFileReady {
                                file_id: up.drive_file_id.clone(),
                                bytes,
                            });
                        }
                        let _ = tx.send(NetEvent::UserAvatarUpdated {
                            email: my_email.clone(),
                            avatar_file_id: Some(up.drive_file_id.clone()),
                        });
                        if let Some(wtx) = ws_tx {
                            let wire = WsWireEvent::AvatarUpdated {
                                session_token: token,
                                email: my_email,
                                avatar_file_id: Some(up.drive_file_id),
                            };
                            if let Ok(json) = serde_json::to_string(&wire) {
                                let _ = wtx.send(json);
                            }
                        }
                        let _ = tx.send(NetEvent::Toast("Аватарка видна всем в сети".into()));
                        ctx_clone.request_repaint();
                    }
                }
            }
        });
    }

    fn clear_and_publish_user_avatar(&mut self) {
        self.user_avatar = None;
        self.user_avatar_id = None;
        if let (Some(token), Some(ws_tx)) = (&self.session_token, &self.ws_out_tx) {
            let wire = WsWireEvent::AvatarUpdated {
                session_token: token.clone(),
                email: self.user_email.clone(),
                avatar_file_id: None,
            };
            if let Ok(json) = serde_json::to_string(&wire) {
                let _ = ws_tx.send(json);
            }
        }
        self.show_toast("Аватарка сброшена");
    }

    fn upload_and_publish_group_avatar(&mut self, ctx: &egui::Context, g_idx: usize, path: PathBuf) {
        let Some(tex) = load_circular_avatar(ctx, &path, &format!("group_av_{}", g_idx)) else {
            self.show_toast("Не удалось открыть изображение");
            return;
        };
        let Some(chat) = self.chats.get_mut(g_idx) else {
            return;
        };
        chat.avatar_tex = Some(tex);
        let Some(sc_id) = chat.server_chat_id else {
            self.show_toast("Аватарка группы обновлена");
            return;
        };
        self.show_toast("Загрузка аватарки группы...");

        let Some(token) = self.session_token.clone() else {
            return;
        };
        let tx = self.net_tx.clone();
        let ws_tx = self.ws_out_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
            else {
                return;
            };
            let form = match reqwest::blocking::multipart::Form::new().file("file", &path) {
                Ok(f) => f,
                Err(_) => return,
            };
            if let Ok(resp) = client
                .post(format!("{}/files/upload", SERVER_URL))
                .header("x-session-token", &token)
                .multipart(form)
                .send()
            {
                if resp.status().is_success() {
                    if let Ok(up) = resp.json::<UploadResponse>() {
                        if let Ok(bytes) = std::fs::read(&path) {
                            let cached =
                                drive_cache_dir().join(format!("avatar_{}", up.drive_file_id));
                            let _ = std::fs::write(&cached, &bytes);
                            let _ = tx.send(NetEvent::AvatarFileReady {
                                file_id: up.drive_file_id.clone(),
                                bytes,
                            });
                        }
                        let _ = tx.send(NetEvent::GroupAvatarUpdated {
                            chat_id: sc_id,
                            avatar_file_id: Some(up.drive_file_id.clone()),
                        });
                        if let Some(wtx) = ws_tx {
                            let wire = WsWireEvent::GroupAvatarUpdated {
                                session_token: token,
                                chat_id: sc_id,
                                avatar_file_id: Some(up.drive_file_id),
                            };
                            if let Ok(json) = serde_json::to_string(&wire) {
                                let _ = wtx.send(json);
                            }
                        }
                        let _ = tx.send(NetEvent::Toast("Аватарка группы обновлена".into()));
                        ctx_clone.request_repaint();
                    }
                }
            }
        });
    }

    fn open_chat_peer_profile(&mut self, chat_idx: usize) {
        let Some(chat) = self.chats.get(chat_idx) else {
            return;
        };
        if chat.is_group || chat.server_chat_id.is_none() {
            return;
        }
        let email = chat
            .peer_email
            .clone()
            .unwrap_or_else(|| "пользователь".into());
        let nick = email.split('@').next().unwrap_or("user");
        let tag = format!("@{}", nick);
        let is_on = self.online_emails.contains(&email) || chat.peer_online;
        self.viewed_profile = Some(ViewedProfile {
            name: chat.title.clone(),
            tag,
            email: email.clone(),
            bio: chat.peer_bio.clone(),
            online: is_on,
            avatar_file_id: chat.peer_avatar_id.clone(),
            avatar_tex: chat.avatar_tex.clone(),
            avatar_color: chat.avatar_color,
            is_me: email == self.user_email,
        });
        self.active_modal = ActiveModal::UserProfile;
    }

    fn open_member_profile(&mut self, chat_idx: usize, member_idx: usize) {
        let Some(chat) = self.chats.get(chat_idx) else {
            return;
        };
        let Some(m) = chat.members.get(member_idx) else {
            return;
        };
        let email = m
            .email
            .clone()
            .unwrap_or_else(|| format!("{}@egramm.local", m.name.to_lowercase()));
        let is_me = m.tag == self.user_tag || email == self.user_email;
        let is_on = is_me || self.online_emails.contains(&email) || m.online;
        self.viewed_profile = Some(ViewedProfile {
            name: m.name.clone(),
            tag: m.tag.clone(),
            email: email.clone(),
            bio: if is_me {
                self.user_bio.clone()
            } else {
                m.bio.clone()
            },
            online: is_on,
            avatar_file_id: m.avatar_file_id.clone(),
            avatar_tex: m.avatar_tex.clone(),
            avatar_color: m.avatar_color,
            is_me,
        });
        self.active_modal = ActiveModal::UserProfile;
    }

    fn invite_or_add_member_to_group(
        &mut self,
        ctx: &egui::Context,
        g_idx: usize,
        raw_input: String,
    ) {
        let trimmed = raw_input.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        let Some(token) = self.session_token.clone() else {
            self.show_toast("Войдите в аккаунт для приглашения");
            return;
        };
        let Some(sc_id) = self.chats.get(g_idx).and_then(|c| c.server_chat_id) else {
            self.show_toast("Сначала создайте группу на сервере");
            return;
        };

        let net_tx = self.net_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
            else {
                return;
            };

            let body = InviteMemberRequest {
                session_token: token,
                chat_id: sc_id,
                target_query: trimmed,
            };

            match client
                .post(format!("{}/api/chats/invite", SERVER_URL))
                .json(&body)
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(data) = resp.json::<InviteMemberResponse>() {
                        let _ = net_tx.send(NetEvent::Toast(format!(
                            "{} добавлен(а) в группу",
                            data.added_member.name
                        )));
                        let _ = net_tx.send(NetEvent::MemberInvited {
                            chat: data.chat,
                            added_member: data.added_member,
                        });
                        ctx_clone.request_repaint();
                    }
                }
                Ok(resp) => {
                    let err = resp
                        .text()
                        .unwrap_or_else(|_| "Пользователь не найден".into());
                    let _ = net_tx.send(NetEvent::Toast(err));
                    ctx_clone.request_repaint();
                }
                Err(e) => {
                    let _ = net_tx.send(NetEvent::Toast(format!("Ошибка сети: {}", e)));
                    ctx_clone.request_repaint();
                }
            }
        });
    }

    fn join_group_on_server(&mut self, ctx: &egui::Context, grp: ChatDto) {
        let Some(token) = self.session_token.clone() else {
            let _ = self.net_tx.send(NetEvent::ChatOpened(grp));
            return;
        };
        let tx = self.net_tx.clone();
        let ctx_clone = ctx.clone();
        let chat_id = grp.id;

        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
            else {
                let _ = tx.send(NetEvent::ChatOpened(grp));
                return;
            };

            let body = JoinChatRequest {
                session_token: token,
                chat_id,
            };

            match client
                .post(format!("{}/api/chats/join", SERVER_URL))
                .json(&body)
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(joined_dto) = resp.json::<ChatDto>() {
                        let _ = tx.send(NetEvent::ChatOpened(joined_dto));
                        let _ = tx.send(NetEvent::Toast("Вы вступили в группу".into()));
                        ctx_clone.request_repaint();
                        return;
                    }
                }
                _ => {}
            }

            let _ = tx.send(NetEvent::ChatOpened(grp));
            ctx_clone.request_repaint();
        });
    }

    fn start_call_in_chat(&mut self, chat_idx: usize) {
        if self.call_status != CallStatus::Idle {
            self.call_window_expanded = true;
            return;
        }
        let Some(chat) = self.chats.get(chat_idx) else {
            return;
        };
        let (Some(sc_id), Some(token), Some(ws_tx)) =
            (chat.server_chat_id, &self.session_token, &self.ws_out_tx)
        else {
            self.show_toast("Нет подключения к серверу");
            return;
        };

        let call_id = generate_call_id();
        let wire = WsWireEvent::CallInvite {
            session_token: token.clone(),
            chat_id: sc_id,
            call_id: call_id.clone(),
            caller_name: self.username.clone(),
            caller_email: self.user_email.clone(),
            participants: Vec::new(),
        };
        if let Ok(json) = serde_json::to_string(&wire) {
            let _ = ws_tx.send(json);
        }
        self.audio_bridge.mic_muted.store(false, Ordering::Relaxed);
        self.audio_bridge
            .speaker_muted
            .store(false, Ordering::Relaxed);
        self.call_window_expanded = true;
        self.call_status = CallStatus::Outgoing {
            chat_id: sc_id,
            call_id,
            peer_name: chat.title.clone(),
            peer_tag: chat.avatar_tag.clone(),
            peer_color: chat.avatar_color,
            started_at: Instant::now(),
        };
    }

    fn accept_incoming_call(&mut self) {
        self.sound_player.stop_ringtone();
        let CallStatus::Incoming {
            chat_id,
            call_id,
            caller_name,
            caller_email,
            peer_color,
            ..
        } = self.call_status.clone()
        else {
            return;
        };
        let (Some(token), Some(ws_tx)) = (self.session_token.clone(), self.ws_out_tx.clone())
        else {
            return;
        };

        let wire = WsWireEvent::CallAccept {
            session_token: token.clone(),
            chat_id,
            call_id: call_id.clone(),
            peer_email: self.user_email.clone(),
            participants: Vec::new(),
        };
        if let Ok(json) = serde_json::to_string(&wire) {
            let _ = ws_tx.send(json);
        }

        self.audio_bridge.mic_muted.store(false, Ordering::Relaxed);
        self.audio_bridge
            .speaker_muted
            .store(false, Ordering::Relaxed);

        let (peer_name, peer_tag) = self
            .chats
            .iter()
            .find(|c| c.server_chat_id == Some(chat_id))
            .map(|c| {
                if c.is_group {
                    (c.title.clone(), c.avatar_tag.clone())
                } else {
                    (
                        caller_name.clone(),
                        caller_name
                            .chars()
                            .take(2)
                            .collect::<String>()
                            .to_uppercase(),
                    )
                }
            })
            .unwrap_or_else(|| {
                (
                    caller_name.clone(),
                    caller_name
                        .chars()
                        .take(2)
                        .collect::<String>()
                        .to_uppercase(),
                )
            });

        match VoiceEngine::start(
            ws_tx,
            token,
            chat_id,
            call_id.clone(),
            self.user_email.clone(),
            self.audio_bridge.clone(),
        ) {
            Ok(engine) => {
                self.voice_engine = Some(engine);
                self.call_window_expanded = true;
                let now = Instant::now();
                let mut init_parts = vec![self.user_email.clone()];
                if !caller_email.is_empty() && caller_email != self.user_email {
                    init_parts.push(caller_email);
                }
                self.call_status = CallStatus::Connected {
                    chat_id,
                    call_id,
                    peer_name,
                    peer_tag,
                    peer_color,
                    started_at: now,
                    handshaking_until: now + Duration::from_millis(650),
                    participants: init_parts,
                };
            }
            Err(e) => {
                self.show_toast(format!("Ошибка аудио: {}", e));
                self.end_active_call(true);
            }
        }
    }

    fn end_active_call(&mut self, send_signal: bool) {
        self.sound_player.stop_ringtone();
        let call_meta = match &self.call_status {
            CallStatus::Idle => None,
            CallStatus::Outgoing {
                chat_id, call_id, ..
            }
            | CallStatus::Incoming {
                chat_id, call_id, ..
            }
            | CallStatus::Connected {
                chat_id, call_id, ..
            } => Some((*chat_id, call_id.clone())),
        };

        if send_signal {
            if let (Some((chat_id, call_id)), Some(token), Some(ws_tx)) =
                (call_meta, &self.session_token, &self.ws_out_tx)
            {
                let wire = WsWireEvent::CallHangup {
                    session_token: token.clone(),
                    chat_id,
                    call_id,
                    peer_email: self.user_email.clone(),
                    actor_email: self.user_email.clone(),
                    participants: Vec::new(),
                };
                if let Ok(json) = serde_json::to_string(&wire) {
                    let _ = ws_tx.send(json);
                }
            }
        }

        if let Some(engine) = self.voice_engine.take() {
            engine.stop();
        }
        self.audio_bridge.set_active_chat(None);
        self.call_status = CallStatus::Idle;
    }

    fn send_mark_read(&self, chat_idx: usize) {
        let Some(chat) = self.chats.get(chat_idx) else {
            return;
        };
        let (Some(sc_id), Some(token), Some(ws_tx)) =
            (chat.server_chat_id, &self.session_token, &self.ws_out_tx)
        else {
            return;
        };
        let has_unread_incoming = chat.messages.iter().any(|m| !m.is_me && !m.is_read);
        if !has_unread_incoming && chat.unread == 0 {
            return;
        }
        let wire = WsWireEvent::MarkRead {
            session_token: token.clone(),
            chat_id: sc_id,
        };
        if let Ok(json) = serde_json::to_string(&wire) {
            let _ = ws_tx.send(json);
        }
    }

    fn decode_server_message(&self, m: ChatMessageDto) -> Message {
        let is_me = m.sender_email == self.user_email;
        let (text, was_enc) = if m.encrypted {
            decrypt_payload(m.chat_id, &m.text)
        } else {
            (m.text, false)
        };

        let attachment = m.attachment.map(|mut sa| {
            if m.encrypted {
                let (dec_t, _) = decrypt_payload(m.chat_id, &sa.title);
                let (dec_s, _) = decrypt_payload(m.chat_id, &sa.subtitle);
                sa.title = dec_t;
                sa.subtitle = dec_s;
            }
            Attachment::from_server(sa)
        });

        Message {
            server_id: Some(m.id),
            sender: m.sender_name,
            text,
            time: m.time,
            is_me,
            encrypted: m.encrypted || was_enc,
            is_read: m.is_read,
            created_at: Instant::now(),
            attachment,
        }
    }

    fn build_chat_from_dto(&self, sc: ChatDto, color_idx: usize) -> Chat {
        let color = GROUP_COLORS[color_idx % GROUP_COLORS.len()];
        let unread_count = sc
            .messages
            .iter()
            .filter(|m| m.sender_email != self.user_email && !m.is_read)
            .count() as u32;

        let messages: Vec<Message> = sc
            .messages
            .into_iter()
            .map(|m| {
                let mut msg = self.decode_server_message(m);
                msg.created_at = Instant::now() - Duration::from_secs(10);
                msg
            })
            .collect();

        let effective_avatar_id = sc
            .avatar_file_id
            .clone()
            .or_else(|| sc.peer_avatar_id.clone());
        let av_tex = effective_avatar_id
            .as_ref()
            .and_then(|fid| self.avatar_tex_cache.get(fid).cloned());

        let mut members: Vec<GroupMember> = sc
            .members
            .into_iter()
            .enumerate()
            .map(|(m_i, gm)| {
                let is_owner = gm.email == self.user_email;
                let m_tex = gm
                    .avatar_file_id
                    .as_ref()
                    .and_then(|fid| self.avatar_tex_cache.get(fid).cloned());
                let is_on = is_owner || gm.online || self.online_emails.contains(&gm.email);
                GroupMember {
                    name: gm.name,
                    tag: gm.tag,
                    email: Some(gm.email),
                    bio: gm.bio,
                    avatar_file_id: gm.avatar_file_id,
                    avatar_tex: m_tex,
                    role: if is_owner {
                        GroupRole::Owner
                    } else {
                        GroupRole::Member
                    },
                    custom_title: if is_owner {
                        "Владелец".into()
                    } else {
                        String::new()
                    },
                    avatar_color: if is_owner {
                        AUTH_BLUE
                    } else {
                        GROUP_COLORS[(m_i + 1) % GROUP_COLORS.len()]
                    },
                    online: is_on,
                    rights: if is_owner {
                        AdminRights::all()
                    } else {
                        AdminRights::none()
                    },
                }
            })
            .collect();

        if members.is_empty() {
            members.push(GroupMember {
                name: self.username.clone(),
                tag: self.user_tag.clone(),
                email: Some(self.user_email.clone()),
                bio: self.user_bio.clone(),
                avatar_file_id: self.user_avatar_id.clone(),
                avatar_tex: self.user_avatar.clone(),
                role: GroupRole::Owner,
                custom_title: "Владелец".into(),
                avatar_color: AUTH_BLUE,
                online: true,
                rights: AdminRights::all(),
            });
        }

        let peer_is_on = sc
            .peer_email
            .as_ref()
            .map(|em| self.online_emails.contains(em))
            .unwrap_or(false)
            || sc.peer_online;

        let sub = if sc.is_group {
            sc.subtitle
        } else if peer_is_on {
            "в сети".into()
        } else {
            "не в сети".into()
        };

        let code_display = if sc.invite_code.trim().is_empty() {
            format!("EGR-{:06}", sc.id.abs() % 1_000_000)
        } else {
            sc.invite_code.clone()
        };

        let mut chat = Chat {
            server_chat_id: Some(sc.id),
            invite_code: code_display.clone(),
            title: sc.title,
            subtitle: sub,
            avatar_tag: sc.avatar_tag,
            avatar_color: color,
            avatar_tex: av_tex,
            avatar_file_id: sc.avatar_file_id,
            peer_avatar_id: sc.peer_avatar_id,
            peer_bio: sc.peer_bio,
            peer_online: peer_is_on,
            unread: unread_count,
            is_group: sc.is_group,
            encrypted: true,
            peer_email: sc.peer_email,
            description: sc.description,
            is_public: false,
            invite_link: format!("egramm.me/{}", code_display),
            members,
            permissions: GroupPermissions::default(),
            messages,
        };
        if chat.is_group {
            chat.refresh_group_subtitle();
        }
        chat
    }

    fn trigger_user_search(&self, ctx: &egui::Context, query: String) {
        let Some(token) = self.session_token.clone() else {
            return;
        };
        let tx = self.net_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(6))
                .build()
            else {
                return;
            };

            if let Ok(resp) = client
                .get(format!("{}/api/users/search", SERVER_URL))
                .query(&[("session_token", token), ("q", query)])
                .send()
            {
                if let Ok(data) = resp.json::<SearchUsersResponse>() {
                    let _ = tx.send(NetEvent::SearchResults {
                        users: data.users,
                        groups: data.groups,
                    });
                    ctx_clone.request_repaint();
                }
            }
        });
    }

    fn open_direct_chat(&self, ctx: &egui::Context, target_email: String) {
        let Some(token) = self.session_token.clone() else {
            return;
        };
        let tx = self.net_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
            else {
                return;
            };

            let body = OpenDmRequest {
                session_token: token,
                target_email,
            };

            match client
                .post(format!("{}/api/dm", SERVER_URL))
                .json(&body)
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(dto) = resp.json::<ChatDto>() {
                        let _ = tx.send(NetEvent::ChatOpened(dto));
                        ctx_clone.request_repaint();
                    }
                }
                _ => {
                    let _ = tx.send(NetEvent::Toast("Не удалось открыть чат".into()));
                    ctx_clone.request_repaint();
                }
            }
        });
    }

    fn create_synced_group(
        &mut self,
        ctx: &egui::Context,
        title: String,
        description: String,
        avatar_tag: String,
        avatar_color: Color32,
        avatar_tex: Option<egui::TextureHandle>,
        avatar_path: Option<PathBuf>,
        is_public: bool,
        invite_link: String,
    ) {
        let Some(token) = self.session_token.clone() else {
            let new_id = self.chats.len();
            let mut chat = Chat {
                server_chat_id: None,
                invite_code: format!("EGR-{:06}", (new_id * 137 + 1042) % 1_000_000),
                title: title.clone(),
                subtitle: "1 участн., 1 в сети".into(),
                avatar_tag,
                avatar_color,
                avatar_tex,
                avatar_file_id: None,
                peer_avatar_id: None,
                peer_bio: String::new(),
                peer_online: false,
                unread: 0,
                is_group: true,
                encrypted: false,
                peer_email: None,
                description,
                is_public,
                invite_link,
                members: vec![GroupMember {
                    name: self.username.clone(),
                    tag: self.user_tag.clone(),
                    email: Some(self.user_email.clone()),
                    bio: self.user_bio.clone(),
                    avatar_file_id: self.user_avatar_id.clone(),
                    avatar_tex: self.user_avatar.clone(),
                    role: GroupRole::Owner,
                    custom_title: "Владелец".into(),
                    avatar_color: AUTH_BLUE,
                    online: true,
                    rights: AdminRights::all(),
                }],
                permissions: GroupPermissions::default(),
                messages: vec![],
            };
            chat.refresh_group_subtitle();
            self.chats.push(chat);
            self.selected_chat = Some(new_id);
            self.chat_switch_time = Instant::now();
            self.show_toast("Группа создана");
            return;
        };

        let net_tx = self.net_tx.clone();
        let ws_tx = self.ws_out_tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let client = match reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = net_tx.send(NetEvent::Toast(format!("Ошибка сети: {}", e)));
                    return;
                }
            };

            let uploaded_av_id: Option<String> = avatar_path.and_then(|p| {
                let form = reqwest::blocking::multipart::Form::new().file("file", &p).ok()?;
                let resp = client
                    .post(format!("{}/files/upload", SERVER_URL))
                    .header("x-session-token", &token)
                    .multipart(form)
                    .send()
                    .ok()?;
                if !resp.status().is_success() {
                    return None;
                }
                let up = resp.json::<UploadResponse>().ok()?;
                if let Ok(bytes) = std::fs::read(&p) {
                    let cached = drive_cache_dir().join(format!("avatar_{}", up.drive_file_id));
                    let _ = std::fs::write(&cached, &bytes);
                    let _ = net_tx.send(NetEvent::AvatarFileReady {
                        file_id: up.drive_file_id.clone(),
                        bytes,
                    });
                }
                Some(up.drive_file_id)
            });

            let body = CreateChatRequest {
                session_token: token.clone(),
                title,
                description,
                avatar_tag,
                avatar_file_id: uploaded_av_id.clone(),
                member_emails: None,
            };

            match client
                .post(format!("{}/api/chats", SERVER_URL))
                .json(&body)
                .send()
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(dto) = resp.json::<ChatDto>() {
                        if let (Some(fid), Some(wtx)) = (uploaded_av_id, ws_tx) {
                            let wire = WsWireEvent::GroupAvatarUpdated {
                                session_token: token,
                                chat_id: dto.id,
                                avatar_file_id: Some(fid),
                            };
                            if let Ok(json) = serde_json::to_string(&wire) {
                                let _ = wtx.send(json);
                            }
                        }
                        let _ = net_tx.send(NetEvent::ChatOpened(dto));
                        let _ = net_tx.send(NetEvent::Toast("Группа создана".into()));
                        ctx_clone.request_repaint();
                    }
                }
                Ok(resp) => {
                    let err = resp.text().unwrap_or_default();
                    let _ = net_tx.send(NetEvent::Toast(format!("Ошибка сервера: {}", err)));
                    ctx_clone.request_repaint();
                }
                Err(e) => {
                    let _ = net_tx.send(NetEvent::Toast(format!(
                        "Не удалось создать группу: {}",
                        e
                    )));
                    ctx_clone.request_repaint();
                }
            }
        });
    }

    fn forward_message(
        &mut self,
        ctx: &egui::Context,
        from_chat_idx: usize,
        msg_idx: usize,
        to_chat_idx: usize,
    ) {
        let Some(src_chat) = self.chats.get(from_chat_idx) else {
            return;
        };
        let Some(orig_msg) = src_chat.messages.get(msg_idx).cloned() else {
            return;
        };

        let forwarded_text = if orig_msg.text.is_empty() {
            format!("Переслано от {}", orig_msg.sender)
        } else {
            format!("Переслано ({}):\n{}", orig_msg.sender, orig_msg.text)
        };

        let sender_name = self.username.clone();
        let session_token = self.session_token.clone();
        let ws_out_tx = self.ws_out_tx.clone();

        let dst_title = {
            let Some(dst_chat) = self.chats.get_mut(to_chat_idx) else {
                return;
            };
            let title = dst_chat.title.clone();

            if let (Some(server_chat_id), Some(token), Some(ws_tx)) =
                (dst_chat.server_chat_id, session_token, ws_out_tx)
            {
                let enc_flag = dst_chat.encrypted;
                let wire_text = if enc_flag {
                    encrypt_payload(server_chat_id, &forwarded_text)
                } else {
                    forwarded_text
                };
                let wire_att = orig_msg
                    .attachment
                    .as_ref()
                    .map(|a| a.to_stored(server_chat_id, enc_flag));

                let wire = WsWireEvent::SendMessage {
                    session_token: token,
                    chat_id: server_chat_id,
                    text: wire_text,
                    encrypted: Some(enc_flag),
                    attachment: wire_att,
                };
                if let Ok(json) = serde_json::to_string(&wire) {
                    let _ = ws_tx.send(json);
                }
                ctx.request_repaint();
            } else {
                dst_chat.messages.push(Message {
                    server_id: None,
                    sender: sender_name,
                    text: forwarded_text,
                    time: Local::now().format("%H:%M").to_string(),
                    is_me: true,
                    encrypted: false,
                    is_read: true,
                    created_at: Instant::now(),
                    attachment: orig_msg.attachment,
                });
            }

            title
        };

        self.show_toast(format!("Переслано в «{}»", dst_title));
    }

    fn cancel_google_login(&mut self) {
        self.auth_attempt = self.auth_attempt.wrapping_add(1);
        self.is_authenticating = false;
        self.auth_url = None;
        self.auth_error = None;
    }

    fn start_google_login(&mut self) {
        self.auth_attempt = self.auth_attempt.wrapping_add(1);
        let attempt = self.auth_attempt;
        self.is_authenticating = true;
        self.auth_url = None;
        self.auth_error = None;
        let tx = self.net_tx.clone();

        std::thread::spawn(move || {
            let client = match reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(NetEvent::AuthError {
                        auth_attempt: attempt,
                        msg: format!("Ошибка клиента: {}", e),
                    });
                    return;
                }
            };

            let start_url = format!("{}/auth/google/start", SERVER_URL);
            let start_res: StartAuthResponse = match client.post(&start_url).send() {
                Ok(resp) if resp.status().is_success() => match resp.json() {
                    Ok(data) => data,
                    Err(e) => {
                        let _ = tx.send(NetEvent::AuthError {
                            auth_attempt: attempt,
                            msg: format!("Ошибка ответа сервера: {}", e),
                        });
                        return;
                    }
                },
                _ => {
                    let _ = tx.send(NetEvent::AuthError {
                        auth_attempt: attempt,
                        msg: "Не удалось подключиться к серверу Egramm.".into(),
                    });
                    return;
                }
            };

            let _ = tx.send(NetEvent::AuthUrlReady(start_res.auth_url.clone()));

            if let Err(e) = open::that(&start_res.auth_url) {
                let _ = tx.send(NetEvent::AuthError {
                    auth_attempt: attempt,
                    msg: format!("Не удалось открыть браузер: {}", e),
                });
                return;
            }

            let poll_url = format!("{}/auth/google/poll?state={}", SERVER_URL, start_res.state);
            for _ in 0..90 {
                std::thread::sleep(Duration::from_secs(1));
                if let Ok(resp) = client.get(&poll_url).send() {
                    if let Ok(poll) = resp.json::<PollResponse>() {
                        if poll.status == "completed" {
                            if let (Some(name), Some(email), Some(token)) =
                                (poll.name, poll.email, poll.session_token)
                            {
                                let _ = tx.send(NetEvent::AuthSuccess {
                                    auth_attempt: attempt,
                                    name,
                                    email,
                                    token,
                                    avatar_file_id: poll.avatar_file_id,
                                    bio: poll.bio.unwrap_or_default(),
                                });
                                return;
                            }
                        } else if poll.status == "expired" || poll.status == "not_found" {
                            let _ = tx.send(NetEvent::AuthError {
                                auth_attempt: attempt,
                                msg: "Вход отменён или время ожидания истекло".into(),
                            });
                            return;
                        }
                    }
                }
            }

            let _ = tx.send(NetEvent::AuthError {
                auth_attempt: attempt,
                msg: "Время ожидания Google истекло — попробуйте снова".into(),
            });
        });
    }

    fn start_realtime_sync(&mut self, ctx: egui::Context) {
        self.ws_loop_running.store(false, Ordering::SeqCst);
        let running_flag = Arc::new(AtomicBool::new(true));
        self.ws_loop_running = running_flag.clone();

        let net_tx = self.net_tx.clone();
        let (ws_out_tx, ws_out_rx) = mpsc::channel::<String>();
        self.ws_out_tx = Some(ws_out_tx);

        let token_opt = self.session_token.clone();
        if let Some(token) = token_opt.clone() {
            let sync_tx = net_tx.clone();
            let sync_ctx = ctx.clone();
            std::thread::spawn(move || {
                if let Ok(client) = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                {
                    if let Ok(resp) = client
                        .get(format!("{}/api/sync", SERVER_URL))
                        .query(&[("session_token", token)])
                        .send()
                    {
                        if let Ok(data) = resp.json::<SyncResponse>() {
                            let _ = sync_tx.send(NetEvent::InitialSync {
                                chats: data.chats,
                                online_emails: data.online_emails,
                            });
                            sync_ctx.request_repaint();
                        }
                    }
                }
            });
        }

        let read_tx = net_tx.clone();
        let read_ctx = ctx.clone();
        let bridge_ws = self.audio_bridge.clone();

        std::thread::spawn(move || {
            let mut pending_queue: Vec<String> = Vec::new();

            while running_flag.load(Ordering::SeqCst) {
                while let Ok(payload) = ws_out_rx.try_recv() {
                    pending_queue.push(payload);
                }

                let connect_res = tungstenite::connect(WS_URL);
                let Ok((mut ws, _)) = connect_res else {
                    let _ = read_tx.send(NetEvent::WsStatus(false));
                    read_ctx.request_repaint();
                    std::thread::sleep(Duration::from_millis(1200));
                    continue;
                };

                match ws.get_ref() {
                    tungstenite::stream::MaybeTlsStream::Plain(tcp) => {
                        let _ = tcp.set_read_timeout(Some(Duration::from_millis(12)));
                    }
                    tungstenite::stream::MaybeTlsStream::NativeTls(tls) => {
                        let _ = tls
                            .get_ref()
                            .set_read_timeout(Some(Duration::from_millis(12)));
                    }
                    _ => {}
                }

                if let Some(ref tok) = token_opt {
                    let auth_ev = WsWireEvent::AuthWs {
                        session_token: tok.clone(),
                    };
                    if let Ok(json) = serde_json::to_string(&auth_ev) {
                        let _ = ws.send(tungstenite::Message::Text(json.into()));
                    }
                }

                let _ = read_tx.send(NetEvent::WsStatus(true));
                read_ctx.request_repaint();

                let mut disconnected = false;
                while running_flag.load(Ordering::SeqCst) && !disconnected {
                    while let Ok(payload) = ws_out_rx.try_recv() {
                        pending_queue.push(payload);
                    }

                    while !pending_queue.is_empty() {
                        let item = pending_queue[0].clone();
                        if ws.send(tungstenite::Message::Text(item.into())).is_err() {
                            disconnected = true;
                            break;
                        }
                        pending_queue.remove(0);
                    }

                    if disconnected {
                        break;
                    }

                    match ws.read() {
                        Ok(tungstenite::Message::Text(txt)) => {
                            if let Ok(ev) = serde_json::from_str::<WsWireEvent>(&txt) {
                                match ev {
                                    WsWireEvent::PresenceUpdate { online_emails, .. } => {
                                        let set: HashSet<String> =
                                            online_emails.into_iter().collect();
                                        let _ = read_tx.send(NetEvent::PresenceUpdate(set));
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::ProfileUpdated {
                                        email,
                                        name,
                                        tag,
                                        bio,
                                        ..
                                    } => {
                                        let _ = read_tx.send(NetEvent::ProfileUpdated {
                                            email,
                                            name,
                                            tag,
                                            bio,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::NewMessage { message } => {
                                        let _ = read_tx.send(NetEvent::IncomingMessage(message));
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::NewChat {
                                        chat,
                                        allowed_emails,
                                    } => {
                                        let _ = read_tx.send(NetEvent::IncomingChat {
                                            chat,
                                            allowed_emails,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::MessagesRead {
                                        chat_id,
                                        reader_email,
                                    } => {
                                        let _ = read_tx.send(NetEvent::MessagesRead {
                                            chat_id,
                                            reader_email,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::MessageDeleted {
                                        chat_id,
                                        message_id,
                                    } => {
                                        let _ = read_tx.send(NetEvent::MessageDeleted {
                                            chat_id,
                                            message_id,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::AvatarUpdated {
                                        email,
                                        avatar_file_id,
                                        ..
                                    } => {
                                        let _ = read_tx.send(NetEvent::UserAvatarUpdated {
                                            email,
                                            avatar_file_id,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::GroupAvatarUpdated {
                                        chat_id,
                                        avatar_file_id,
                                        ..
                                    } => {
                                        let _ = read_tx.send(NetEvent::GroupAvatarUpdated {
                                            chat_id,
                                            avatar_file_id,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::CallInvite {
                                        chat_id,
                                        call_id,
                                        caller_name,
                                        caller_email,
                                        ..
                                    } => {
                                        let _ = read_tx.send(NetEvent::CallInvite {
                                            chat_id,
                                            call_id,
                                            caller_name,
                                            caller_email,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::CallAccept {
                                        chat_id,
                                        call_id,
                                        peer_email,
                                        participants,
                                        ..
                                    } => {
                                        let _ = read_tx.send(NetEvent::CallAccept {
                                            chat_id,
                                            call_id,
                                            peer_email,
                                            participants,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::CallHangup {
                                        chat_id,
                                        peer_email,
                                        actor_email,
                                        participants,
                                        ..
                                    } => {
                                        let from_email = if !actor_email.is_empty() {
                                            actor_email
                                        } else {
                                            peer_email
                                        };
                                        let _ = read_tx.send(NetEvent::CallHangup {
                                            chat_id,
                                            from_email,
                                            participants,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::CallRejected { chat_id, reason } => {
                                        let _ = read_tx.send(NetEvent::CallRejected {
                                            chat_id,
                                            reason,
                                        });
                                        read_ctx.request_repaint();
                                    }
                                    WsWireEvent::CallAudio {
                                        chat_id,
                                        sender_email,
                                        sample_rate,
                                        pcm_b64,
                                        ..
                                    } => {
                                        bridge_ws.push_remote_audio(
                                            chat_id,
                                            &sender_email,
                                            sample_rate,
                                            &pcm_b64,
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Ok(tungstenite::Message::Ping(p)) => {
                            let _ = ws.send(tungstenite::Message::Pong(p));
                        }
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(io_err))
                        if matches!(
                                io_err.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => {
                            disconnected = true;
                        }
                    }
                }

                let _ = ws.close(None);
                let _ = read_tx.send(NetEvent::WsStatus(false));
                read_ctx.request_repaint();
                if !running_flag.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(800));
            }
        });
    }

    fn trigger_drive_download(
        &mut self,
        ctx: &egui::Context,
        drive_file_id: String,
        title: String,
        open_after: bool,
    ) {
        if !open_after && self.downloading_drive_ids.contains(&drive_file_id) {
            return;
        }
        let Some(token) = self.session_token.clone() else {
            return;
        };
        self.downloading_drive_ids.insert(drive_file_id.clone());

        let tx = self.net_tx.clone();
        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let safe_name = title.replace(['/', '\\', ':'], "_");
            let dest_path = drive_cache_dir().join(format!("{}_{}", drive_file_id, safe_name));

            if dest_path.exists() {
                let _ = tx.send(NetEvent::FileReady {
                    drive_file_id,
                    local_path: dest_path,
                    open_after,
                });
                ctx_clone.request_repaint();
                return;
            }

            let url = format!("{}/files/{}", SERVER_URL, drive_file_id);
            if let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
            {
                if let Ok(resp) = client
                    .get(&url)
                    .header("x-session-token", &token)
                    .send()
                {
                    if resp.status().is_success() {
                        if let Ok(bytes) = resp.bytes() {
                            if std::fs::write(&dest_path, &bytes).is_ok() {
                                let _ = tx.send(NetEvent::FileReady {
                                    drive_file_id,
                                    local_path: dest_path,
                                    open_after,
                                });
                                ctx_clone.request_repaint();
                                return;
                            }
                        }
                    }
                }
            }
            let _ = tx.send(NetEvent::Toast("Не удалось скачать файл".into()));
            ctx_clone.request_repaint();
        });
    }

    fn send_message(&mut self, ctx: &egui::Context) {
        let trimmed = self.input_text.trim().to_string();
        let attachment = self.pending_attachment.take();
        if trimmed.is_empty() && attachment.is_none() {
            return;
        }
        let Some(chat_idx) = self.selected_chat else {
            return;
        };
        let Some(chat) = self.chats.get_mut(chat_idx) else {
            return;
        };

        self.input_text.clear();

        if let (Some(server_chat_id), Some(token), Some(ws_tx)) = (
            chat.server_chat_id,
            self.session_token.clone(),
            self.ws_out_tx.clone(),
        ) {
            let enc_flag = chat.encrypted;
            let wire_text = if enc_flag {
                encrypt_payload(server_chat_id, &trimmed)
            } else {
                trimmed
            };

            if let Some(att) = attachment {
                if let Some(local_path) = att.local_path.clone() {
                    self.show_toast(format!("Загрузка «{}»...", att.title));
                    let net_tx = self.net_tx.clone();
                    let ctx_clone = ctx.clone();

                    std::thread::spawn(move || {
                        let client = match reqwest::blocking::Client::builder()
                            .timeout(Duration::from_secs(180))
                            .build()
                        {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = net_tx.send(NetEvent::Toast(format!("Ошибка сети: {}", e)));
                                return;
                            }
                        };

                        let form = match reqwest::blocking::multipart::Form::new()
                            .file("file", &local_path)
                        {
                            Ok(f) => f,
                            Err(e) => {
                                let _ = net_tx
                                    .send(NetEvent::Toast(format!("Ошибка чтения файла: {}", e)));
                                return;
                            }
                        };

                        match client
                            .post(format!("{}/files/upload", SERVER_URL))
                            .header("x-session-token", &token)
                            .multipart(form)
                            .send()
                        {
                            Ok(resp) if resp.status().is_success() => {
                                if let Ok(up) = resp.json::<UploadResponse>() {
                                    let safe_name = up.title.replace(['/', '\\', ':'], "_");
                                    let cache_path = drive_cache_dir()
                                        .join(format!("{}_{}", up.drive_file_id, safe_name));
                                    let _ = std::fs::copy(&local_path, &cache_path);

                                    let (out_title, out_sub) = if enc_flag {
                                        (
                                            encrypt_payload(server_chat_id, &up.title),
                                            encrypt_payload(server_chat_id, &up.subtitle),
                                        )
                                    } else {
                                        (up.title, up.subtitle)
                                    };

                                    let wire = WsWireEvent::SendMessage {
                                        session_token: token,
                                        chat_id: server_chat_id,
                                        text: wire_text,
                                        encrypted: Some(enc_flag),
                                        attachment: Some(StoredAttachment {
                                            kind: up.kind,
                                            title: out_title,
                                            subtitle: out_sub,
                                            drive_file_id: Some(up.drive_file_id),
                                        }),
                                    };
                                    if let Ok(json) = serde_json::to_string(&wire) {
                                        let _ = ws_tx.send(json);
                                    }
                                    ctx_clone.request_repaint();
                                }
                            }
                            Ok(resp) => {
                                let err = resp.text().unwrap_or_default();
                                let _ = net_tx.send(NetEvent::Toast(format!(
                                    "Сервер отклонил файл: {}",
                                    err
                                )));
                                ctx_clone.request_repaint();
                            }
                            Err(e) => {
                                let _ = net_tx
                                    .send(NetEvent::Toast(format!("Ошибка отправки файла: {}", e)));
                                ctx_clone.request_repaint();
                            }
                        }
                    });
                } else {
                    let wire_att = att.to_stored(server_chat_id, enc_flag);
                    let wire = WsWireEvent::SendMessage {
                        session_token: token,
                        chat_id: server_chat_id,
                        text: wire_text,
                        encrypted: Some(enc_flag),
                        attachment: Some(wire_att),
                    };
                    if let Ok(json) = serde_json::to_string(&wire) {
                        let _ = ws_tx.send(json);
                    }
                }
            } else {
                let wire = WsWireEvent::SendMessage {
                    session_token: token,
                    chat_id: server_chat_id,
                    text: wire_text,
                    encrypted: Some(enc_flag),
                    attachment: None,
                };
                if let Ok(json) = serde_json::to_string(&wire) {
                    let _ = ws_tx.send(json);
                }
            }
            return;
        }

        let time_str = Local::now().format("%H:%M").to_string();
        chat.messages.push(Message {
            server_id: None,
            sender: self.username.clone(),
            text: trimmed,
            time: time_str,
            is_me: true,
            encrypted: false,
            is_read: true,
            created_at: Instant::now(),
            attachment,
        });
    }

    fn pick_attachment(&mut self, kind: AttachmentKind) {
        self.attach_menu_open = false;
        match kind {
            AttachmentKind::Media => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Выберите фото или видео")
                    .add_filter(
                        "Медиа",
                        &[
                            "png", "jpg", "jpeg", "gif", "webp", "bmp", "mp4", "mov", "mkv",
                        ],
                    )
                    .pick_file()
                {
                    self.pending_attachment =
                        Some(Attachment::from_path(path, Some(AttachmentKind::Media)));
                }
            }
            AttachmentKind::Document => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Выберите документ или любой файл")
                    .pick_file()
                {
                    self.pending_attachment =
                        Some(Attachment::from_path(path, Some(AttachmentKind::Document)));
                }
            }
            AttachmentKind::Music => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Выберите музыку")
                    .add_filter("Аудио", &["mp3", "wav", "flac", "ogg", "m4a"])
                    .pick_file()
                {
                    self.pending_attachment =
                        Some(Attachment::from_path(path, Some(AttachmentKind::Music)));
                }
            }
            AttachmentKind::Poll => {
                self.active_modal = ActiveModal::CreatePoll;
            }
            AttachmentKind::Article => {
                self.pending_attachment = Some(Attachment {
                    kind: AttachmentKind::Article,
                    title: "Черновик статьи".into(),
                    subtitle: "Нажмите отправить с текстом".into(),
                    local_path: None,
                    drive_file_id: None,
                });
            }
            AttachmentKind::Location => {
                self.pending_attachment = Some(Attachment {
                    kind: AttachmentKind::Location,
                    title: "Моя геопозиция".into(),
                    subtitle: "Точка на карте • Egramm".into(),
                    local_path: None,
                    drive_file_id: None,
                });
            }
        }
    }

    fn show_intro_animation(&mut self, ctx: &egui::Context) {
        let Some(start) = self.intro_start else {
            return;
        };

        let t = start.elapsed().as_secs_f32();
        let total = 2.45_f32;
        if t >= total {
            self.intro_start = None;
            return;
        }
        ctx.request_repaint();

        let screen = ctx.screen_rect();
        let center = screen.center();

        let exit_t = ((t - 1.90) / 0.55).clamp(0.0, 1.0);
        let master_alpha = 1.0 - ease_out_cubic(exit_t);
        let exit_zoom = 1.0 + ease_out_cubic(exit_t) * 0.08;

        let mut skip = false;

        egui::Area::new(egui::Id::new("egramm_first_intro"))
            .fixed_pos(screen.min)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
                if resp.clicked() && t > 0.35 {
                    skip = true;
                }
                let p = ui.painter();

                p.rect_filled(
                    rect,
                    Rounding::ZERO,
                    with_alpha(Color32::from_rgb(9, 15, 24), master_alpha),
                );

                let glow_t = ease_out_cubic((t / 0.85).clamp(0.0, 1.0));
                let glow_center = Pos2::new(center.x, center.y - 24.0);
                for (r_base, col, a) in [
                    (320.0, Color32::from_rgb(28, 92, 200), 0.12),
                    (210.0, Color32::from_rgb(43, 135, 232), 0.16),
                    (125.0, Color32::from_rgb(80, 185, 255), 0.20),
                ] {
                    let breathe = 1.0 + (t * 2.8).sin() * 0.03;
                    p.circle_filled(
                        glow_center,
                        r_base * glow_t * breathe * exit_zoom,
                        with_alpha(col, a * master_alpha),
                    );
                }

                let logo_t = ((t - 0.12) / 0.52).clamp(0.0, 1.0);
                let logo_scale = ease_out_back(logo_t) * exit_zoom;
                let logo_alpha = ease_out_cubic((t - 0.10) / 0.28) * master_alpha;

                if logo_alpha > 0.01 {
                    let r = 46.0 * logo_scale.max(0.01);
                    p.circle_filled(glow_center, r, with_alpha(AUTH_BLUE, logo_alpha));
                    p.circle_stroke(
                        glow_center,
                        r,
                        Stroke::new(
                            2.0,
                            with_alpha(Color32::from_rgb(156, 218, 255), 0.75 * logo_alpha),
                        ),
                    );
                    p.text(
                        glow_center,
                        egui::Align2::CENTER_CENTER,
                        "E",
                        semibold_font((46.0 * logo_scale).max(8.0)),
                        with_alpha(Color32::WHITE, logo_alpha),
                    );
                }

                let title_t = ease_out_cubic(((t - 0.42) / 0.42).clamp(0.0, 1.0));
                if title_t > 0.01 {
                    let title_y = glow_center.y + 82.0 + (1.0 - title_t) * 20.0;
                    p.text(
                        Pos2::new(center.x, title_y),
                        egui::Align2::CENTER_CENTER,
                        "Egramm",
                        semibold_font(34.0 * exit_zoom),
                        with_alpha(Color32::WHITE, title_t * master_alpha),
                    );
                }

                let welcome_t = ease_out_cubic(((t - 0.75) / 0.42).clamp(0.0, 1.0));
                if welcome_t > 0.01 {
                    let wy = glow_center.y + 124.0 + (1.0 - welcome_t) * 14.0;
                    p.text(
                        Pos2::new(center.x, wy),
                        egui::Align2::CENTER_CENTER,
                        format!("Добро пожаловать, {}", self.username),
                        FontId::proportional(17.0),
                        with_alpha(Color32::from_rgb(198, 224, 250), welcome_t * master_alpha),
                    );
                }
            });

        if skip {
            self.intro_start = None;
        }
    }

    fn show_login_screen(&mut self, ctx: &egui::Context) {
        let pal = self.palette();
        let elapsed = self.auth_anim_start.elapsed().as_secs_f32();
        if elapsed < 1.1 || self.is_authenticating {
            ctx.request_repaint();
        }

        let card_t = ease_out_cubic(elapsed / 0.45);
        let logo_t = ease_out_cubic((elapsed - 0.08) / 0.50);
        let text_t = ease_out_cubic((elapsed - 0.22) / 0.45);
        let btn_t = ease_out_cubic((elapsed - 0.36) / 0.48);

        let mut theme_toggled = false;
        let mut cancel_login = false;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.auth_bg))
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(18.0);
                        egui::Frame::none()
                            .fill(pal.auth_card)
                            .stroke(Stroke::new(1.0, pal.auth_border))
                            .rounding(Rounding::same(18.0))
                            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                            .show(ui, |ui| {
                                if draw_theme_slider_row(
                                    ui,
                                    self.is_dark_mode,
                                    self.theme_anim,
                                    &pal,
                                    pal.auth_card,
                                ) {
                                    theme_toggled = true;
                                }
                            });
                    });
                });

                let avail = ui.available_size();
                let card_w = 380.0;
                let card_h = 370.0;

                let float_y = (1.0 - card_t) * 26.0;
                let top_pad = ((avail.y - card_h) * 0.5 + float_y - 16.0).max(12.0);
                let left_pad = ((avail.x - card_w) * 0.5).max(16.0);

                ui.add_space(top_pad);
                ui.horizontal(|ui| {
                    ui.add_space(left_pad);

                    egui::Frame::none()
                        .fill(with_alpha(pal.auth_card, card_t))
                        .stroke(Stroke::new(1.2, with_alpha(pal.auth_border, card_t)))
                        .rounding(Rounding::same(24.0))
                        .inner_margin(egui::Margin::symmetric(32.0, 34.0))
                        .show(ui, |ui| {
                            ui.set_width(card_w - 64.0);
                            ui.vertical_centered(|ui| {
                                let logo_offset = (1.0 - logo_t) * 18.0;
                                ui.add_space(logo_offset);

                                let (logo_rect, logo_resp) =
                                    ui.allocate_exact_size(Vec2::splat(84.0), egui::Sense::click());
                                let p = ui.painter();
                                let c = logo_rect.center();

                                let hover_scale = if logo_resp.hovered() { 2.0 } else { 0.0 };
                                let r = (36.0 + hover_scale) * (0.85 + 0.15 * logo_t);

                                p.circle_filled(c, r + 7.0, with_alpha(pal.auth_blue_soft, logo_t));
                                p.circle_filled(
                                    c,
                                    r,
                                    with_alpha(
                                        if logo_resp.hovered() {
                                            AUTH_BLUE_HOVER
                                        } else {
                                            AUTH_BLUE
                                        },
                                        logo_t,
                                    ),
                                );
                                p.text(
                                    c,
                                    egui::Align2::CENTER_CENTER,
                                    "E",
                                    semibold_font(36.0),
                                    with_alpha(Color32::WHITE, logo_t),
                                );

                                if logo_resp.clicked() && !self.is_authenticating {
                                    self.auth_anim_start = Instant::now();
                                }

                                ui.add_space(16.0 - logo_offset * 0.5);

                                ui.label(
                                    egui::RichText::new("Egramm")
                                        .font(semibold_font(24.0))
                                        .color(with_alpha(pal.auth_text, text_t)),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("Вход в аккаунт")
                                        .size(14.0)
                                        .color(with_alpha(pal.auth_muted, text_t)),
                                );

                                ui.add_space(24.0 + (1.0 - btn_t) * 14.0);

                                let btn_w = ui.available_width();
                                let btn_h = 46.0;
                                let (btn_rect, btn_resp) = ui.allocate_exact_size(
                                    Vec2::new(btn_w, btn_h),
                                    egui::Sense::click(),
                                );

                                let btn_bg = if self.is_authenticating {
                                    pal.auth_blue_soft
                                } else if btn_resp.hovered() {
                                    AUTH_BLUE_HOVER
                                } else {
                                    AUTH_BLUE
                                };

                                let bp = ui.painter();
                                bp.rect_filled(
                                    btn_rect,
                                    Rounding::same(23.0),
                                    with_alpha(btn_bg, btn_t),
                                );

                                if self.is_authenticating {
                                    bp.text(
                                        btn_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "Подтвердите вход в браузере...",
                                        semibold_font(14.0),
                                        AUTH_BLUE,
                                    );
                                } else {
                                    let badge_center =
                                        Pos2::new(btn_rect.min.x + 24.0, btn_rect.center().y);
                                    bp.circle_filled(
                                        badge_center,
                                        14.0,
                                        with_alpha(Color32::WHITE, btn_t),
                                    );
                                    bp.text(
                                        badge_center,
                                        egui::Align2::CENTER_CENTER,
                                        "G",
                                        semibold_font(16.0),
                                        with_alpha(AUTH_BLUE, btn_t),
                                    );

                                    bp.text(
                                        Pos2::new(btn_rect.center().x + 8.0, btn_rect.center().y),
                                        egui::Align2::CENTER_CENTER,
                                        "Войти через Google",
                                        semibold_font(15.0),
                                        with_alpha(Color32::WHITE, btn_t),
                                    );

                                    if btn_resp.clicked() && btn_t > 0.5 {
                                        self.start_google_login();
                                    }
                                }

                                if self.is_authenticating {
                                    ui.add_space(10.0);
                                    if ui
                                        .add_sized(
                                            Vec2::new(btn_w, 30.0),
                                            egui::Button::new(
                                                egui::RichText::new(
                                                    "Закрыли вкладку? Отменить и повторить",
                                                )
                                                    .size(12.2)
                                                    .color(DANGER_RED),
                                            )
                                                .fill(Color32::TRANSPARENT)
                                                .rounding(15.0),
                                        )
                                        .clicked()
                                    {
                                        cancel_login = true;
                                    }
                                }

                                if let Some(err) = &self.auth_error {
                                    ui.add_space(10.0);
                                    ui.label(egui::RichText::new(err).size(12.0).color(DANGER_RED));
                                }

                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new("Защищено TLS + ChaCha20-Poly1305")
                                        .size(11.5)
                                        .color(with_alpha(pal.auth_muted, btn_t)),
                                );
                            });
                        });
                });
            });

        if cancel_login {
            self.cancel_google_login();
        }

        if theme_toggled {
            self.is_dark_mode = !self.is_dark_mode;
            let new_pal = self.palette();
            apply_theme_style(ctx, &new_pal, self.is_dark_mode);
        }
    }

    fn show_lightbox(&mut self, ctx: &egui::Context) {
        let Some(path) = self.lightbox_image.clone() else {
            return;
        };

        let elapsed = self.lightbox_open_time.elapsed().as_secs_f32();
        let fade = ease_out_cubic(elapsed / 0.16);
        if elapsed < 0.2 {
            ctx.request_repaint();
        }

        let scroll_y = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll_y.abs() > 0.2 {
            let factor = if scroll_y > 0.0 { 1.14 } else { 1.0 / 1.14 };
            self.lightbox_zoom = (self.lightbox_zoom * factor).clamp(0.5, 6.0);
            if self.lightbox_zoom <= 1.02 {
                self.lightbox_offset = Vec2::ZERO;
            }
        }

        let tex = get_or_load_image(ctx, &mut self.image_cache, &path);
        let screen = ctx.screen_rect();
        let mut close_now = false;

        egui::Area::new(egui::Id::new("tg_photo_lightbox"))
            .fixed_pos(screen.min)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (bg_rect, bg_resp) =
                    ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());

                ui.painter().rect_filled(
                    bg_rect,
                    Rounding::ZERO,
                    Color32::from_rgba_unmultiplied(8, 12, 18, (232.0 * fade) as u8),
                );

                let mut img_rect = Rect::NOTHING;
                if let Some(t) = &tex {
                    let orig = t.size_vec2();
                    let max_w = (screen.width() - 120.0).max(200.0);
                    let max_h = (screen.height() - 150.0).max(200.0);
                    let fit_scale = (max_w / orig.x).min(max_h / orig.y).min(1.0);
                    let base_size = orig * fit_scale;
                    let disp_size = base_size * self.lightbox_zoom * (0.94 + 0.06 * fade);

                    img_rect =
                        Rect::from_center_size(screen.center() + self.lightbox_offset, disp_size);

                    ui.painter().image(
                        t.id(),
                        img_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        with_alpha(Color32::WHITE, fade),
                    );
                }

                let bar_w = 290.0;
                let bar_h = 40.0;
                let bar_rect = Rect::from_center_size(
                    Pos2::new(screen.center().x, screen.max.y - 38.0),
                    Vec2::new(bar_w, bar_h),
                );
                let close_rect = Rect::from_center_size(
                    Pos2::new(screen.max.x - 32.0, screen.min.y + 30.0),
                    Vec2::splat(38.0),
                );

                if bg_resp.dragged() && self.lightbox_zoom > 1.01 {
                    self.lightbox_offset += bg_resp.drag_delta();
                }

                if bg_resp.double_clicked() {
                    if let Some(pos) = bg_resp.interact_pointer_pos() {
                        if img_rect.contains(pos) {
                            if self.lightbox_zoom > 1.15 {
                                self.lightbox_zoom = 1.0;
                                self.lightbox_offset = Vec2::ZERO;
                            } else {
                                self.lightbox_zoom = 2.2;
                            }
                        }
                    }
                } else if bg_resp.clicked() {
                    if let Some(pos) = bg_resp.interact_pointer_pos() {
                        if !img_rect.contains(pos)
                            && !bar_rect.contains(pos)
                            && !close_rect.contains(pos)
                        {
                            close_now = true;
                        }
                    }
                }

                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("Фото");
                ui.painter().text(
                    Pos2::new(screen.min.x + 24.0, screen.min.y + 30.0),
                    egui::Align2::LEFT_CENTER,
                    fname,
                    semibold_font(14.5),
                    with_alpha(Color32::WHITE, fade),
                );

                let close_resp = ui.interact(
                    close_rect,
                    egui::Id::new("lightbox_close_btn"),
                    egui::Sense::click(),
                );
                ui.painter().circle_filled(
                    close_rect.center(),
                    18.0,
                    with_alpha(
                        if close_resp.hovered() {
                            Color32::from_rgb(56, 70, 88)
                        } else {
                            Color32::from_rgb(30, 40, 52)
                        },
                        fade,
                    ),
                );
                ui.painter().text(
                    close_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "✕",
                    semibold_font(16.0),
                    with_alpha(Color32::WHITE, fade),
                );
                if close_resp.clicked() {
                    close_now = true;
                }

                ui.painter().rect_filled(
                    bar_rect,
                    Rounding::same(20.0),
                    with_alpha(Color32::from_rgb(24, 34, 46), 0.94 * fade),
                );
                ui.painter().rect_stroke(
                    bar_rect,
                    Rounding::same(20.0),
                    Stroke::new(1.0, with_alpha(Color32::from_rgb(52, 70, 92), fade)),
                );

                let mut bar_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(bar_rect.shrink2(Vec2::new(10.0, 4.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );

                if bar_ui
                    .add_sized(
                        Vec2::new(34.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("−").size(18.0).color(Color32::WHITE),
                        )
                            .fill(Color32::TRANSPARENT)
                            .rounding(15.0),
                    )
                    .clicked()
                {
                    self.lightbox_zoom = (self.lightbox_zoom / 1.25).clamp(0.5, 6.0);
                    if self.lightbox_zoom <= 1.02 {
                        self.lightbox_offset = Vec2::ZERO;
                    }
                }

                let pct = format!("{}%", (self.lightbox_zoom * 100.0).round() as i32);
                if bar_ui
                    .add_sized(
                        Vec2::new(62.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new(pct)
                                .font(semibold_font(13.0))
                                .color(Color32::WHITE),
                        )
                            .fill(Color32::TRANSPARENT)
                            .rounding(15.0),
                    )
                    .on_hover_text("Сбросить масштаб")
                    .clicked()
                {
                    self.lightbox_zoom = 1.0;
                    self.lightbox_offset = Vec2::ZERO;
                }

                if bar_ui
                    .add_sized(
                        Vec2::new(34.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("+").size(18.0).color(Color32::WHITE),
                        )
                            .fill(Color32::TRANSPARENT)
                            .rounding(15.0),
                    )
                    .clicked()
                {
                    self.lightbox_zoom = (self.lightbox_zoom * 1.25).clamp(0.5, 6.0);
                }

                bar_ui.add_space(10.0);
                if bar_ui
                    .add_sized(
                        Vec2::new(108.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("Открыть файл")
                                .size(12.5)
                                .color(Color32::from_rgb(125, 194, 255)),
                        )
                            .fill(Color32::from_rgb(34, 48, 66))
                            .rounding(15.0),
                    )
                    .clicked()
                {
                    let _ = open::that(&path);
                }
            });

        if close_now {
            self.lightbox_image = None;
            self.lightbox_zoom = 1.0;
            self.lightbox_offset = Vec2::ZERO;
        }
    }
}

fn draw_star_icon(p: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment([Pos2::new(c.x, c.y - r), Pos2::new(c.x, c.y + r)], s);
    p.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], s);
    let d = r * 0.48;
    p.line_segment(
        [Pos2::new(c.x - d, c.y - d), Pos2::new(c.x + d, c.y + d)],
        Stroke::new(1.2, color),
    );
    p.line_segment(
        [Pos2::new(c.x + d, c.y - d), Pos2::new(c.x - d, c.y + d)],
        Stroke::new(1.2, color),
    );
    p.circle_filled(c, r * 0.34, color);
}

fn draw_person_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.7, color);
    p.circle_stroke(Pos2::new(c.x, c.y - 4.0), 3.6, s);
    let body = Rect::from_min_max(
        Pos2::new(c.x - 6.5, c.y + 1.5),
        Pos2::new(c.x + 6.5, c.y + 7.5),
    );
    p.rect_stroke(
        body,
        Rounding {
            nw: 6.0,
            ne: 6.0,
            sw: 2.0,
            se: 2.0,
        },
        s,
    );
}

fn draw_group_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.circle_stroke(Pos2::new(c.x - 2.5, c.y - 4.0), 3.0, s);
    p.circle_stroke(Pos2::new(c.x + 4.0, c.y - 3.2), 2.3, s);
    let b1 = Rect::from_min_max(
        Pos2::new(c.x - 7.5, c.y + 1.2),
        Pos2::new(c.x + 2.5, c.y + 7.2),
    );
    p.rect_stroke(
        b1,
        Rounding {
            nw: 5.0,
            ne: 5.0,
            sw: 1.5,
            se: 1.5,
        },
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 3.5, c.y + 2.0), Pos2::new(c.x + 7.5, c.y + 2.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 7.5, c.y + 2.0), Pos2::new(c.x + 7.5, c.y + 6.8)],
        s,
    );
}

fn draw_logout_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y - 6.5), Pos2::new(c.x, c.y - 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y - 6.5), Pos2::new(c.x - 6.0, c.y + 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y + 6.5), Pos2::new(c.x, c.y + 6.5)],
        s,
    );
    p.line_segment([Pos2::new(c.x - 1.5, c.y), Pos2::new(c.x + 6.5, c.y)], s);
    p.line_segment(
        [Pos2::new(c.x + 3.2, c.y - 3.2), Pos2::new(c.x + 6.5, c.y)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 3.2, c.y + 3.2), Pos2::new(c.x + 6.5, c.y)],
        s,
    );
}

fn draw_phone_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.8, color);
    p.line_segment(
        [Pos2::new(c.x - 5.0, c.y - 5.5), Pos2::new(c.x - 2.0, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 5.0, c.y - 5.5), Pos2::new(c.x - 6.5, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.5, c.y - 2.5), Pos2::new(c.x + 2.5, c.y + 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.5, c.y + 6.5), Pos2::new(c.x + 5.5, c.y + 5.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 5.5, c.y + 5.0), Pos2::new(c.x + 2.5, c.y + 2.0)],
        s,
    );
}

fn draw_hangup_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(2.2, color);
    p.line_segment(
        [Pos2::new(c.x - 9.0, c.y + 2.0), Pos2::new(c.x - 4.0, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 4.0, c.y - 2.5), Pos2::new(c.x + 4.0, c.y - 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 4.0, c.y - 2.5), Pos2::new(c.x + 9.0, c.y + 2.0)],
        s,
    );
    p.circle_filled(Pos2::new(c.x - 8.5, c.y + 2.5), 2.1, color);
    p.circle_filled(Pos2::new(c.x + 8.5, c.y + 2.5), 2.1, color);
}

fn draw_mic_icon(p: &egui::Painter, c: Pos2, muted: bool, color: Color32) {
    let s = Stroke::new(1.8, color);
    let body = Rect::from_center_size(Pos2::new(c.x, c.y - 2.5), Vec2::new(6.8, 10.5));
    p.rect_stroke(body, Rounding::same(3.4), s);
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y - 1.0), Pos2::new(c.x - 6.0, c.y + 2.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 6.0, c.y - 1.0), Pos2::new(c.x + 6.0, c.y + 2.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 6.0, c.y + 2.0), Pos2::new(c.x + 6.0, c.y + 2.0)],
        s,
    );
    p.line_segment([Pos2::new(c.x, c.y + 2.0), Pos2::new(c.x, c.y + 7.5)], s);
    p.line_segment(
        [Pos2::new(c.x - 4.0, c.y + 7.5), Pos2::new(c.x + 4.0, c.y + 7.5)],
        s,
    );
    if muted {
        p.line_segment(
            [Pos2::new(c.x - 8.5, c.y - 8.0), Pos2::new(c.x + 8.5, c.y + 8.0)],
            Stroke::new(2.2, DANGER_RED),
        );
    }
}

fn draw_speaker_icon(p: &egui::Painter, c: Pos2, muted: bool, color: Color32) {
    let s = Stroke::new(1.8, color);
    let box_r = Rect::from_min_max(
        Pos2::new(c.x - 7.5, c.y - 3.0),
        Pos2::new(c.x - 3.0, c.y + 3.0),
    );
    p.rect_stroke(box_r, Rounding::same(1.0), s);
    p.line_segment(
        [Pos2::new(c.x - 3.0, c.y - 3.0), Pos2::new(c.x + 2.0, c.y - 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.0, c.y - 6.5), Pos2::new(c.x + 2.0, c.y + 6.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.0, c.y + 6.5), Pos2::new(c.x - 3.0, c.y + 3.0)],
        s,
    );
    if muted {
        p.line_segment(
            [Pos2::new(c.x + 4.5, c.y - 3.0), Pos2::new(c.x + 9.0, c.y + 3.0)],
            Stroke::new(2.0, DANGER_RED),
        );
        p.line_segment(
            [Pos2::new(c.x + 9.0, c.y - 3.0), Pos2::new(c.x + 4.5, c.y + 3.0)],
            Stroke::new(2.0, DANGER_RED),
        );
    } else {
        p.line_segment(
            [Pos2::new(c.x + 5.2, c.y - 3.2), Pos2::new(c.x + 5.2, c.y + 3.2)],
            s,
        );
        p.line_segment(
            [Pos2::new(c.x + 8.4, c.y - 5.5), Pos2::new(c.x + 8.4, c.y + 5.5)],
            s,
        );
    }
}

fn draw_lock_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.4, color);
    let body = Rect::from_center_size(Pos2::new(c.x, c.y + 2.0), Vec2::new(8.5, 6.5));
    p.rect_stroke(body, Rounding::same(1.5), s);
    let arch = Rect::from_center_size(Pos2::new(c.x, c.y - 2.5), Vec2::new(5.5, 5.5));
    p.rect_stroke(
        arch,
        Rounding {
            nw: 3.0,
            ne: 3.0,
            sw: 0.0,
            se: 0.0,
        },
        s,
    );
}

fn draw_call_fingerprint_glyph(p: &egui::Painter, c: Pos2, kind: u8, color: Color32) {
    match kind {
        0 => draw_star_icon(p, c, 5.5, color),
        1 => {
            p.circle_stroke(c, 5.0, Stroke::new(1.6, color));
            p.circle_filled(c, 2.2, color);
        }
        2 => {
            let r = 5.4;
            let pts = [
                Pos2::new(c.x, c.y - r),
                Pos2::new(c.x + r, c.y),
                Pos2::new(c.x, c.y + r),
                Pos2::new(c.x - r, c.y),
            ];
            for i in 0..4 {
                p.line_segment([pts[i], pts[(i + 1) % 4]], Stroke::new(1.6, color));
            }
            p.circle_filled(c, 1.8, color);
        }
        _ => {
            let r = Rect::from_center_size(c, Vec2::splat(9.0));
            p.rect_stroke(r, Rounding::same(2.5), Stroke::new(1.6, color));
            p.circle_filled(c, 2.0, color);
        }
    }
}

fn draw_sliders_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    for (dy, knob_x) in [(-4.5, -2.5), (0.0, 3.0), (4.5, -1.5)] {
        p.line_segment(
            [Pos2::new(c.x - 7.0, c.y + dy), Pos2::new(c.x + 7.0, c.y + dy)],
            s,
        );
        p.circle_filled(Pos2::new(c.x + knob_x, c.y + dy), 2.3, color);
    }
}

fn draw_sun_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    p.circle_filled(c, 3.4, color);
    let s = Stroke::new(1.35, color);
    for i in 0..8 {
        let angle = i as f32 * std::f32::consts::FRAC_PI_4;
        let dir = Vec2::new(angle.cos(), angle.sin());
        p.line_segment([c + dir * 5.2, c + dir * 7.2], s);
    }
}

fn draw_moon_icon(p: &egui::Painter, c: Pos2, color: Color32, cutout_bg: Color32) {
    p.circle_filled(c, 5.8, color);
    p.circle_filled(Pos2::new(c.x + 2.6, c.y - 2.0), 4.8, cutout_bg);
}

fn draw_theme_slider_row(
    ui: &mut egui::Ui,
    is_dark: bool,
    anim: f32,
    pal: &ThemePalette,
    cutout_bg: Color32,
) -> bool {
    let mut toggled = false;
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 7.0;

        let (sun_rect, sun_resp) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
        draw_sun_icon(
            ui.painter(),
            sun_rect.center(),
            if !is_dark {
                Color32::from_rgb(240, 165, 36)
            } else {
                pal.text_muted
            },
        );
        if sun_resp.clicked() && is_dark {
            toggled = true;
        }

        let (track_rect, track_resp) =
            ui.allocate_exact_size(Vec2::new(42.0, 22.0), egui::Sense::click());
        let p = ui.painter();
        let track_bg = if is_dark {
            AUTH_BLUE
        } else {
            Color32::from_rgb(198, 214, 232)
        };
        p.rect_filled(track_rect, Rounding::same(11.0), track_bg);

        let knob_x = egui::lerp(
            (track_rect.min.x + 11.0_f32)..=(track_rect.max.x - 11.0_f32),
            anim,
        );
        p.circle_filled(Pos2::new(knob_x, track_rect.center().y), 8.2, Color32::WHITE);

        if track_resp.on_hover_text("Переключить тему").clicked() {
            toggled = true;
        }

        let (moon_rect, moon_resp) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
        draw_moon_icon(
            ui.painter(),
            moon_rect.center(),
            if is_dark {
                Color32::from_rgb(122, 192, 255)
            } else {
                pal.text_muted
            },
            cutout_bg,
        );
        if moon_resp.clicked() && !is_dark {
            toggled = true;
        }
    });
    toggled
}

fn draw_toggle_switch(ui: &mut egui::Ui, value: &mut bool, pal: &ThemePalette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(38.0, 20.0), egui::Sense::click());
    let bg = if *value { pal.accent_blue } else { pal.bg_input };
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
    let knob_x = if *value {
        rect.max.x - 10.0
    } else {
        rect.min.x + 10.0
    };
    ui.painter()
        .circle_filled(Pos2::new(knob_x, rect.center().y), 7.5, Color32::WHITE);
    if resp.clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

fn draw_drawer_row(
    ui: &mut egui::Ui,
    label: &str,
    text_color: Color32,
    pal: &ThemePalette,
    draw_icon: impl FnOnce(&egui::Painter, Pos2, Color32),
) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), egui::Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, Rounding::same(10.0), pal.bg_hover);
    }
    draw_icon(p, Pos2::new(rect.min.x + 22.0, rect.center().y), text_color);
    p.text(
        Pos2::new(rect.min.x + 48.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        text_color,
    );
    resp
}

fn draw_avatar_circle(
    p: &egui::Painter,
    rect: Rect,
    avatar: &Option<egui::TextureHandle>,
    fallback_text: &str,
    fallback_color: Color32,
    font_size: f32,
) {
    if let Some(tex) = avatar {
        p.image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        p.circle_filled(rect.center(), rect.width() * 0.5, fallback_color);
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            fallback_text,
            semibold_font(font_size),
            Color32::WHITE,
        );
    }
}

fn draw_paperclip_button(ui: &mut egui::Ui, is_open: bool, pal: &ThemePalette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
    let p = ui.painter();
    let c = rect.center();
    if response.hovered() || is_open {
        p.circle_filled(c, 17.0, pal.bg_hover);
    }
    let color = if is_open || response.hovered() {
        pal.accent_light
    } else {
        pal.text_muted
    };
    let s = Stroke::new(1.8, color);
    p.line_segment(
        [Pos2::new(c.x - 5.0, c.y + 1.0), Pos2::new(c.x + 3.0, c.y - 7.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 3.0, c.y - 7.0), Pos2::new(c.x + 7.0, c.y - 3.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 7.0, c.y - 3.0), Pos2::new(c.x - 2.0, c.y + 6.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.0, c.y + 6.0), Pos2::new(c.x - 5.5, c.y + 2.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 5.5, c.y + 2.5), Pos2::new(c.x + 1.5, c.y - 4.5)],
        s,
    );
    response
}

fn draw_copy_icon(p: &egui::Painter, c: Pos2, color: Color32, fill_bg: Color32) {
    let s = Stroke::new(1.5, color);
    let back = Rect::from_min_size(Pos2::new(c.x - 6.0, c.y - 7.0), Vec2::new(9.5, 11.0));
    let front = Rect::from_min_size(Pos2::new(c.x - 2.5, c.y - 3.5), Vec2::new(9.5, 11.0));
    p.rect_stroke(back, Rounding::same(2.0), s);
    p.rect_filled(front, Rounding::same(2.0), fill_bg);
    p.rect_stroke(front, Rounding::same(2.0), s);
}

fn draw_forward_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    p.line_segment(
        [Pos2::new(c.x - 6.5, c.y + 1.0), Pos2::new(c.x + 5.5, c.y + 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 1.5, c.y - 3.5), Pos2::new(c.x + 6.0, c.y + 1.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 1.5, c.y + 5.5), Pos2::new(c.x + 6.0, c.y + 1.0)],
        s,
    );
}

fn draw_trash_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    let s = Stroke::new(1.5, color);
    p.line_segment(
        [Pos2::new(c.x - 6.5, c.y - 4.5), Pos2::new(c.x + 6.5, c.y - 4.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.5, c.y - 7.0), Pos2::new(c.x + 2.5, c.y - 7.0)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x - 2.5, c.y - 7.0), Pos2::new(c.x - 2.5, c.y - 4.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 2.5, c.y - 7.0), Pos2::new(c.x + 2.5, c.y - 4.5)],
        s,
    );
    let body = Rect::from_min_max(
        Pos2::new(c.x - 5.0, c.y - 4.5),
        Pos2::new(c.x + 5.0, c.y + 6.5),
    );
    p.rect_stroke(body, Rounding::same(1.5), s);
    p.line_segment(
        [Pos2::new(c.x - 1.8, c.y - 1.5), Pos2::new(c.x - 1.8, c.y + 3.5)],
        s,
    );
    p.line_segment(
        [Pos2::new(c.x + 1.8, c.y - 1.5), Pos2::new(c.x + 1.8, c.y + 3.5)],
        s,
    );
}

fn draw_menu_icon(p: &egui::Painter, c: Pos2, kind: AttachmentKind, color: Color32) {
    let s = Stroke::new(1.6, color);
    match kind {
        AttachmentKind::Media => {
            let r = Rect::from_center_size(c, Vec2::new(16.0, 13.0));
            p.rect_stroke(r, Rounding::same(2.5), s);
            p.circle_filled(Pos2::new(c.x - 3.5, c.y - 2.5), 1.5, color);
            p.line_segment(
                [Pos2::new(r.min.x + 2.0, r.max.y - 2.0), Pos2::new(c.x - 1.0, c.y + 0.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x - 1.0, c.y + 0.5), Pos2::new(c.x + 2.0, c.y + 3.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 0.5, c.y + 1.5), Pos2::new(c.x + 4.0, c.y - 1.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 4.0, c.y - 1.5), Pos2::new(r.max.x - 1.5, r.max.y - 2.0)],
                s,
            );
        }
        AttachmentKind::Document => {
            let r = Rect::from_center_size(c, Vec2::new(13.0, 16.0));
            p.rect_stroke(r, Rounding::same(2.0), s);
            p.line_segment(
                [Pos2::new(c.x - 3.0, c.y - 2.0), Pos2::new(c.x + 3.0, c.y - 2.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x - 3.0, c.y + 2.0), Pos2::new(c.x + 3.0, c.y + 2.0)],
                s,
            );
        }
        AttachmentKind::Poll => {
            p.line_segment(
                [Pos2::new(c.x - 5.0, c.y + 6.0), Pos2::new(c.x - 5.0, c.y)],
                s,
            );
            p.line_segment([Pos2::new(c.x, c.y + 6.0), Pos2::new(c.x, c.y - 6.0)], s);
            p.line_segment(
                [Pos2::new(c.x + 5.0, c.y + 6.0), Pos2::new(c.x + 5.0, c.y - 2.0)],
                s,
            );
        }
        AttachmentKind::Article => {
            let r = Rect::from_center_size(c, Vec2::new(15.0, 15.0));
            p.rect_stroke(r, Rounding::same(2.0), s);
            p.line_segment(
                [Pos2::new(c.x - 4.0, c.y - 3.5), Pos2::new(c.x + 4.0, c.y - 3.5)],
                s,
            );
            p.line_segment([Pos2::new(c.x - 4.0, c.y), Pos2::new(c.x + 4.0, c.y)], s);
            p.line_segment(
                [Pos2::new(c.x - 4.0, c.y + 3.5), Pos2::new(c.x + 1.5, c.y + 3.5)],
                s,
            );
        }
        AttachmentKind::Location => {
            p.circle_stroke(Pos2::new(c.x, c.y - 2.0), 5.0, s);
            p.circle_filled(Pos2::new(c.x, c.y - 2.0), 1.6, color);
            p.line_segment(
                [Pos2::new(c.x - 3.5, c.y + 2.0), Pos2::new(c.x, c.y + 7.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 3.5, c.y + 2.0), Pos2::new(c.x, c.y + 7.5)],
                s,
            );
        }
        AttachmentKind::Music => {
            p.circle_stroke(Pos2::new(c.x - 3.5, c.y + 4.5), 2.3, s);
            p.circle_stroke(Pos2::new(c.x + 4.5, c.y + 3.0), 2.3, s);
            p.line_segment(
                [Pos2::new(c.x - 1.2, c.y + 4.5), Pos2::new(c.x - 1.2, c.y - 5.0)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x + 6.8, c.y + 3.0), Pos2::new(c.x + 6.8, c.y - 6.5)],
                s,
            );
            p.line_segment(
                [Pos2::new(c.x - 1.2, c.y - 5.0), Pos2::new(c.x + 6.8, c.y - 6.5)],
                s,
            );
        }
    }
}

fn draw_send_button(ui: &mut egui::Ui, is_ready: bool, pal: &ThemePalette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
    let painter = ui.painter();
    let center = rect.center();
    let fill_color = if !is_ready {
        pal.bg_input
    } else if response.hovered() {
        pal.accent_blue_hover
    } else {
        pal.accent_blue
    };
    painter.circle_filled(center, 17.5, fill_color);
    let arrow_color = if is_ready {
        Color32::WHITE
    } else {
        pal.text_muted
    };
    let stroke = Stroke::new(2.4, arrow_color);
    let top = Pos2::new(center.x, center.y - 6.5);
    painter.line_segment([Pos2::new(center.x, center.y + 6.5), top], stroke);
    painter.line_segment([top, Pos2::new(center.x - 5.2, center.y - 1.3)], stroke);
    painter.line_segment([top, Pos2::new(center.x + 5.2, center.y - 1.3)], stroke);
    response
}

fn draw_call_circle_btn(
    ui: &mut egui::Ui,
    bg: Color32,
    hover_bg: Color32,
    label: &str,
    draw_icon: impl FnOnce(&egui::Painter, Pos2),
) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::click());
        let fill = if resp.hovered() { hover_bg } else { bg };
        ui.painter().circle_filled(rect.center(), 25.5, fill);
        draw_icon(ui.painter(), rect.center());
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(label)
                .size(11.5)
                .color(Color32::from_rgb(198, 214, 232)),
        );
        if resp.clicked() {
            clicked = true;
        }
    });
    clicked
}

impl EgrammApp {
    fn show_call_window(&mut self, ctx: &egui::Context) {
        if self.call_status == CallStatus::Idle || !self.call_window_expanded {
            return;
        }

        ctx.request_repaint_after(Duration::from_millis(33));

        let status = self.call_status.clone();
        let (
            chat_id,
            call_id,
            peer_name,
            peer_tag,
            peer_color,
            status_line,
            status_color,
            pulse_t,
            is_incoming,
            is_connected,
            participant_count,
        ) = match &status {
            CallStatus::Outgoing {
                chat_id,
                call_id,
                peer_name,
                peer_tag,
                peer_color,
                started_at,
            } => {
                let secs = started_at.elapsed().as_secs_f32();
                let dots = ".".repeat(((secs * 2.2) as usize % 3) + 1);
                let lbl = if secs < 0.7 {
                    format!("Подготовка{}", dots)
                } else if secs < 1.5 {
                    format!("Подключение{}", dots)
                } else {
                    format!("Вызов{}", dots)
                };
                (
                    *chat_id,
                    call_id.clone(),
                    peer_name.clone(),
                    peer_tag.clone(),
                    *peer_color,
                    lbl,
                    Color32::from_rgb(138, 198, 255),
                    secs,
                    false,
                    false,
                    1_usize,
                )
            }
            CallStatus::Incoming {
                chat_id,
                call_id,
                caller_name,
                peer_color,
                received_at,
                ..
            } => {
                let tag: String = caller_name
                    .chars()
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                let secs = received_at.elapsed().as_secs_f32();
                (
                    *chat_id,
                    call_id.clone(),
                    caller_name.clone(),
                    tag,
                    *peer_color,
                    "Входящий голосовой звонок".to_string(),
                    ONLINE_GREEN,
                    secs,
                    true,
                    false,
                    2_usize,
                )
            }
            CallStatus::Connected {
                chat_id,
                call_id,
                peer_name,
                peer_tag,
                peer_color,
                started_at,
                handshaking_until,
                participants,
            } => {
                let now = Instant::now();
                let secs_f = started_at.elapsed().as_secs_f32();
                let cnt = participants.len().max(2).min(MAX_CALL_PARTICIPANTS);
                let (lbl, col) = if now < *handshaking_until {
                    (
                        format!("Подключение ({}/{})...", cnt, MAX_CALL_PARTICIPANTS),
                        Color32::from_rgb(138, 198, 255),
                    )
                } else {
                    let s = started_at.elapsed().as_secs();
                    (
                        format!(
                            "{:02}:{:02}  •  {}/{} участн.",
                            s / 60,
                            s % 60,
                            cnt,
                            MAX_CALL_PARTICIPANTS
                        ),
                        ONLINE_GREEN,
                    )
                };
                (
                    *chat_id,
                    call_id.clone(),
                    peer_name.clone(),
                    peer_tag.clone(),
                    *peer_color,
                    lbl,
                    col,
                    secs_f,
                    false,
                    true,
                    cnt,
                )
            }
            CallStatus::Idle => return,
        };

        let peer_avatar: Option<egui::TextureHandle> = self
            .chats
            .iter()
            .find(|c| c.server_chat_id == Some(chat_id))
            .and_then(|c| c.avatar_tex.clone());

        let fingerprint = call_encryption_fingerprint(chat_id, &call_id);
        let remote_lvl = self.audio_bridge.remote_level_f32();
        let mic_lvl = self.audio_bridge.mic_level_f32();
        let mic_muted = self.audio_bridge.mic_muted.load(Ordering::Relaxed);
        let spk_muted = self.audio_bridge.speaker_muted.load(Ordering::Relaxed);
        let mut vol_pct = (self.audio_bridge.speaker_volume_f32() * 100.0).round() as i32;
        let mut mic_pct = (self.audio_bridge.mic_gain_f32() * 100.0).round() as i32;

        let mut do_minimize = false;
        let mut do_accept = false;
        let mut do_hangup = false;
        let mut do_toggle_mic = false;
        let mut do_toggle_spk = false;

        let win_h = if is_incoming { 375.0 } else { 475.0 };

        egui::Window::new("egramm_tg_call_window")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_pos(Pos2::new(
                (ctx.screen_rect().center().x - 175.0).max(40.0),
                (ctx.screen_rect().center().y - win_h * 0.5).max(30.0),
            ))
            .fixed_size(Vec2::new(350.0, win_h))
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(17, 26, 38))
                    .stroke(Stroke::new(1.2, Color32::from_rgb(48, 74, 104)))
                    .rounding(Rounding::same(22.0))
                    .shadow(egui::epaint::Shadow {
                        offset: Vec2::new(0.0, 12.0),
                        blur: 28.0,
                        spread: 2.0,
                        color: Color32::from_rgba_unmultiplied(0, 0, 0, 150),
                    }),
            )
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized(
                                    Vec2::splat(28.0),
                                    egui::Button::new(
                                        egui::RichText::new("—")
                                            .size(14.0)
                                            .color(Color32::from_rgb(168, 190, 214)),
                                    )
                                        .fill(Color32::from_rgb(28, 41, 58))
                                        .rounding(14.0),
                                )
                                .on_hover_text("Свернуть окно звонка в шапку чата")
                                .clicked()
                            {
                                do_minimize = true;
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (fp_rect, fp_resp) = ui
                                    .allocate_exact_size(Vec2::new(112.0, 26.0), egui::Sense::hover());
                                ui.painter().rect_filled(
                                    fp_rect,
                                    Rounding::same(13.0),
                                    Color32::from_rgb(25, 38, 54),
                                );
                                draw_lock_icon(
                                    ui.painter(),
                                    Pos2::new(fp_rect.min.x + 15.0, fp_rect.center().y),
                                    ONLINE_GREEN,
                                );
                                for (idx, (glyph_kind, glyph_col)) in fingerprint.iter().enumerate()
                                {
                                    let gx = fp_rect.min.x + 36.0 + idx as f32 * 20.0;
                                    draw_call_fingerprint_glyph(
                                        ui.painter(),
                                        Pos2::new(gx, fp_rect.center().y),
                                        *glyph_kind,
                                        *glyph_col,
                                    );
                                }
                                fp_resp.on_hover_text(format!(
                                    "Голосовая комната ({}/{} участн.).\nChaCha20-Poly1305: символы проверки ключа канала.",
                                    participant_count, MAX_CALL_PARTICIPANTS
                                ));
                            });
                        });

                        ui.add_space(16.0);

                        ui.vertical_centered(|ui| {
                            let (av_box, _) =
                                ui.allocate_exact_size(Vec2::splat(134.0), egui::Sense::hover());
                            let c = av_box.center();
                            let p = ui.painter();

                            let base_r = 47.0;
                            let ring_col = if is_connected || is_incoming {
                                ONLINE_GREEN
                            } else {
                                AUTH_BLUE
                            };

                            if is_connected {
                                let voice_boost = remote_lvl * 22.0;
                                p.circle_filled(
                                    c,
                                    base_r + 14.0 + voice_boost,
                                    with_alpha(ring_col, 0.14),
                                );
                                p.circle_filled(
                                    c,
                                    base_r + 7.0 + voice_boost * 0.55,
                                    with_alpha(ring_col, 0.24),
                                );
                            } else {
                                let wave1 = (pulse_t * 1.4).fract();
                                let wave2 = (pulse_t * 1.4 + 0.5).fract();
                                p.circle_stroke(
                                    c,
                                    base_r + wave1 * 24.0,
                                    Stroke::new(2.0, with_alpha(ring_col, (1.0 - wave1) * 0.55)),
                                );
                                p.circle_stroke(
                                    c,
                                    base_r + wave2 * 24.0,
                                    Stroke::new(2.0, with_alpha(ring_col, (1.0 - wave2) * 0.55)),
                                );
                            }

                            let av_rect = Rect::from_center_size(c, Vec2::splat(base_r * 2.0));
                            draw_avatar_circle(
                                p,
                                av_rect,
                                &peer_avatar,
                                &peer_tag,
                                peer_color,
                                30.0,
                            );

                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new(&peer_name)
                                    .font(semibold_font(21.0))
                                    .color(Color32::WHITE),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(&status_line)
                                    .font(semibold_font(13.0))
                                    .color(status_color),
                            );
                        });

                        if !is_incoming {
                            ui.add_space(16.0);
                            egui::Frame::none()
                                .fill(Color32::from_rgb(24, 36, 51))
                                .rounding(Rounding::same(14.0))
                                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Громкость")
                                                .size(12.5)
                                                .color(Color32::from_rgb(176, 198, 222)),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.spacing_mut().slider_width = 165.0;
                                                if ui
                                                    .add(
                                                        egui::Slider::new(&mut vol_pct, 0..=200)
                                                            .suffix("%"),
                                                    )
                                                    .changed()
                                                {
                                                    self.audio_bridge
                                                        .set_speaker_volume(vol_pct as f32 / 100.0);
                                                }
                                            },
                                        );
                                    });

                                    ui.add_space(6.0);

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Микрофон")
                                                .size(12.5)
                                                .color(Color32::from_rgb(176, 198, 222)),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.spacing_mut().slider_width = 165.0;
                                                if ui
                                                    .add(
                                                        egui::Slider::new(&mut mic_pct, 0..=200)
                                                            .suffix("%"),
                                                    )
                                                    .changed()
                                                {
                                                    self.audio_bridge
                                                        .set_mic_gain(mic_pct as f32 / 100.0);
                                                }
                                            },
                                        );
                                    });

                                    ui.add_space(7.0);
                                    let (bar_rect, _) = ui.allocate_exact_size(
                                        Vec2::new(ui.available_width(), 5.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        bar_rect,
                                        Rounding::same(2.5),
                                        Color32::from_rgb(16, 24, 35),
                                    );
                                    let level = if mic_muted {
                                        0.0
                                    } else {
                                        mic_lvl.clamp(0.0, 1.0)
                                    };
                                    if level > 0.01 {
                                        let fill_rect = Rect::from_min_size(
                                            bar_rect.min,
                                            Vec2::new(bar_rect.width() * level, bar_rect.height()),
                                        );
                                        ui.painter().rect_filled(
                                            fill_rect,
                                            Rounding::same(2.5),
                                            ONLINE_GREEN,
                                        );
                                    }
                                });
                        }

                        ui.add_space(20.0);

                        if is_incoming {
                            ui.columns(2, |cols| {
                                if draw_call_circle_btn(
                                    &mut cols[0],
                                    DANGER_RED,
                                    Color32::from_rgb(248, 112, 112),
                                    "Отклонить",
                                    |p, c| draw_hangup_icon(p, c, Color32::WHITE),
                                ) {
                                    do_hangup = true;
                                }
                                if draw_call_circle_btn(
                                    &mut cols[1],
                                    ONLINE_GREEN,
                                    Color32::from_rgb(88, 214, 138),
                                    "Принять",
                                    |p, c| draw_phone_icon(p, c, Color32::WHITE),
                                ) {
                                    do_accept = true;
                                }
                            });
                        } else {
                            ui.columns(3, |cols| {
                                let spk_bg = if spk_muted {
                                    Color32::from_rgb(66, 36, 44)
                                } else {
                                    Color32::from_rgb(35, 50, 70)
                                };
                                if draw_call_circle_btn(
                                    &mut cols[0],
                                    spk_bg,
                                    Color32::from_rgb(48, 66, 92),
                                    if spk_muted { "Без звука" } else { "Звук" },
                                    |p, c| draw_speaker_icon(p, c, spk_muted, Color32::WHITE),
                                ) {
                                    do_toggle_spk = true;
                                }

                                let mic_bg = if mic_muted {
                                    Color32::from_rgb(66, 36, 44)
                                } else {
                                    Color32::from_rgb(35, 50, 70)
                                };
                                if draw_call_circle_btn(
                                    &mut cols[1],
                                    mic_bg,
                                    Color32::from_rgb(48, 66, 92),
                                    if mic_muted { "Выкл." } else { "Микрофон" },
                                    |p, c| draw_mic_icon(p, c, mic_muted, Color32::WHITE),
                                ) {
                                    do_toggle_mic = true;
                                }

                                if draw_call_circle_btn(
                                    &mut cols[2],
                                    DANGER_RED,
                                    Color32::from_rgb(248, 112, 112),
                                    "Сбросить",
                                    |p, c| draw_hangup_icon(p, c, Color32::WHITE),
                                ) {
                                    do_hangup = true;
                                }
                            });
                        }
                    });
            });

        if do_minimize {
            self.call_window_expanded = false;
        }
        if do_toggle_mic {
            self.audio_bridge.mic_muted.store(!mic_muted, Ordering::Relaxed);
        }
        if do_toggle_spk {
            self.audio_bridge
                .speaker_muted
                .store(!spk_muted, Ordering::Relaxed);
        }
        if do_accept {
            self.accept_incoming_call();
        }
        if do_hangup {
            self.end_active_call(true);
        }
    }
}
impl eframe::App for EgrammApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let target = if self.is_dark_mode { 1.0 } else { 0.0 };
        if (self.theme_anim - target).abs() > 0.005 {
            let dt = ctx.input(|i| i.stable_dt).min(0.05);
            let step = dt * 10.0;
            if self.theme_anim < target {
                self.theme_anim = (self.theme_anim + step).min(target);
            } else {
                self.theme_anim = (self.theme_anim - step).max(target);
            }
            ctx.request_repaint();
        } else {
            self.theme_anim = target;
        }

        while let Ok(event) = self.net_rx.try_recv() {
            match event {
                NetEvent::AuthUrlReady(url) => {
                    if self.is_authenticating {
                        self.auth_url = Some(url);
                    }
                }
                NetEvent::AuthSuccess {
                    auth_attempt,
                    name,
                    email,
                    token,
                    avatar_file_id,
                    bio,
                } => {
                    if auth_attempt != self.auth_attempt {
                        continue;
                    }
                    self.username = name.clone();
                    self.user_email = email.clone();
                    self.user_avatar_id = avatar_file_id.clone();
                    self.user_bio = bio;
                    self.audio_bridge.set_my_email(email.clone());
                    let nick = email.split('@').next().unwrap_or("user");
                    self.user_tag = format!("@{}", nick);
                    self.session_token = Some(token);
                    self.is_authenticating = false;
                    self.auth_url = None;
                    self.auth_error = None;
                    self.is_logged_in = true;
                    self.chat_switch_time = Instant::now();

                    if !self.has_played_intro {
                        self.intro_start = Some(Instant::now());
                        self.has_played_intro = true;
                    }

                    if let Some(fid) = avatar_file_id {
                        self.request_avatar_download(ctx, &fid);
                    }
                    self.start_realtime_sync(ctx.clone());
                }
                NetEvent::AuthError { auth_attempt, msg } => {
                    if auth_attempt != self.auth_attempt {
                        continue;
                    }
                    self.is_authenticating = false;
                    self.auth_url = None;
                    self.auth_error = Some(msg);
                }
                NetEvent::InitialSync {
                    chats: server_chats,
                    online_emails,
                } => {
                    let mut set: HashSet<String> = online_emails.into_iter().collect();
                    if !self.user_email.is_empty() {
                        set.insert(self.user_email.clone());
                    }
                    self.online_emails = set.clone();

                    self.chats.retain(|c| c.server_chat_id.is_none());
                    for (idx, sc) in server_chats.into_iter().enumerate() {
                        let chat = self.build_chat_from_dto(sc, idx + 1);
                        self.chats.push(chat);
                    }
                    self.apply_presence_set(&set);
                    self.selected_chat = Some(0);
                    self.apply_avatar_textures_to_state(ctx);
                }
                NetEvent::PresenceUpdate(mut set) => {
                    if !self.user_email.is_empty() {
                        set.insert(self.user_email.clone());
                    }
                    self.apply_presence_set(&set);
                }
                NetEvent::ProfileUpdated {
                    email,
                    name,
                    tag,
                    bio,
                } => {
                    if email == self.user_email {
                        continue;
                    }
                    for chat in &mut self.chats {
                        if !chat.is_group && chat.peer_email.as_deref() == Some(&email) {
                            chat.title = name.clone();
                            chat.avatar_tag =
                                name.chars().take(2).collect::<String>().to_uppercase();
                            chat.peer_bio = bio.clone();
                        }
                        for m in &mut chat.members {
                            if m.email.as_deref() == Some(&email) {
                                m.name = name.clone();
                                m.tag = tag.clone();
                                m.bio = bio.clone();
                            }
                        }
                    }
                    for u in &mut self.user_search_results {
                        if u.email == email {
                            u.name = name.clone();
                            u.tag = tag.clone();
                            u.bio = bio.clone();
                        }
                    }
                    if let Some(vp) = &mut self.viewed_profile {
                        if vp.email == email {
                            vp.name = name;
                            vp.tag = tag;
                            vp.bio = bio;
                        }
                    }
                }
                NetEvent::AvatarFileReady { file_id, bytes } => {
                    self.fetching_avatars.remove(&file_id);
                    if let Some(tex) =
                        load_circular_avatar_from_bytes(ctx, &bytes, &format!("av_{}", file_id))
                    {
                        self.avatar_tex_cache.insert(file_id.clone(), tex.clone());
                        if self.user_avatar_id.as_deref() == Some(&file_id) {
                            self.user_avatar = Some(tex.clone());
                        }
                        for chat in &mut self.chats {
                            if chat.avatar_file_id.as_deref() == Some(&file_id)
                                || chat.peer_avatar_id.as_deref() == Some(&file_id)
                            {
                                chat.avatar_tex = Some(tex.clone());
                            }
                            for m in &mut chat.members {
                                if m.avatar_file_id.as_deref() == Some(&file_id) {
                                    m.avatar_tex = Some(tex.clone());
                                }
                            }
                        }
                        if let Some(vp) = &mut self.viewed_profile {
                            if vp.avatar_file_id.as_deref() == Some(&file_id) {
                                vp.avatar_tex = Some(tex);
                            }
                        }
                    }
                }
                NetEvent::UserAvatarUpdated {
                    email,
                    avatar_file_id,
                } => {
                    if email == self.user_email {
                        self.user_avatar_id = avatar_file_id.clone();
                        if avatar_file_id.is_none() {
                            self.user_avatar = None;
                        }
                    }
                    for chat in &mut self.chats {
                        if chat.peer_email.as_deref() == Some(&email) {
                            chat.peer_avatar_id = avatar_file_id.clone();
                            if avatar_file_id.is_none() {
                                chat.avatar_tex = None;
                            }
                        }
                        for m in &mut chat.members {
                            if m.email.as_deref() == Some(&email) {
                                m.avatar_file_id = avatar_file_id.clone();
                                if avatar_file_id.is_none() {
                                    m.avatar_tex = None;
                                }
                            }
                        }
                    }
                    if let Some(fid) = avatar_file_id {
                        self.request_avatar_download(ctx, &fid);
                    }
                }
                NetEvent::GroupAvatarUpdated {
                    chat_id,
                    avatar_file_id,
                } => {
                    for chat in &mut self.chats {
                        if chat.server_chat_id == Some(chat_id) {
                            chat.avatar_file_id = avatar_file_id.clone();
                            if avatar_file_id.is_none() {
                                chat.avatar_tex = None;
                            }
                        }
                    }
                    if let Some(fid) = avatar_file_id {
                        self.request_avatar_download(ctx, &fid);
                    }
                }
                NetEvent::IncomingChat {
                    chat,
                    allowed_emails,
                } => {
                    if !allowed_emails.contains(&self.user_email) {
                        continue;
                    }
                    if let Some(pos) = self
                        .chats
                        .iter()
                        .position(|c| c.server_chat_id == Some(chat.id))
                    {
                        let updated = self.build_chat_from_dto(chat, pos + 1);
                        self.chats[pos] = updated;
                    } else {
                        let color_idx = self.chats.len() + 1;
                        let new_chat = self.build_chat_from_dto(chat, color_idx);
                        self.chats.push(new_chat);
                    }
                    self.apply_avatar_textures_to_state(ctx);
                }
                NetEvent::MemberInvited {
                    chat,
                    added_member: _,
                } => {
                    if let Some(pos) = self
                        .chats
                        .iter()
                        .position(|c| c.server_chat_id == Some(chat.id))
                    {
                        let updated = self.build_chat_from_dto(chat, pos + 1);
                        self.chats[pos] = updated;
                        self.apply_avatar_textures_to_state(ctx);
                    }
                }
                NetEvent::ChatOpened(sc) => {
                    if let Some(pos) = self
                        .chats
                        .iter()
                        .position(|c| c.server_chat_id == Some(sc.id))
                    {
                        self.selected_chat = Some(pos);
                        self.chat_switch_time = Instant::now();
                        self.chats[pos].unread = 0;
                        self.send_mark_read(pos);
                    } else {
                        let color_idx = self.chats.len() + 1;
                        let new_chat = self.build_chat_from_dto(sc, color_idx);
                        let new_idx = self.chats.len();
                        self.chats.push(new_chat);
                        self.selected_chat = Some(new_idx);
                        self.chat_switch_time = Instant::now();
                        self.send_mark_read(new_idx);
                        self.apply_avatar_textures_to_state(ctx);
                    }
                    self.search_query.clear();
                    self.user_search_results.clear();
                    self.group_search_results.clear();
                }
                NetEvent::MessagesRead {
                    chat_id,
                    reader_email,
                } => {
                    if reader_email != self.user_email {
                        if let Some(chat) = self
                            .chats
                            .iter_mut()
                            .find(|c| c.server_chat_id == Some(chat_id))
                        {
                            for msg in &mut chat.messages {
                                if msg.is_me {
                                    msg.is_read = true;
                                }
                            }
                        }
                    }
                }
                NetEvent::SearchResults { users, groups } => {
                    for u in &users {
                        if let Some(fid) = &u.avatar_file_id {
                            self.request_avatar_download(ctx, fid);
                        }
                    }
                    for g in &groups {
                        if let Some(fid) = &g.avatar_file_id {
                            self.request_avatar_download(ctx, fid);
                        }
                    }
                    self.user_search_results = users;
                    self.group_search_results = groups;
                }
                NetEvent::WsStatus(connected) => {
                    let was_disconnected = !self.ws_connected && connected;
                    self.ws_connected = connected;
                    if was_disconnected {
                        if let (Some(token), Some(ws_tx)) = (&self.session_token, &self.ws_out_tx)
                        {
                            let auth_ev = WsWireEvent::AuthWs {
                                session_token: token.clone(),
                            };
                            if let Ok(json) = serde_json::to_string(&auth_ev) {
                                let _ = ws_tx.send(json);
                            }
                        }
                        if let Some(sel) = self.selected_chat {
                            self.send_mark_read(sel);
                        }
                    }
                }
                NetEvent::IncomingMessage(m) => {
                    let target_idx = self
                        .chats
                        .iter()
                        .position(|c| c.server_chat_id == Some(m.chat_id));
                    if let Some(c_idx) = target_idx {
                        let already_exists = self.chats[c_idx]
                            .messages
                            .iter()
                            .any(|existing| existing.server_id == Some(m.id));
                        if !already_exists {
                            let mut decoded = self.decode_server_message(m);
                            let is_me = decoded.is_me;
                            let is_current_chat = self.selected_chat == Some(c_idx);
                            let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));

                            if !is_me {
                                self.sound_player.play_notify();
                            }

                            if is_current_chat && !is_me {
                                decoded.is_read = false;
                                self.chats[c_idx].messages.push(decoded);
                                self.send_mark_read(c_idx);
                                if let Some(last) = self.chats[c_idx].messages.last_mut() {
                                    last.is_read = true;
                                }
                                continue;
                            }

                            if !is_me && (!is_current_chat || !window_focused) {
                                let preview = if !decoded.text.is_empty() {
                                    decoded.text.clone()
                                } else if let Some(att) = &decoded.attachment {
                                    format!("[{}] {}", att.kind.tag(), att.title)
                                } else {
                                    "Новое сообщение".to_string()
                                };
                                self.show_toast(format!("{}: {}", decoded.sender, preview));
                                notify_windows_message(&decoded.sender, &preview);
                            }

                            let should_inc_unread = !is_me && !is_current_chat;
                            self.chats[c_idx].messages.push(decoded);
                            if should_inc_unread {
                                self.chats[c_idx].unread += 1;
                            }
                        }
                    }
                }
                NetEvent::MessageDeleted {
                    chat_id,
                    message_id,
                } => {
                    if let Some(chat) = self
                        .chats
                        .iter_mut()
                        .find(|c| c.server_chat_id == Some(chat_id))
                    {
                        chat.messages.retain(|msg| msg.server_id != Some(message_id));
                    }
                }
                NetEvent::FileReady {
                    drive_file_id,
                    local_path,
                    open_after,
                } => {
                    self.downloading_drive_ids.remove(&drive_file_id);
                    for chat in &mut self.chats {
                        for msg in &mut chat.messages {
                            if let Some(att) = &mut msg.attachment {
                                if att.drive_file_id.as_deref() == Some(&drive_file_id) {
                                    att.local_path = Some(local_path.clone());
                                }
                            }
                        }
                    }
                    if open_after {
                        let _ = open::that(&local_path);
                    }
                }
                NetEvent::CallInvite {
                    chat_id,
                    call_id,
                    caller_name,
                    caller_email,
                } => {
                    if caller_email == self.user_email {
                        continue;
                    }
                    if self.call_status != CallStatus::Idle {
                        continue;
                    }
                    let chat_info = self
                        .chats
                        .iter()
                        .find(|c| c.server_chat_id == Some(chat_id))
                        .map(|c| (c.avatar_color, c.title.clone(), c.is_group));

                    let (peer_color, display_name, is_group) =
                        chat_info.unwrap_or_else(|| (AUTH_BLUE, caller_name.clone(), false));

                    if let Some(pos) = self
                        .chats
                        .iter()
                        .position(|c| c.server_chat_id == Some(chat_id))
                    {
                        self.selected_chat = Some(pos);
                    }
                    self.call_window_expanded = true;
                    let final_cid = if call_id.is_empty() {
                        generate_call_id()
                    } else {
                        call_id
                    };
                    self.call_status = CallStatus::Incoming {
                        chat_id,
                        call_id: final_cid,
                        caller_name: if is_group {
                            format!("{} ({})", display_name, caller_name)
                        } else {
                            caller_name.clone()
                        },
                        caller_email,
                        peer_color,
                        received_at: Instant::now(),
                    };
                    notify_windows_message(&caller_name, "Входящий голосовой звонок");
                    self.sound_player.start_ringtone();
                }
                NetEvent::CallAccept {
                    chat_id,
                    call_id,
                    peer_email,
                    participants,
                } => {
                    self.sound_player.stop_ringtone();
                    if peer_email == self.user_email {
                        continue;
                    }

                    let mut updated_parts = if participants.is_empty() {
                        vec![self.user_email.clone(), peer_email.clone()]
                    } else {
                        participants
                    };
                    if !self.user_email.is_empty() && !updated_parts.contains(&self.user_email) {
                        updated_parts.insert(0, self.user_email.clone());
                    }
                    if !peer_email.is_empty() && !updated_parts.contains(&peer_email) {
                        updated_parts.push(peer_email);
                    }

                    match self.call_status.clone() {
                        CallStatus::Outgoing {
                            chat_id: out_cid,
                            call_id: out_call_id,
                            peer_name,
                            peer_tag,
                            peer_color,
                            ..
                        } if out_cid == chat_id => {
                            let final_cid = if !call_id.is_empty() {
                                call_id
                            } else {
                                out_call_id
                            };
                            if let (Some(token), Some(ws_tx)) =
                                (self.session_token.clone(), self.ws_out_tx.clone())
                            {
                                match VoiceEngine::start(
                                    ws_tx,
                                    token,
                                    chat_id,
                                    final_cid.clone(),
                                    self.user_email.clone(),
                                    self.audio_bridge.clone(),
                                ) {
                                    Ok(engine) => {
                                        self.voice_engine = Some(engine);
                                        self.call_window_expanded = true;
                                        let now = Instant::now();
                                        self.call_status = CallStatus::Connected {
                                            chat_id,
                                            call_id: final_cid,
                                            peer_name,
                                            peer_tag,
                                            peer_color,
                                            started_at: now,
                                            handshaking_until: now + Duration::from_millis(650),
                                            participants: updated_parts,
                                        };
                                    }
                                    Err(e) => {
                                        self.show_toast(format!("Ошибка микрофона: {}", e));
                                        self.end_active_call(true);
                                    }
                                }
                            }
                        }
                        CallStatus::Connected {
                            chat_id: cur_cid,
                            call_id: cur_call_id,
                            peer_name,
                            peer_tag,
                            peer_color,
                            started_at,
                            handshaking_until,
                            ..
                        } if cur_cid == chat_id => {
                            self.call_status = CallStatus::Connected {
                                chat_id: cur_cid,
                                call_id: if !call_id.is_empty() {
                                    call_id
                                } else {
                                    cur_call_id
                                },
                                peer_name,
                                peer_tag,
                                peer_color,
                                started_at,
                                handshaking_until,
                                participants: updated_parts,
                            };
                        }
                        _ => {}
                    }
                }
                NetEvent::CallHangup {
                    chat_id,
                    from_email,
                    participants: server_parts,
                } => {
                    if from_email == self.user_email {
                        continue;
                    }
                    self.audio_bridge.remove_sender(&from_email);

                    match self.call_status.clone() {
                        CallStatus::Connected {
                            chat_id: cur_cid,
                            call_id,
                            peer_name,
                            peer_tag,
                            peer_color,
                            started_at,
                            handshaking_until,
                            mut participants,
                        } if cur_cid == chat_id => {
                            if !server_parts.is_empty() {
                                participants = server_parts;
                            } else if !from_email.is_empty() {
                                participants.retain(|em| em != &from_email);
                            }
                            if participants.len() >= 2 {
                                self.call_status = CallStatus::Connected {
                                    chat_id: cur_cid,
                                    call_id,
                                    peer_name,
                                    peer_tag,
                                    peer_color,
                                    started_at,
                                    handshaking_until,
                                    participants,
                                };
                                self.show_toast("Один из участников вышел из звонка");
                            } else {
                                self.end_active_call(false);
                                self.show_toast("Звонок завершён");
                            }
                        }
                        CallStatus::Outgoing { chat_id: c, .. }
                        | CallStatus::Incoming { chat_id: c, .. }
                        if c == chat_id =>
                            {
                                self.end_active_call(false);
                                self.show_toast("Звонок завершён");
                            }
                        _ => {}
                    }
                }
                NetEvent::CallRejected { chat_id, reason } => {
                    let matches = match &self.call_status {
                        CallStatus::Outgoing { chat_id: c, .. }
                        | CallStatus::Incoming { chat_id: c, .. }
                        | CallStatus::Connected { chat_id: c, .. } => *c == chat_id,
                        CallStatus::Idle => false,
                    };
                    if matches {
                        self.end_active_call(false);
                    }
                    self.show_toast(reason);
                }
                NetEvent::Toast(txt) => {
                    self.show_toast(txt);
                }
            }
        }

        if !self.is_logged_in {
            self.show_login_screen(ctx);
            return;
        }

        let trimmed_search = self.search_query.trim().to_string();
        if trimmed_search != self.last_searched_query {
            self.last_searched_query = trimmed_search.clone();
            if trimmed_search.is_empty() {
                self.user_search_results.clear();
                self.group_search_results.clear();
            } else {
                self.trigger_user_search(ctx, trimmed_search);
            }
        }

        if let Some(chat_idx) = self.selected_chat {
            let mut to_fetch: Vec<(String, String)> = Vec::new();
            if let Some(chat) = self.chats.get(chat_idx) {
                for msg in &chat.messages {
                    if let Some(att) = &msg.attachment {
                        if att.local_path.is_none() && is_image_name(&att.title) {
                            if let Some(id) = &att.drive_file_id {
                                to_fetch.push((id.clone(), att.title.clone()));
                            }
                        }
                    }
                }
            }
            for (id, title) in to_fetch {
                self.trigger_drive_download(ctx, id, title, false);
            }
        }

        let pal = self.palette();

        if self.selected_chat.is_some()
            && self.lightbox_image.is_none()
            && self.intro_start.is_none()
        {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if let Some(file) = dropped.into_iter().next() {
                if let Some(path) = file.path {
                    self.pending_attachment = Some(Attachment::from_path(path, None));
                    self.attach_menu_open = false;
                }
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.intro_start.is_some() {
                self.intro_start = None;
            } else if self.lightbox_image.is_some() {
                self.lightbox_image = None;
                self.lightbox_zoom = 1.0;
                self.lightbox_offset = Vec2::ZERO;
            } else if self.msg_context_menu.is_some() {
                self.msg_context_menu = None;
            } else if self.attach_menu_open {
                self.attach_menu_open = false;
            } else if self.active_modal != ActiveModal::None {
                self.active_modal = ActiveModal::None;
                self.editing_admin_idx = None;
            } else if self.menu_open {
                self.menu_open = false;
            } else if self.call_status != CallStatus::Idle && self.call_window_expanded {
                self.call_window_expanded = false;
            } else if self.selected_chat.is_some() {
                self.selected_chat = None;
            }
        }

        let switch_elapsed = self.chat_switch_time.elapsed().as_secs_f32();
        if switch_elapsed < 0.22 {
            ctx.request_repaint();
        }
        if self.call_status != CallStatus::Idle {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if let Some(idx) = self.selected_chat {
            if let Some(chat) = self.chats.get(idx) {
                if let Some(last) = chat.messages.last() {
                    if last.created_at.elapsed().as_secs_f32() < 0.22 {
                        ctx.request_repaint();
                    }
                }
            }
        }

        let mut open_dm_email: Option<String> = None;
        let mut join_found_group: Option<ChatDto> = None;

        egui::SidePanel::left("tg_sidebar")
            .default_width(290.0)
            .min_width(240.0)
            .max_width(400.0)
            .frame(egui::Frame::none().fill(pal.bg_panel).stroke(Stroke::new(1.0, pal.border_line)))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(10.0, 10.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let menu_btn = ui.add_sized(
                                Vec2::splat(34.0),
                                egui::Button::new(egui::RichText::new("☰").size(17.0).color(pal.text_muted))
                                    .fill(Color32::TRANSPARENT)
                                    .rounding(17.0),
                            );
                            if menu_btn.on_hover_text("Меню Egramm").clicked() {
                                self.menu_open = !self.menu_open;
                                self.attach_menu_open = false;
                                self.msg_context_menu = None;
                            }

                            let search_margin_x = 10.0;
                            let search_w = (ui.available_width() - search_margin_x * 2.0 - 2.0).max(80.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.search_query)
                                    .hint_text("Поиск: чат, @ник, почта или код EGR-...")
                                    .desired_width(search_w)
                                    .margin(Vec2::new(search_margin_x, 7.0)),
                            );
                        });
                    });

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut clicked_chat = None;

                    for (idx, chat) in self.chats.iter_mut().enumerate() {
                        if !self.search_query.is_empty() {
                            let q = self.search_query.trim().to_lowercase();
                            let match_title = chat.title.to_lowercase().contains(&q);
                            let match_peer = chat
                                .peer_email
                                .as_ref()
                                .map(|e| e.to_lowercase().contains(&q))
                                .unwrap_or(false);
                            let match_code = !chat.invite_code.is_empty()
                                && chat.invite_code.to_lowercase().contains(&q);
                            if !match_title && !match_peer && !match_code {
                                continue;
                            }
                        }

                        let is_selected = self.selected_chat == Some(idx);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), egui::Sense::click());
                        let painter = ui.painter();

                        let row_bg = if is_selected {
                            pal.bg_selected
                        } else if response.hovered() {
                            pal.bg_hover
                        } else {
                            Color32::TRANSPARENT
                        };
                        painter.rect_filled(rect, Rounding::ZERO, row_bg);

                        let avatar_rect = Rect::from_center_size(
                            Pos2::new(rect.min.x + 34.0, rect.center().y),
                            Vec2::splat(44.0),
                        );
                        draw_avatar_circle(painter, avatar_rect, &chat.avatar_tex, &chat.avatar_tag, chat.avatar_color, 14.0);

                        let title_color = if is_selected { pal.text_on_selected } else { pal.text_main };
                        let sub_color = if is_selected { with_alpha(pal.text_on_selected, 0.85) } else { pal.text_muted };

                        let text_x = rect.min.x + 68.0;
                        painter.text(Pos2::new(text_x, rect.min.y + 13.0), egui::Align2::LEFT_TOP, &chat.title, semibold_font(14.5), title_color);

                        let last_time = chat.messages.last().map(|m| m.time.as_str()).unwrap_or("");
                        painter.text(Pos2::new(rect.max.x - 12.0, rect.min.y + 14.0), egui::Align2::RIGHT_TOP, last_time, FontId::proportional(11.5), sub_color);

                        let preview_owned = chat
                            .messages
                            .last()
                            .map(|m| {
                                if !m.text.is_empty() {
                                    m.text.clone()
                                } else if let Some(att) = &m.attachment {
                                    format!("[{}] {}", att.kind.tag(), att.title)
                                } else {
                                    String::new()
                                }
                            })
                            .unwrap_or_default();

                        let short_preview: String = preview_owned.chars().take(28).collect();
                        let preview_text = if preview_owned.chars().count() > 28 {
                            format!("{}...", short_preview)
                        } else {
                            short_preview
                        };
                        painter.text(Pos2::new(text_x, rect.min.y + 36.0), egui::Align2::LEFT_TOP, preview_text, FontId::proportional(13.0), sub_color);

                        if chat.unread > 0 {
                            let badge_center = Pos2::new(rect.max.x - 22.0, rect.min.y + 43.0);
                            painter.circle_filled(badge_center, 10.5, if is_selected { Color32::WHITE } else { pal.accent_blue });
                            painter.text(
                                badge_center,
                                egui::Align2::CENTER_CENTER,
                                chat.unread.to_string(),
                                semibold_font(11.0),
                                if is_selected { pal.bg_selected } else { Color32::WHITE },
                            );
                        }

                        if response.clicked() {
                            chat.unread = 0;
                            clicked_chat = Some(idx);
                        }
                    }

                    if !self.search_query.trim().is_empty() && !self.group_search_results.is_empty() {
                        ui.add_space(8.0);
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(14.0, 4.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Группы по коду / названию").font(semibold_font(12.0)).color(pal.accent_light));
                            });

                        for (g_i, grp) in self.group_search_results.iter().enumerate() {
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), egui::Sense::click());
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(rect, Rounding::ZERO, pal.bg_hover);
                            }
                            let av_rect = Rect::from_center_size(Pos2::new(rect.min.x + 34.0, rect.center().y), Vec2::splat(40.0));
                            let g_tex = grp
                                .avatar_file_id
                                .as_ref()
                                .and_then(|fid| self.avatar_tex_cache.get(fid).cloned());
                            draw_avatar_circle(p, av_rect, &g_tex, &grp.avatar_tag, GROUP_COLORS[(g_i + 1) % GROUP_COLORS.len()], 13.5);
                            let text_x = rect.min.x + 66.0;
                            let code_lbl = if grp.invite_code.is_empty() {
                                format!("EGR-{:06}", grp.id.abs() % 1_000_000)
                            } else {
                                grp.invite_code.clone()
                            };
                            p.text(Pos2::new(text_x, rect.min.y + 11.0), egui::Align2::LEFT_TOP, &grp.title, semibold_font(14.0), pal.text_main);
                            p.text(Pos2::new(text_x, rect.min.y + 32.0), egui::Align2::LEFT_TOP, format!("{} • {}", code_lbl, grp.subtitle), FontId::proportional(12.0), pal.text_muted);
                            if resp.clicked() {
                                join_found_group = Some(grp.clone());
                            }
                        }
                    }

                    if !self.search_query.trim().is_empty() && !self.user_search_results.is_empty() {
                        ui.add_space(8.0);
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(14.0, 4.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Люди в Egramm").font(semibold_font(12.0)).color(pal.accent_light));
                            });

                        for (u_idx, user) in self.user_search_results.iter().enumerate() {
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), egui::Sense::click());
                            let p = ui.painter();
                            if resp.hovered() {
                                p.rect_filled(rect, Rounding::ZERO, pal.bg_hover);
                            }
                            let av_rect = Rect::from_center_size(Pos2::new(rect.min.x + 34.0, rect.center().y), Vec2::splat(40.0));
                            let init: String = user.name.chars().take(2).collect::<String>().to_uppercase();
                            let u_tex = user
                                .avatar_file_id
                                .as_ref()
                                .and_then(|fid| self.avatar_tex_cache.get(fid).cloned());
                            draw_avatar_circle(p, av_rect, &u_tex, &init, GROUP_COLORS[(u_idx + 2) % GROUP_COLORS.len()], 13.5);
                            let text_x = rect.min.x + 66.0;
                            p.text(Pos2::new(text_x, rect.min.y + 11.0), egui::Align2::LEFT_TOP, &user.name, semibold_font(14.0), pal.text_main);
                            let st = if user.online { "в сети" } else { "не в сети" };
                            p.text(Pos2::new(text_x, rect.min.y + 32.0), egui::Align2::LEFT_TOP, format!("{} • {}", user.tag, st), FontId::proportional(12.0), pal.text_muted);
                            if resp.clicked() {
                                open_dm_email = Some(user.email.clone());
                            }
                        }
                    }

                    if let Some(idx) = clicked_chat {
                        if self.selected_chat != Some(idx) {
                            self.selected_chat = Some(idx);
                            self.chat_switch_time = Instant::now();
                            self.attach_menu_open = false;
                            self.msg_context_menu = None;
                        }
                        self.send_mark_read(idx);
                    }
                });
            });

        if let Some(target_email) = open_dm_email {
            self.open_direct_chat(ctx, target_email);
        }
        if let Some(grp_dto) = join_found_group {
            self.join_group_on_server(ctx, grp_dto);
        }

        let mut opened_ctx_menu: Option<(usize, usize, Pos2)> = None;
        let mut opened_lightbox: Option<PathBuf> = None;
        let mut open_group_manage: Option<usize> = None;
        let mut open_peer_profile_for: Option<usize> = None;
        let mut drive_open_request: Option<(String, String)> = None;
        let mut send_now = false;
        let mut toggle_call_for_chat: Option<usize> = None;
        let mut accept_call_now = false;
        let mut hangup_call_now = false;
        let mut expand_call_window_now = false;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.bg_chat))
            .show(ctx, |ui| {
                if self.call_status != CallStatus::Idle && !self.call_window_expanded {
                    let banner_col = match self.call_status {
                        CallStatus::Connected { .. } | CallStatus::Incoming { .. } => Color32::from_rgb(35, 134, 78),
                        _ => Color32::from_rgb(34, 96, 174),
                    };
                    egui::TopBottomPanel::top("tg_minimized_call_bar")
                        .frame(egui::Frame::none().fill(banner_col).inner_margin(egui::Margin::symmetric(14.0, 6.0)))
                        .show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                let title = match &self.call_status {
                                    CallStatus::Outgoing { peer_name, .. } => format!("Вызов: {}...  (нажмите, чтобы открыть)", peer_name),
                                    CallStatus::Incoming { caller_name, .. } => format!("Входящий звонок: {}", caller_name),
                                    CallStatus::Connected { peer_name, started_at, participants, .. } => {
                                        let s = started_at.elapsed().as_secs();
                                        format!(
                                            "В звонке: {} ({}/{})  •  {:02}:{:02}  (открыть)",
                                            peer_name,
                                            participants.len().max(2),
                                            MAX_CALL_PARTICIPANTS,
                                            s / 60,
                                            s % 60
                                        )
                                    }
                                    CallStatus::Idle => String::new(),
                                };
                                if ui
                                    .add(
                                        egui::Button::new(egui::RichText::new(title).font(semibold_font(12.8)).color(Color32::WHITE))
                                            .fill(Color32::TRANSPARENT),
                                    )
                                    .clicked()
                                {
                                    expand_call_window_now = true;
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui
                                        .add_sized(
                                            Vec2::new(86.0, 24.0),
                                            egui::Button::new(egui::RichText::new("Сбросить").size(12.0).color(Color32::WHITE))
                                                .fill(DANGER_RED)
                                                .rounding(12.0),
                                        )
                                        .clicked()
                                    {
                                        hangup_call_now = true;
                                    }
                                    if matches!(self.call_status, CallStatus::Incoming { .. }) {
                                        if ui
                                            .add_sized(
                                                Vec2::new(82.0, 24.0),
                                                egui::Button::new(egui::RichText::new("Принять").size(12.0).color(Color32::WHITE))
                                                    .fill(Color32::from_rgb(24, 102, 58))
                                                    .rounding(12.0),
                                            )
                                            .clicked()
                                        {
                                            accept_call_now = true;
                                        }
                                    } else {
                                        let muted = self.audio_bridge.mic_muted.load(Ordering::Relaxed);
                                        if ui
                                            .add_sized(
                                                Vec2::new(88.0, 24.0),
                                                egui::Button::new(
                                                    egui::RichText::new(if muted { "Мик. выкл" } else { "Мик. вкл" })
                                                        .size(12.0)
                                                        .color(Color32::WHITE),
                                                )
                                                    .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 55))
                                                    .rounding(12.0),
                                            )
                                            .clicked()
                                        {
                                            self.audio_bridge.mic_muted.store(!muted, Ordering::Relaxed);
                                        }
                                    }
                                });
                            });
                        });
                }

                if let Some(chat_idx) = self.selected_chat {
                    egui::TopBottomPanel::top("tg_chat_header")
                        .frame(
                            egui::Frame::none()
                                .fill(pal.bg_panel)
                                .stroke(Stroke::new(1.0, pal.border_line))
                                .inner_margin(egui::Margin::symmetric(16.0, 9.0)),
                        )
                        .show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(chat) = self.chats.get(chat_idx) {
                                    let (av_rect, av_resp) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
                                    draw_avatar_circle(ui.painter(), av_rect, &chat.avatar_tex, &chat.avatar_tag, chat.avatar_color, 12.5);
                                    if av_resp.clicked() {
                                        if chat.is_group {
                                            open_group_manage = Some(chat_idx);
                                        } else if chat.server_chat_id.is_some() {
                                            open_peer_profile_for = Some(chat_idx);
                                        }
                                    }

                                    ui.add_space(6.0);

                                    let header_info = ui
                                        .vertical(|ui| {
                                            ui.label(egui::RichText::new(&chat.title).font(semibold_font(15.5)).color(pal.text_main));
                                            if chat.server_chat_id.is_some() {
                                                let (sub_text, sub_col) = if !self.ws_connected {
                                                    ("подключение...".into(), pal.text_muted)
                                                } else if chat.is_group {
                                                    (format!("{} • {}", chat.invite_code, chat.subtitle), pal.text_muted)
                                                } else {
                                                    let is_on = chat
                                                        .peer_email
                                                        .as_ref()
                                                        .map(|em| self.online_emails.contains(em))
                                                        .unwrap_or(chat.peer_online);
                                                    if is_on {
                                                        ("в сети".into(), ONLINE_GREEN)
                                                    } else {
                                                        ("не в сети".into(), pal.text_muted)
                                                    }
                                                };
                                                ui.label(egui::RichText::new(sub_text).size(12.2).color(sub_col));
                                            }
                                        })
                                        .response
                                        .interact(egui::Sense::click());

                                    let tip = if chat.is_group {
                                        "Нажмите, чтобы управлять группой"
                                    } else {
                                        "Нажмите, чтобы открыть профиль"
                                    };
                                    if header_info.on_hover_text(tip).clicked() {
                                        if chat.is_group {
                                            open_group_manage = Some(chat_idx);
                                        } else if chat.server_chat_id.is_some() {
                                            open_peer_profile_for = Some(chat_idx);
                                        }
                                    }

                                    let is_group = chat.is_group;
                                    let is_saved = chat.server_chat_id.is_none();
                                    let in_call = self.call_status != CallStatus::Idle;

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if is_group {
                                            let (s_rect, s_resp) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::click());
                                            if s_resp.hovered() {
                                                ui.painter().circle_filled(s_rect.center(), 17.0, pal.bg_hover);
                                            }
                                            draw_sliders_icon(ui.painter(), s_rect.center(), if s_resp.hovered() { pal.accent_light } else { pal.text_muted });
                                            if s_resp.on_hover_text("Управление группой").clicked() {
                                                open_group_manage = Some(chat_idx);
                                            }
                                        }

                                        if !is_saved {
                                            let (c_rect, c_resp) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::click());
                                            if in_call {
                                                ui.painter().circle_filled(c_rect.center(), 17.0, Color32::from_rgba_unmultiplied(68, 198, 122, 48));
                                            } else if c_resp.hovered() {
                                                ui.painter().circle_filled(c_rect.center(), 17.0, pal.bg_hover);
                                            }
                                            draw_phone_icon(
                                                ui.painter(),
                                                c_rect.center(),
                                                if in_call { ONLINE_GREEN } else if c_resp.hovered() { pal.accent_light } else { pal.text_muted },
                                            );
                                            let c_tip = if in_call {
                                                "Открыть окно звонка"
                                            } else if is_group {
                                                "Групповой звонок (до 4 участн.)"
                                            } else {
                                                "Позвонить"
                                            };
                                            if c_resp.on_hover_text(c_tip).clicked() {
                                                toggle_call_for_chat = Some(chat_idx);
                                            }
                                        }
                                    });
                                }
                            });
                        });

                    egui::TopBottomPanel::bottom("tg_chat_input")
                        .frame(
                            egui::Frame::none()
                                .fill(pal.bg_panel)
                                .stroke(Stroke::new(1.0, pal.border_line))
                                .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
                        )
                        .show_inside(ui, |ui| {
                            if let Some(att) = self.pending_attachment.clone() {
                                let mut remove_att = false;
                                egui::Frame::none()
                                    .fill(pal.bg_input)
                                    .rounding(Rounding::same(10.0))
                                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            egui::Frame::none()
                                                .fill(att.kind.badge_color())
                                                .rounding(Rounding::same(6.0))
                                                .inner_margin(egui::Margin::symmetric(7.0, 3.0))
                                                .show(ui, |ui| {
                                                    ui.label(egui::RichText::new(att.kind.tag()).font(semibold_font(10.5)).color(Color32::WHITE));
                                                });
                                            ui.vertical(|ui| {
                                                ui.label(egui::RichText::new(&att.title).font(semibold_font(13.0)).color(pal.text_main));
                                                ui.label(egui::RichText::new(&att.subtitle).size(11.5).color(pal.text_muted));
                                            });
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui
                                                    .add(egui::Button::new(egui::RichText::new("✕").color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(12.0))
                                                    .clicked()
                                                {
                                                    remove_att = true;
                                                }
                                            });
                                        });
                                    });
                                if remove_att {
                                    self.pending_attachment = None;
                                }
                                ui.add_space(6.0);
                            }

                            ui.horizontal(|ui| {
                                let clip_resp = draw_paperclip_button(ui, self.attach_menu_open, &pal);
                                self.attach_btn_rect = clip_resp.rect;
                                if clip_resp.on_hover_text("Прикрепить файл").clicked() {
                                    self.attach_menu_open = !self.attach_menu_open;
                                    self.msg_context_menu = None;
                                }

                                let send_w = 36.0;
                                let margin_x = 12.0;
                                let gap = ui.spacing().item_spacing.x;
                                let input_width = (ui.available_width() - send_w - gap - margin_x * 2.0 - 4.0).max(80.0);

                                let text_resp = ui.add(
                                    egui::TextEdit::singleline(&mut self.input_text)
                                        .hint_text(if self.pending_attachment.is_some() {
                                            "Добавить подпись..."
                                        } else {
                                            "Написать сообщение..."
                                        })
                                        .font(FontId::proportional(14.5))
                                        .desired_width(input_width)
                                        .margin(Vec2::new(margin_x, 8.0)),
                                );

                                if text_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    send_now = true;
                                    text_resp.request_focus();
                                }

                                let is_ready = !self.input_text.trim().is_empty() || self.pending_attachment.is_some();
                                if draw_send_button(ui, is_ready, &pal).clicked() {
                                    send_now = true;
                                }
                            });
                        });

                    let right_click_pos = if self.lightbox_image.is_none()
                        && self.intro_start.is_none()
                        && self.active_modal == ActiveModal::None
                        && !self.menu_open
                        && ctx.input(|i| i.pointer.secondary_clicked())
                    {
                        ctx.input(|i| i.pointer.interact_pos())
                    } else {
                        None
                    };

                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().fill(pal.bg_chat).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
                        .show_inside(ui, |ui| {
                            let ease_t = ease_out_cubic(switch_elapsed / 0.18);
                            let chat_alpha = ease_t;
                            ui.add_space((1.0 - ease_t) * 10.0);

                            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                                if let Some(chat) = self.chats.get(chat_idx) {
                                    for (msg_idx, msg) in chat.messages.iter().enumerate() {
                                        let msg_ease = ease_out_cubic(msg.created_at.elapsed().as_secs_f32() / 0.18);
                                        let alpha = chat_alpha * msg_ease;

                                        let align = if msg.is_me {
                                            egui::Layout::right_to_left(egui::Align::TOP)
                                        } else {
                                            egui::Layout::left_to_right(egui::Align::TOP)
                                        };

                                        let mut bubble_rect = Rect::NOTHING;

                                        ui.with_layout(align, |ui| {
                                            let bg = with_alpha(if msg.is_me { pal.bubble_out } else { pal.bubble_in }, alpha);
                                            let bubble = egui::Frame::none()
                                                .fill(bg)
                                                .rounding(Rounding::same(14.0))
                                                .inner_margin(egui::Margin::symmetric(13.0, 8.0))
                                                .show(ui, |ui| {
                                                    ui.set_max_width(430.0);
                                                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                                        if !msg.is_me {
                                                            let sender_lbl = ui.add(
                                                                egui::Label::new(
                                                                    egui::RichText::new(&msg.sender)
                                                                        .font(semibold_font(13.0))
                                                                        .color(with_alpha(pal.accent_light, alpha)),
                                                                )
                                                                    .sense(egui::Sense::click()),
                                                            );
                                                            if sender_lbl.on_hover_text("Открыть профиль").clicked() && !chat.is_group {
                                                                open_peer_profile_for = Some(chat_idx);
                                                            }
                                                        }

                                                        if let Some(att) = &msg.attachment {
                                                            let img_tex = att
                                                                .local_path
                                                                .as_ref()
                                                                .filter(|p| p.to_str().map(is_image_name).unwrap_or(false))
                                                                .and_then(|p| get_or_load_image(ctx, &mut self.image_cache, p));

                                                            if let Some(tex) = img_tex {
                                                                let orig = tex.size_vec2();
                                                                let scale = (340.0_f32 / orig.x).min(280.0_f32 / orig.y).min(1.0);
                                                                let disp = orig * scale;
                                                                let img_resp = ui.add(
                                                                    egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), disp))
                                                                        .rounding(Rounding::same(10.0))
                                                                        .tint(with_alpha(Color32::WHITE, alpha))
                                                                        .sense(egui::Sense::click()),
                                                                );
                                                                if img_resp.on_hover_text("Нажмите, чтобы приблизить фото").clicked() {
                                                                    if let Some(p) = &att.local_path {
                                                                        opened_lightbox = Some(p.clone());
                                                                    }
                                                                }
                                                            } else {
                                                                let card_resp = egui::Frame::none()
                                                                    .fill(with_alpha(pal.card_inner, alpha))
                                                                    .rounding(Rounding::same(10.0))
                                                                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                                                                    .show(ui, |ui| {
                                                                        ui.horizontal(|ui| {
                                                                            let (ic_rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
                                                                            ui.painter().circle_filled(ic_rect.center(), 17.0, with_alpha(att.kind.badge_color(), alpha));
                                                                            draw_menu_icon(ui.painter(), ic_rect.center(), att.kind, with_alpha(Color32::WHITE, alpha));
                                                                            ui.add_space(4.0);
                                                                            ui.vertical(|ui| {
                                                                                ui.label(egui::RichText::new(&att.title).font(semibold_font(13.5)).color(with_alpha(pal.text_main, alpha)));
                                                                                ui.label(egui::RichText::new(&att.subtitle).size(11.5).color(with_alpha(pal.accent_light, alpha)));
                                                                            });
                                                                        });
                                                                    })
                                                                    .response
                                                                    .interact(egui::Sense::click());

                                                                if card_resp.clicked() {
                                                                    if let Some(p) = &att.local_path {
                                                                        let _ = open::that(p);
                                                                    } else if let Some(id) = &att.drive_file_id {
                                                                        drive_open_request = Some((id.clone(), att.title.clone()));
                                                                    }
                                                                }
                                                            }

                                                            if !msg.text.is_empty() {
                                                                ui.add_space(6.0);
                                                            }
                                                        }

                                                        if !msg.text.is_empty() {
                                                            ui.label(egui::RichText::new(&msg.text).font(FontId::proportional(14.5)).color(with_alpha(pal.text_main, alpha)));
                                                        }

                                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                                            ui.spacing_mut().item_spacing.x = 4.0;
                                                            if msg.is_me {
                                                                let (tick_rect, _) = ui.allocate_exact_size(Vec2::new(18.0, 14.0), egui::Sense::hover());
                                                                draw_read_ticks(ui.painter(), tick_rect, msg.is_read, pal.text_muted, alpha);
                                                            }
                                                            ui.label(egui::RichText::new(&msg.time).size(11.0).color(with_alpha(pal.text_muted, alpha)));
                                                        });
                                                    });
                                                });
                                            bubble_rect = bubble.response.rect;
                                        });

                                        if let Some(pos) = right_click_pos {
                                            if bubble_rect.contains(pos) {
                                                opened_ctx_menu = Some((chat_idx, msg_idx, pos));
                                            }
                                        }
                                        ui.add_space(6.0);
                                    }
                                }
                            });
                        });
                } else {
                    ui.centered_and_justified(|ui| {
                        egui::Frame::none()
                            .fill(pal.bg_panel)
                            .stroke(Stroke::new(1.0, pal.border_line))
                            .rounding(Rounding::same(16.0))
                            .inner_margin(egui::Margin::symmetric(16.0, 7.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Выберите чат, чтобы начать общение").size(13.5).color(pal.text_main));
                            });
                    });
                }
            });

        if expand_call_window_now {
            self.call_window_expanded = true;
        }
        if let Some(c_idx) = toggle_call_for_chat {
            self.start_call_in_chat(c_idx);
        }
        if accept_call_now {
            self.accept_incoming_call();
        }
        if hangup_call_now {
            self.end_active_call(true);
        }
        if send_now {
            self.send_message(ctx);
        }
        if let Some((drive_id, title)) = drive_open_request {
            self.show_toast(format!("Скачивание «{}»...", title));
            self.trigger_drive_download(ctx, drive_id, title, true);
        }
        if let Some(p_idx) = open_peer_profile_for {
            self.open_chat_peer_profile(p_idx);
            self.attach_menu_open = false;
            self.msg_context_menu = None;
        }
        if let Some(g_idx) = open_group_manage {
            self.active_modal = ActiveModal::ManageGroup(g_idx);
            self.group_tab = GroupTab::Info;
            self.editing_admin_idx = None;
            self.attach_menu_open = false;
            self.msg_context_menu = None;
        }
        if let Some(img_path) = opened_lightbox {
            self.lightbox_image = Some(img_path);
            self.lightbox_zoom = 1.0;
            self.lightbox_offset = Vec2::ZERO;
            self.lightbox_open_time = Instant::now();
            self.msg_context_menu = None;
            self.attach_menu_open = false;
        }
        if let Some(new_menu) = opened_ctx_menu {
            self.msg_context_menu = Some(new_menu);
            self.attach_menu_open = false;
        }

        if let Some((c_idx, m_idx, raw_pos)) = self.msg_context_menu {
            let menu_w = 216.0;
            let row_h = 36.0;
            let menu_h = row_h * 4.0 + 14.0;
            let screen = ctx.screen_rect();
            let menu_pos = Pos2::new(
                raw_pos.x.min(screen.max.x - menu_w - 8.0).max(8.0),
                raw_pos.y.min(screen.max.y - menu_h - 8.0).max(8.0),
            );
            let menu_rect = Rect::from_min_size(menu_pos, Vec2::new(menu_w, menu_h));

            if opened_ctx_menu.is_none()
                && ctx.input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked())
            {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !menu_rect.contains(pos) {
                        self.msg_context_menu = None;
                    }
                }
            }

            let mut action = 0_u8;
            egui::Area::new(egui::Id::new("tg_msg_context_menu"))
                .fixed_pos(menu_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(pal.bg_popup)
                        .stroke(Stroke::new(1.0, pal.popup_border))
                        .rounding(Rounding::same(10.0))
                        .inner_margin(egui::Margin::symmetric(0.0, 6.0))
                        .show(ui, |ui| {
                            ui.set_width(menu_w);

                            let (copy_rect, copy_resp) = ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                            let copy_bg = if copy_resp.hovered() { pal.bg_hover } else { pal.bg_popup };
                            if copy_resp.hovered() { ui.painter().rect_filled(copy_rect, Rounding::ZERO, pal.bg_hover); }
                            draw_copy_icon(ui.painter(), Pos2::new(copy_rect.min.x + 24.0, copy_rect.center().y), pal.text_main, copy_bg);
                            ui.painter().text(Pos2::new(copy_rect.min.x + 48.0, copy_rect.center().y), egui::Align2::LEFT_CENTER, "Копировать", FontId::proportional(14.0), pal.text_main);
                            if copy_resp.clicked() { action = 1; }

                            let (fwd_rect, fwd_resp) = ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                            if fwd_resp.hovered() { ui.painter().rect_filled(fwd_rect, Rounding::ZERO, pal.bg_hover); }
                            draw_forward_icon(ui.painter(), Pos2::new(fwd_rect.min.x + 24.0, fwd_rect.center().y), pal.accent_light);
                            ui.painter().text(Pos2::new(fwd_rect.min.x + 48.0, fwd_rect.center().y), egui::Align2::LEFT_CENTER, "Переслать", FontId::proportional(14.0), pal.text_main);
                            if fwd_resp.clicked() { action = 4; }

                            let (del_me_rect, del_me_resp) = ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                            if del_me_resp.hovered() { ui.painter().rect_filled(del_me_rect, Rounding::ZERO, pal.bg_hover); }
                            draw_trash_icon(ui.painter(), Pos2::new(del_me_rect.min.x + 24.0, del_me_rect.center().y), pal.text_main);
                            ui.painter().text(Pos2::new(del_me_rect.min.x + 48.0, del_me_rect.center().y), egui::Align2::LEFT_CENTER, "Удалить у меня", FontId::proportional(14.0), pal.text_main);
                            if del_me_resp.clicked() { action = 2; }

                            let (del_all_rect, del_all_resp) = ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                            if del_all_resp.hovered() { ui.painter().rect_filled(del_all_rect, Rounding::ZERO, pal.bg_hover); }
                            draw_trash_icon(ui.painter(), Pos2::new(del_all_rect.min.x + 24.0, del_all_rect.center().y), DANGER_RED);
                            ui.painter().text(Pos2::new(del_all_rect.min.x + 48.0, del_all_rect.center().y), egui::Align2::LEFT_CENTER, "Удалить у всех", FontId::proportional(14.0), DANGER_RED);
                            if del_all_resp.clicked() { action = 3; }
                        });
                });

            match action {
                1 => {
                    if let Some(chat) = self.chats.get(c_idx) {
                        if let Some(msg) = chat.messages.get(m_idx) {
                            let to_copy = if !msg.text.is_empty() {
                                msg.text.clone()
                            } else if let Some(att) = &msg.attachment {
                                att.title.clone()
                            } else {
                                String::new()
                            };
                            if !to_copy.is_empty() {
                                ctx.copy_text(to_copy);
                                self.show_toast("Скопировано в буфер");
                            }
                        }
                    }
                    self.msg_context_menu = None;
                }
                2 => {
                    if let Some(chat) = self.chats.get_mut(c_idx) {
                        if m_idx < chat.messages.len() {
                            chat.messages.remove(m_idx);
                        }
                    }
                    self.show_toast("Сообщение удалено у вас");
                    self.msg_context_menu = None;
                }
                3 => {
                    if let Some(chat) = self.chats.get_mut(c_idx) {
                        if m_idx < chat.messages.len() {
                            let removed = chat.messages.remove(m_idx);
                            if let (Some(sc_id), Some(msg_id), Some(token), Some(ws_tx)) = (
                                chat.server_chat_id,
                                removed.server_id,
                                &self.session_token,
                                &self.ws_out_tx,
                            ) {
                                let wire = WsWireEvent::DeleteMessage {
                                    session_token: token.clone(),
                                    chat_id: sc_id,
                                    message_id: msg_id,
                                    for_everyone: true,
                                };
                                if let Ok(json) = serde_json::to_string(&wire) {
                                    let _ = ws_tx.send(json);
                                }
                            }
                        }
                    }
                    self.show_toast("Сообщение удалено у всех");
                    self.msg_context_menu = None;
                }
                4 => {
                    self.msg_context_menu = None;
                    self.active_modal = ActiveModal::ForwardMessage { chat_idx: c_idx, msg_idx: m_idx };
                }
                _ => {}
            }
        }

        if self.attach_menu_open && self.selected_chat.is_some() {
            let menu_w = 210.0;
            let row_h = 38.0;
            let items: [(AttachmentKind, &str); 6] = [
                (AttachmentKind::Media, "Фото или видео"),
                (AttachmentKind::Document, "Документ"),
                (AttachmentKind::Poll, "Опрос"),
                (AttachmentKind::Article, "Статья"),
                (AttachmentKind::Location, "Геопозиция"),
                (AttachmentKind::Music, "Музыка"),
            ];
            let menu_h = items.len() as f32 * row_h + 16.0;
            let menu_pos = Pos2::new(self.attach_btn_rect.min.x, (self.attach_btn_rect.min.y - menu_h - 10.0).max(10.0));
            let menu_rect = Rect::from_min_size(menu_pos, Vec2::new(menu_w, menu_h));

            if ctx.input(|i| i.pointer.any_click()) {
                if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !menu_rect.contains(pos) && !self.attach_btn_rect.contains(pos) {
                        self.attach_menu_open = false;
                    }
                }
            }

            let mut chosen_kind: Option<AttachmentKind> = None;
            egui::Area::new(egui::Id::new("tg_attach_popup"))
                .fixed_pos(menu_pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(pal.bg_popup)
                        .stroke(Stroke::new(1.0, pal.popup_border))
                        .rounding(Rounding::same(12.0))
                        .inner_margin(egui::Margin::symmetric(0.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(menu_w);
                            for (kind, label) in items {
                                let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                let p = ui.painter();
                                if resp.hovered() { p.rect_filled(row_rect, Rounding::ZERO, pal.bg_hover); }
                                draw_menu_icon(p, Pos2::new(row_rect.min.x + 26.0, row_rect.center().y), kind, pal.text_main);
                                p.text(Pos2::new(row_rect.min.x + 52.0, row_rect.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(14.0), pal.text_main);
                                if resp.clicked() { chosen_kind = Some(kind); }
                            }
                        });
                });

            if let Some(kind) = chosen_kind {
                self.pick_attachment(kind);
            }
        }

        let mut drawer_toggle_theme = false;
        if self.menu_open {
            let screen_rect = ctx.screen_rect();
            egui::Area::new(egui::Id::new("tg_menu_backdrop"))
                .fixed_pos(Pos2::new(280.0, 0.0))
                .order(egui::Order::Middle)
                .show(ctx, |ui| {
                    let dim_size = Vec2::new((screen_rect.width() - 280.0).max(1.0), screen_rect.height());
                    let (rect, resp) = ui.allocate_exact_size(dim_size, egui::Sense::click());
                    ui.painter().rect_filled(rect, Rounding::ZERO, Color32::from_rgba_unmultiplied(0, 0, 0, 125));
                    if resp.clicked() { self.menu_open = false; }
                });

            egui::Window::new("tg_drawer_menu")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_pos(Pos2::ZERO)
                .fixed_size(Vec2::new(280.0, screen_rect.height()))
                .frame(egui::Frame::none().fill(pal.bg_panel))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(pal.drawer_header)
                        .inner_margin(egui::Margin::symmetric(18.0, 18.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (av_rect, av_resp) = ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::click());
                                let initials: String = self.username.chars().take(2).collect::<String>().to_uppercase();
                                draw_avatar_circle(ui.painter(), av_rect, &self.user_avatar, &initials, AUTH_BLUE, 18.0);
                                if av_resp.on_hover_text("Открыть профиль").clicked() {
                                    self.menu_open = false;
                                    self.profile_editing = false;
                                    self.active_modal = ActiveModal::Profile;
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                    if ui.add(egui::Button::new(egui::RichText::new("✕").color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                        self.menu_open = false;
                                    }
                                });
                            });
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new(&self.username).font(semibold_font(16.0)).color(pal.text_main));
                            if !self.user_email.is_empty() {
                                ui.label(egui::RichText::new(&self.user_email).size(12.0).color(pal.text_muted));
                            }
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(&self.user_tag).size(12.5).color(pal.accent_light));
                        });

                    ui.add_space(8.0);
                    egui::Frame::none().inner_margin(egui::Margin::symmetric(10.0, 4.0)).show(ui, |ui| {
                        if draw_drawer_row(ui, "Мой профиль", pal.text_main, &pal, draw_person_icon).clicked() {
                            self.menu_open = false;
                            self.profile_editing = false;
                            self.active_modal = ActiveModal::Profile;
                        }
                        ui.add_space(2.0);
                        if draw_drawer_row(ui, "Создать группу", pal.text_main, &pal, draw_group_icon).clicked() {
                            self.menu_open = false;
                            self.new_group_name.clear();
                            self.new_group_desc.clear();
                            self.new_group_link.clear();
                            self.new_group_public = false;
                            self.new_group_avatar = None;
                            self.new_group_avatar_path = None;
                            self.active_modal = ActiveModal::CreateGroup;
                        }
                        ui.add_space(6.0);
                        ui.painter().line_segment(
                            [Pos2::new(ui.min_rect().min.x + 8.0, ui.cursor().min.y), Pos2::new(ui.min_rect().min.x + ui.available_width() - 8.0, ui.cursor().min.y)],
                            Stroke::new(1.0, pal.border_line),
                        );
                        ui.add_space(6.0);

                        egui::Frame::none().inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Тема").size(14.0).color(pal.text_main));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.allocate_ui_with_layout(Vec2::new(96.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        if draw_theme_slider_row(ui, self.is_dark_mode, self.theme_anim, &pal, pal.bg_panel) {
                                            drawer_toggle_theme = true;
                                        }
                                    });
                                });
                            });
                        });

                        ui.add_space(6.0);
                        ui.painter().line_segment(
                            [Pos2::new(ui.min_rect().min.x + 8.0, ui.cursor().min.y), Pos2::new(ui.min_rect().min.x + ui.available_width() - 8.0, ui.cursor().min.y)],
                            Stroke::new(1.0, pal.border_line),
                        );
                        ui.add_space(6.0);

                        if draw_drawer_row(ui, "Выйти из аккаунта", DANGER_RED, &pal, draw_logout_icon).clicked() {
                            self.end_active_call(true);
                            self.ws_loop_running.store(false, Ordering::SeqCst);
                            self.audio_bridge.set_my_email(String::new());
                            self.menu_open = false;
                            self.is_logged_in = false;
                            self.session_token = None;
                            self.ws_out_tx = None;
                            self.ws_connected = false;
                            self.online_emails.clear();
                            self.chats.retain(|c| c.server_chat_id.is_none());
                            self.selected_chat = Some(0);
                            self.auth_anim_start = Instant::now();
                        }
                    });

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        egui::Frame::none().inner_margin(egui::Margin::symmetric(18.0, 14.0)).show(ui, |ui| {
                            ui.label(egui::RichText::new("Egramm Beta").size(11.5).color(pal.text_muted));
                        });
                    });
                });
        }

        if drawer_toggle_theme {
            self.is_dark_mode = !self.is_dark_mode;
            let new_pal = self.palette();
            apply_theme_style(ctx, &new_pal, self.is_dark_mode);
        }

        let mut pick_user_avatar = false;
        let mut clear_user_avatar = false;
        let mut pick_new_group_avatar = false;
        let mut pick_existing_group_avatar: Option<usize> = None;
        let mut delete_group_idx: Option<usize> = None;
        let mut toast_msg: Option<String> = None;
        let mut send_poll_now = false;
        let mut save_profile_now = false;
        let mut create_group_req: Option<(String, String, String, Color32, Option<egui::TextureHandle>, Option<PathBuf>, bool, String)> = None;
        let mut forward_req: Option<(usize, usize, usize)> = None;
        let mut invite_member_req: Option<(usize, String)> = None;
        let mut open_member_prof_req: Option<(usize, usize)> = None;
        let mut dm_from_profile_email: Option<String> = None;

        match self.active_modal {
            ActiveModal::UserProfile => {
                if let Some(vp) = self.viewed_profile.clone() {
                    egui::Window::new("Профиль пользователя")
                        .title_bar(false)
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                        .fixed_size(Vec2::new(380.0, 420.0))
                        .frame(egui::Frame::none().fill(pal.bg_panel).stroke(Stroke::new(1.0, pal.popup_border)).rounding(Rounding::same(16.0)))
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .fill(pal.drawer_header)
                                .rounding(Rounding { nw: 16.0, ne: 16.0, sw: 0.0, se: 0.0 })
                                .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Профиль пользователя").font(semibold_font(16.5)).color(pal.text_main));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.add(egui::Button::new(egui::RichText::new("✕").size(15.0).color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                                self.active_modal = ActiveModal::None;
                                            }
                                        });
                                    });
                                    ui.add_space(12.0);
                                    ui.horizontal(|ui| {
                                        let (av_rect, _) = ui.allocate_exact_size(Vec2::splat(74.0), egui::Sense::hover());
                                        let initials: String = vp.name.chars().take(2).collect::<String>().to_uppercase();
                                        draw_avatar_circle(ui.painter(), av_rect, &vp.avatar_tex, &initials, vp.avatar_color, 25.0);
                                        ui.add_space(12.0);
                                        ui.vertical(|ui| {
                                            ui.add_space(8.0);
                                            ui.label(egui::RichText::new(&vp.name).font(semibold_font(19.0)).color(pal.text_main));
                                            ui.label(
                                                egui::RichText::new(if vp.online { "в сети" } else { "не в сети" })
                                                    .size(12.5)
                                                    .color(if vp.online { ONLINE_GREEN } else { pal.text_muted }),
                                            );
                                        });
                                    });
                                });

                            egui::Frame::none().inner_margin(egui::Margin::symmetric(18.0, 14.0)).show(ui, |ui| {
                                let bio_display = if vp.bio.trim().is_empty() {
                                    "Описание не указано"
                                } else {
                                    vp.bio.as_str()
                                };
                                for (k, v) in [
                                    ("Имя пользователя", vp.tag.as_str()),
                                    ("О себе", bio_display),
                                    ("Электронная почта", vp.email.as_str()),
                                ] {
                                    ui.label(egui::RichText::new(k).size(11.5).color(pal.text_muted));
                                    ui.label(egui::RichText::new(v).size(14.0).color(pal.text_main));
                                    ui.add_space(10.0);
                                }

                                ui.add_space(14.0);
                                if !vp.is_me && vp.email.contains('@') {
                                    if ui
                                        .add_sized(
                                            Vec2::new(ui.available_width(), 36.0),
                                            egui::Button::new(egui::RichText::new("Написать сообщение").font(semibold_font(13.5)).color(Color32::WHITE))
                                                .fill(pal.accent_blue)
                                                .rounding(18.0),
                                        )
                                        .clicked()
                                    {
                                        dm_from_profile_email = Some(vp.email.clone());
                                        self.active_modal = ActiveModal::None;
                                    }
                                }
                            });
                        });
                } else {
                    self.active_modal = ActiveModal::None;
                }
            }

            ActiveModal::ForwardMessage { chat_idx, msg_idx } => {
                egui::Window::new("Переслать сообщение")
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .fixed_size(Vec2::new(360.0, 390.0))
                    .frame(egui::Frame::none().fill(pal.bg_panel).stroke(Stroke::new(1.0, pal.popup_border)).rounding(Rounding::same(16.0)))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none().inner_margin(egui::Margin::same(16.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Переслать в чат").font(semibold_font(16.5)).color(pal.text_main));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(egui::RichText::new("✕").color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                        self.active_modal = ActiveModal::None;
                                    }
                                });
                            });
                            ui.add_space(10.0);
                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                for (dst_idx, dst_chat) in self.chats.iter().enumerate() {
                                    let row = egui::Frame::none()
                                        .fill(pal.bg_input)
                                        .rounding(Rounding::same(10.0))
                                        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
                                                draw_avatar_circle(ui.painter(), r, &dst_chat.avatar_tex, &dst_chat.avatar_tag, dst_chat.avatar_color, 12.0);
                                                ui.label(egui::RichText::new(&dst_chat.title).font(semibold_font(13.5)).color(pal.text_main));
                                            });
                                        })
                                        .response
                                        .interact(egui::Sense::click());
                                    if row.clicked() {
                                        forward_req = Some((chat_idx, msg_idx, dst_idx));
                                        self.active_modal = ActiveModal::None;
                                    }
                                    ui.add_space(6.0);
                                }
                            });
                        });
                    });
            }

            ActiveModal::Profile => {
                egui::Window::new("Профиль")
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .fixed_size(Vec2::new(400.0, 470.0))
                    .frame(egui::Frame::none().fill(pal.bg_panel).stroke(Stroke::new(1.0, pal.popup_border)).rounding(Rounding::same(16.0)))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(pal.drawer_header)
                            .rounding(Rounding { nw: 16.0, ne: 16.0, sw: 0.0, se: 0.0 })
                            .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Мой профиль").font(semibold_font(16.5)).color(pal.text_main));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(egui::Button::new(egui::RichText::new("✕").size(15.0).color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                            self.active_modal = ActiveModal::None;
                                        }
                                    });
                                });
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    let (av_rect, av_resp) = ui.allocate_exact_size(Vec2::splat(78.0), egui::Sense::click());
                                    let initials: String = self.username.chars().take(2).collect::<String>().to_uppercase();
                                    draw_avatar_circle(ui.painter(), av_rect, &self.user_avatar, &initials, AUTH_BLUE, 26.0);
                                    ui.painter().circle_filled(Pos2::new(av_rect.max.x - 10.0, av_rect.max.y - 10.0), 8.5, pal.drawer_header);
                                    ui.painter().circle_filled(Pos2::new(av_rect.max.x - 10.0, av_rect.max.y - 10.0), 6.0, ONLINE_GREEN);
                                    if av_resp.hovered() {
                                        ui.painter().circle_filled(av_rect.center(), 39.0, Color32::from_rgba_unmultiplied(0, 0, 0, 130));
                                        ui.painter().text(av_rect.center(), egui::Align2::CENTER_CENTER, "Фото", semibold_font(12.5), Color32::WHITE);
                                    }
                                    if av_resp.clicked() { pick_user_avatar = true; }
                                    ui.add_space(10.0);
                                    ui.vertical(|ui| {
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new(&self.username).font(semibold_font(19.0)).color(pal.text_main));
                                        ui.label(egui::RichText::new("в сети").size(12.5).color(ONLINE_GREEN));
                                        ui.add_space(6.0);
                                        ui.horizontal(|ui| {
                                            if ui.add_sized(Vec2::new(118.0, 26.0), egui::Button::new(egui::RichText::new("Сменить фото").size(12.0).color(pal.accent_light)).fill(pal.bg_input).rounding(13.0)).clicked() {
                                                pick_user_avatar = true;
                                            }
                                            if self.user_avatar.is_some()
                                                && ui.add_sized(Vec2::new(72.0, 26.0), egui::Button::new(egui::RichText::new("Удалить").size(12.0).color(DANGER_RED)).fill(Color32::TRANSPARENT).rounding(13.0)).clicked()
                                            {
                                                clear_user_avatar = true;
                                            }
                                        });
                                    });
                                });
                            });

                        egui::Frame::none().inner_margin(egui::Margin::symmetric(18.0, 14.0)).show(ui, |ui| {
                            if self.profile_editing {
                                ui.label(egui::RichText::new("Отображаемое имя").size(12.0).color(pal.text_muted));
                                ui.add(egui::TextEdit::singleline(&mut self.username).desired_width(344.0).margin(Vec2::new(10.0, 7.0)));
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Имя пользователя (@tag)").size(12.0).color(pal.text_muted));
                                ui.add(egui::TextEdit::singleline(&mut self.user_tag).desired_width(344.0).margin(Vec2::new(10.0, 7.0)));
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("О себе").size(12.0).color(pal.text_muted));
                                ui.add(egui::TextEdit::singleline(&mut self.user_bio).hint_text("Расскажите о себе...").desired_width(344.0).margin(Vec2::new(10.0, 7.0)));
                            } else {
                                for (k, v) in [
                                    ("Имя пользователя", self.user_tag.as_str()),
                                    ("О себе", if self.user_bio.is_empty() { "Не заполнено" } else { self.user_bio.as_str() }),
                                    ("Google-аккаунт", if self.user_email.is_empty() { "Не указан" } else { self.user_email.as_str() }),
                                ] {
                                    ui.label(egui::RichText::new(k).size(11.5).color(pal.text_muted));
                                    ui.label(egui::RichText::new(v).size(14.0).color(pal.text_main));
                                    ui.add_space(10.0);
                                }
                            }
                            ui.add_space(14.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if self.profile_editing {
                                    if ui.add_sized(Vec2::new(124.0, 34.0), egui::Button::new(egui::RichText::new("Сохранить").color(Color32::WHITE)).fill(pal.accent_blue).rounding(17.0)).clicked() {
                                        self.profile_editing = false;
                                        save_profile_now = true;
                                        toast_msg = Some("Профиль сохранён".into());
                                    }
                                } else if ui.add_sized(Vec2::new(150.0, 34.0), egui::Button::new(egui::RichText::new("Редактировать").color(Color32::WHITE)).fill(pal.accent_blue).rounding(17.0)).clicked() {
                                    self.profile_editing = true;
                                }
                            });
                        });
                    });
            }

            ActiveModal::CreateGroup => {
                egui::Window::new("Создать группу")
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .fixed_size(Vec2::new(410.0, 480.0))
                    .frame(egui::Frame::none().fill(pal.bg_panel).stroke(Stroke::new(1.0, pal.popup_border)).rounding(Rounding::same(16.0)))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none().inner_margin(egui::Margin::same(18.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Новая группа").font(semibold_font(17.5)).color(pal.text_main));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(egui::RichText::new("✕").color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                        self.active_modal = ActiveModal::None;
                                    }
                                });
                            });
                            ui.add_space(12.0);
                            ui.horizontal(|ui| {
                                let (av_rect, av_resp) = ui.allocate_exact_size(Vec2::splat(68.0), egui::Sense::click());
                                let t: String = self.new_group_name.trim().chars().take(2).collect::<String>().to_uppercase();
                                let preview_tag = if t.is_empty() { "GR".into() } else { t };
                                draw_avatar_circle(ui.painter(), av_rect, &self.new_group_avatar, &preview_tag, GROUP_COLORS[self.new_group_color_idx], 22.0);
                                if av_resp.hovered() {
                                    ui.painter().circle_filled(av_rect.center(), 34.0, Color32::from_rgba_unmultiplied(0, 0, 0, 125));
                                    ui.painter().text(av_rect.center(), egui::Align2::CENTER_CENTER, "Фото", semibold_font(12.0), Color32::WHITE);
                                }
                                if av_resp.on_hover_text("Выбрать аватарку группы").clicked() { pick_new_group_avatar = true; }
                                ui.add_space(10.0);
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new("Название группы").size(12.0).color(pal.text_muted));
                                    ui.add(egui::TextEdit::singleline(&mut self.new_group_name).hint_text("Например: CS2 Squad").desired_width(268.0).margin(Vec2::new(10.0, 8.0)));
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        for (ci, &col) in GROUP_COLORS.iter().enumerate() {
                                            let (cr, resp) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
                                            ui.painter().circle_filled(cr.center(), 8.5, col);
                                            if self.new_group_color_idx == ci {
                                                ui.painter().circle_stroke(cr.center(), 10.0, Stroke::new(1.8, Color32::WHITE));
                                            }
                                            if resp.clicked() {
                                                self.new_group_color_idx = ci;
                                                self.new_group_avatar = None;
                                                self.new_group_avatar_path = None;
                                            }
                                        }
                                    });
                                });
                            });
                            ui.add_space(14.0);
                            ui.label(egui::RichText::new("Описание (необязательно)").size(12.0).color(pal.text_muted));
                            ui.add(egui::TextEdit::multiline(&mut self.new_group_desc).hint_text("О чём эта группа...").desired_rows(2).desired_width(354.0).margin(Vec2::new(10.0, 8.0)));
                            ui.add_space(12.0);
                            ui.label(egui::RichText::new("Тип группы").size(12.0).color(pal.text_muted));
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                for (is_pub, lbl) in [(false, "Частная"), (true, "Публичная")] {
                                    let active = self.new_group_public == is_pub;
                                    if ui
                                        .add_sized(
                                            Vec2::new(112.0, 30.0),
                                            egui::Button::new(egui::RichText::new(lbl).size(13.0).color(if active { Color32::WHITE } else { pal.text_muted }))
                                                .fill(if active { pal.accent_blue } else { pal.bg_input })
                                                .rounding(15.0),
                                        )
                                        .clicked()
                                    {
                                        self.new_group_public = is_pub;
                                    }
                                }
                            });
                            if self.new_group_public {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Публичная ссылка (egramm.me/...)").size(12.0).color(pal.text_muted));
                                ui.add(egui::TextEdit::singleline(&mut self.new_group_link).hint_text("my_squad").desired_width(354.0).margin(Vec2::new(10.0, 7.0)));
                            }
                            ui.add_space(18.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add_sized(Vec2::new(130.0, 36.0), egui::Button::new(egui::RichText::new("Создать").color(Color32::WHITE)).fill(pal.accent_blue).rounding(18.0)).clicked() {
                                    let title = self.new_group_name.trim().to_string();
                                    if !title.is_empty() {
                                        let tag: String = title.chars().take(2).collect::<String>().to_uppercase();
                                        let link = if self.new_group_public {
                                            let clean = self.new_group_link.trim();
                                            if clean.is_empty() { format!("egramm.me/group_{}", self.chats.len() + 1) } else { format!("egramm.me/{}", clean) }
                                        } else {
                                            format!("egramm.me/+inv{:04}", (self.chats.len() + 1) * 731)
                                        };
                                        create_group_req = Some((
                                            title,
                                            self.new_group_desc.trim().to_string(),
                                            if tag.is_empty() { "GR".into() } else { tag },
                                            GROUP_COLORS[self.new_group_color_idx],
                                            self.new_group_avatar.take(),
                                            self.new_group_avatar_path.take(),
                                            self.new_group_public,
                                            link,
                                        ));
                                        self.new_group_name.clear();
                                        self.new_group_desc.clear();
                                        self.new_group_link.clear();
                                        self.active_modal = ActiveModal::None;
                                    }
                                }
                                if ui.add_sized(Vec2::new(90.0, 36.0), egui::Button::new(egui::RichText::new("Отмена").color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(18.0)).clicked() {
                                    self.active_modal = ActiveModal::None;
                                }
                            });
                        });
                    });
            }

            ActiveModal::ManageGroup(g_idx) => {
                if g_idx >= self.chats.len() {
                    self.active_modal = ActiveModal::None;
                } else {
                    egui::Window::new("Управление группой")
                        .title_bar(false)
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                        .fixed_size(Vec2::new(500.0, 560.0))
                        .frame(egui::Frame::none().fill(pal.bg_panel).stroke(Stroke::new(1.0, pal.popup_border)).rounding(Rounding::same(16.0)))
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            let mut close_modal = false;
                            egui::Frame::none().inner_margin(egui::Margin::symmetric(18.0, 14.0)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Управление группой").font(semibold_font(17.5)).color(pal.text_main));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(egui::Button::new(egui::RichText::new("✕").size(15.0).color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                            close_modal = true;
                                        }
                                    });
                                });
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    for (tab, label) in [(GroupTab::Info, "Инфо"), (GroupTab::Members, "Участники"), (GroupTab::Admins, "Админы"), (GroupTab::Permissions, "Права")] {
                                        let active = self.group_tab == tab;
                                        if ui
                                            .add_sized(
                                                Vec2::new(108.0, 32.0),
                                                egui::Button::new(egui::RichText::new(label).font(semibold_font(13.0)).color(if active { Color32::WHITE } else { pal.text_muted }))
                                                    .fill(if active { pal.accent_blue } else { pal.bg_input })
                                                    .rounding(16.0),
                                            )
                                            .clicked()
                                        {
                                            self.group_tab = tab;
                                            self.editing_admin_idx = None;
                                        }
                                    }
                                });
                                ui.add_space(14.0);
                                let chat = &mut self.chats[g_idx];
                                ui.push_id("manage_group_scroll", |ui| {
                                    egui::ScrollArea::vertical().max_height(442.0).show(ui, |ui| match self.group_tab {
                                        GroupTab::Info => {
                                            if !chat.invite_code.is_empty() {
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(format!("Код группы: {}", chat.invite_code)).font(semibold_font(12.5)).color(pal.accent_light));
                                                    if ui.add(egui::Button::new(egui::RichText::new("Копировать код").size(11.5).color(pal.text_muted)).fill(pal.bg_input).rounding(10.0)).clicked() {
                                                        ctx.copy_text(chat.invite_code.clone());
                                                        toast_msg = Some("Код группы скопирован".into());
                                                    }
                                                });
                                                ui.add_space(8.0);
                                            }
                                            ui.horizontal(|ui| {
                                                let (av_rect, av_resp) = ui.allocate_exact_size(Vec2::splat(74.0), egui::Sense::click());
                                                draw_avatar_circle(ui.painter(), av_rect, &chat.avatar_tex, &chat.avatar_tag, chat.avatar_color, 24.0);
                                                if av_resp.hovered() {
                                                    ui.painter().circle_filled(av_rect.center(), 37.0, Color32::from_rgba_unmultiplied(0, 0, 0, 125));
                                                    ui.painter().text(av_rect.center(), egui::Align2::CENTER_CENTER, "Фото", semibold_font(12.5), Color32::WHITE);
                                                }
                                                if av_resp.clicked() { pick_existing_group_avatar = Some(g_idx); }
                                                ui.add_space(12.0);
                                                ui.vertical(|ui| {
                                                    ui.label(egui::RichText::new("Название").size(12.0).color(pal.text_muted));
                                                    if ui.add(egui::TextEdit::singleline(&mut chat.title).desired_width(345.0).margin(Vec2::new(10.0, 8.0))).changed() {
                                                        let tag: String = chat.title.trim().chars().take(2).collect::<String>().to_uppercase();
                                                        if !tag.is_empty() { chat.avatar_tag = tag; }
                                                    }
                                                    ui.add_space(6.0);
                                                    ui.horizontal(|ui| {
                                                        if ui.add_sized(Vec2::new(124.0, 26.0), egui::Button::new(egui::RichText::new("Сменить аватар").size(12.0).color(pal.accent_light)).fill(pal.bg_input).rounding(13.0)).clicked() {
                                                            pick_existing_group_avatar = Some(g_idx);
                                                        }
                                                        if chat.avatar_tex.is_some()
                                                            && ui.add_sized(Vec2::new(82.0, 26.0), egui::Button::new(egui::RichText::new("Сбросить").size(12.0).color(DANGER_RED)).fill(Color32::TRANSPARENT).rounding(13.0)).clicked()
                                                        {
                                                            chat.avatar_tex = None;
                                                        }
                                                    });
                                                });
                                            });
                                            ui.add_space(12.0);
                                            ui.label(egui::RichText::new("Цвет иконки").size(12.0).color(pal.text_muted));
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                for &col in &GROUP_COLORS {
                                                    let (cr, resp) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());
                                                    ui.painter().circle_filled(cr.center(), 10.0, col);
                                                    if chat.avatar_color == col && chat.avatar_tex.is_none() {
                                                        ui.painter().circle_stroke(cr.center(), 11.5, Stroke::new(2.0, Color32::WHITE));
                                                    }
                                                    if resp.clicked() {
                                                        chat.avatar_color = col;
                                                        chat.avatar_tex = None;
                                                    }
                                                }
                                            });
                                            ui.add_space(12.0);
                                            ui.label(egui::RichText::new("Описание").size(12.0).color(pal.text_muted));
                                            ui.add(egui::TextEdit::multiline(&mut chat.description).hint_text("Добавьте описание группы...").desired_rows(3).desired_width(440.0).margin(Vec2::new(10.0, 8.0)));
                                            ui.add_space(12.0);
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("Публичная группа").size(13.5).color(pal.text_main));
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { draw_toggle_switch(ui, &mut chat.is_public, &pal); });
                                            });
                                            ui.add_space(6.0);
                                            ui.label(egui::RichText::new(if chat.is_public { "Публичная ссылка" } else { "Пригласительная ссылка" }).size(12.0).color(pal.text_muted));
                                            ui.horizontal(|ui| {
                                                ui.add(egui::TextEdit::singleline(&mut chat.invite_link).desired_width(332.0).margin(Vec2::new(10.0, 7.0)));
                                                if ui.add_sized(Vec2::new(98.0, 32.0), egui::Button::new(egui::RichText::new("Копировать").size(12.5).color(pal.accent_light)).fill(pal.bg_input).rounding(14.0)).clicked() {
                                                    ctx.copy_text(chat.invite_link.clone());
                                                    toast_msg = Some("Ссылка скопирована".into());
                                                }
                                            });
                                            ui.add_space(18.0);
                                            if ui.add_sized(Vec2::new(165.0, 34.0), egui::Button::new(egui::RichText::new("Удалить группу").color(DANGER_RED)).fill(Color32::from_rgba_unmultiplied(235, 95, 95, 28)).rounding(17.0)).clicked() {
                                                delete_group_idx = Some(g_idx);
                                                close_modal = true;
                                            }
                                        }
                                        GroupTab::Members => {
                                            ui.horizontal(|ui| {
                                                ui.add(egui::TextEdit::singleline(&mut self.add_member_input).hint_text("Введите @ник или почту зарегистрированного").desired_width(325.0).margin(Vec2::new(10.0, 8.0)));
                                                if ui.add_sized(Vec2::new(106.0, 34.0), egui::Button::new(egui::RichText::new("Пригласить").color(Color32::WHITE)).fill(pal.accent_blue).rounding(17.0)).clicked() {
                                                    let raw = self.add_member_input.trim().to_string();
                                                    if !raw.is_empty() {
                                                        invite_member_req = Some((g_idx, raw));
                                                        self.add_member_input.clear();
                                                    }
                                                }
                                            });
                                            ui.add_space(12.0);
                                            let mut kick_idx: Option<usize> = None;
                                            let mut promote_idx: Option<usize> = None;
                                            for (m_idx, member) in chat.members.iter().enumerate() {
                                                let card = egui::Frame::none().fill(pal.bg_input).rounding(Rounding::same(12.0)).inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        let (r, _) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::hover());
                                                        let init: String = member.name.chars().take(2).collect::<String>().to_uppercase();
                                                        draw_avatar_circle(ui.painter(), r, &member.avatar_tex, &init, member.avatar_color, 13.0);
                                                        ui.vertical(|ui| {
                                                            ui.horizontal(|ui| {
                                                                ui.label(egui::RichText::new(&member.name).font(semibold_font(14.0)).color(pal.text_main));
                                                                if member.role != GroupRole::Member {
                                                                    let badge = if !member.custom_title.is_empty() { member.custom_title.as_str() } else { member.role.label() };
                                                                    ui.label(egui::RichText::new(badge).size(11.5).color(pal.accent_light));
                                                                }
                                                            });
                                                            ui.label(egui::RichText::new(format!("{} • {}", member.tag, if member.online { "в сети" } else { "не в сети" })).size(11.5).color(pal.text_muted));
                                                        });
                                                        if member.role != GroupRole::Owner {
                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                if ui.add(egui::Button::new(egui::RichText::new("Удалить").size(12.0).color(DANGER_RED)).fill(Color32::TRANSPARENT).rounding(10.0)).clicked() {
                                                                    kick_idx = Some(m_idx);
                                                                }
                                                                if member.role == GroupRole::Member
                                                                    && ui.add(egui::Button::new(egui::RichText::new("Сделать админом").size(12.0).color(pal.accent_light)).fill(pal.bg_panel).rounding(10.0)).clicked()
                                                                {
                                                                    promote_idx = Some(m_idx);
                                                                }
                                                            });
                                                        }
                                                    });
                                                }).response.interact(egui::Sense::click());
                                                if card.on_hover_text("Нажмите, чтобы посмотреть профиль").clicked() {
                                                    open_member_prof_req = Some((g_idx, m_idx));
                                                }
                                                ui.add_space(6.0);
                                            }
                                            if let Some(p_idx) = promote_idx {
                                                if let Some(m) = chat.members.get_mut(p_idx) {
                                                    m.role = GroupRole::Admin;
                                                    m.rights = AdminRights::default_admin();
                                                    if m.custom_title.is_empty() { m.custom_title = "Администратор".into(); }
                                                    self.group_tab = GroupTab::Admins;
                                                    self.editing_admin_idx = Some(p_idx);
                                                    toast_msg = Some(format!("{} назначен админом", m.name));
                                                }
                                            }
                                            if let Some(k_idx) = kick_idx {
                                                if k_idx < chat.members.len() {
                                                    let removed = chat.members.remove(k_idx);
                                                    chat.refresh_group_subtitle();
                                                    toast_msg = Some(format!("{} исключён из группы", removed.name));
                                                }
                                            }
                                        }
                                        GroupTab::Admins => {
                                            if let Some(a_idx) = self.editing_admin_idx {
                                                if let Some(admin) = chat.members.get_mut(a_idx) {
                                                    ui.horizontal(|ui| {
                                                        if ui.add(egui::Button::new(egui::RichText::new("← Назад").color(pal.accent_light)).fill(pal.bg_input).rounding(12.0)).clicked() {
                                                            self.editing_admin_idx = None;
                                                        }
                                                        ui.label(egui::RichText::new(format!("Права: {}", admin.name)).font(semibold_font(15.0)).color(pal.text_main));
                                                    });
                                                    ui.add_space(10.0);
                                                    ui.label(egui::RichText::new("Должность").size(12.0).color(pal.text_muted));
                                                    ui.add(egui::TextEdit::singleline(&mut admin.custom_title).hint_text("Например: Модератор").desired_width(430.0).margin(Vec2::new(10.0, 7.0)));
                                                    ui.add_space(12.0);
                                                    let mut perm_row = |ui: &mut egui::Ui, title: &str, flag: &mut bool| {
                                                        ui.horizontal(|ui| {
                                                            ui.label(egui::RichText::new(title).size(13.5).color(pal.text_main));
                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { draw_toggle_switch(ui, flag, &pal); });
                                                        });
                                                        ui.add_space(6.0);
                                                    };
                                                    perm_row(ui, "Изменение профиля группы", &mut admin.rights.change_info);
                                                    perm_row(ui, "Удаление сообщений", &mut admin.rights.delete_messages);
                                                    perm_row(ui, "Блокировка участников", &mut admin.rights.ban_users);
                                                    perm_row(ui, "Приглашение по ссылке", &mut admin.rights.invite_users);
                                                    perm_row(ui, "Закрепление сообщений", &mut admin.rights.pin_messages);
                                                    perm_row(ui, "Управление войс-чатами", &mut admin.rights.manage_voice);
                                                    perm_row(ui, "Назначение новых админов", &mut admin.rights.add_admins);

                                                    if admin.role == GroupRole::Admin {
                                                        ui.add_space(10.0);
                                                        if ui.add_sized(Vec2::new(190.0, 34.0), egui::Button::new(egui::RichText::new("Снять права админа").color(DANGER_RED)).fill(Color32::from_rgba_unmultiplied(235, 95, 95, 28)).rounding(17.0)).clicked() {
                                                            admin.role = GroupRole::Member;
                                                            admin.custom_title.clear();
                                                            admin.rights = AdminRights::none();
                                                            self.editing_admin_idx = None;
                                                            toast_msg = Some("Права администратора сняты".into());
                                                        }
                                                    }
                                                } else {
                                                    self.editing_admin_idx = None;
                                                }
                                            } else {
                                                for (m_idx, member) in chat.members.iter().enumerate() {
                                                    if member.role == GroupRole::Member { continue; }
                                                    let card = egui::Frame::none()
                                                        .fill(pal.bg_input)
                                                        .rounding(Rounding::same(12.0))
                                                        .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                                                        .show(ui, |ui| {
                                                            ui.horizontal(|ui| {
                                                                let (r, _) = ui.allocate_exact_size(Vec2::splat(38.0), egui::Sense::hover());
                                                                let init: String = member.name.chars().take(2).collect::<String>().to_uppercase();
                                                                draw_avatar_circle(ui.painter(), r, &member.avatar_tex, &init, member.avatar_color, 13.0);
                                                                ui.vertical(|ui| {
                                                                    ui.label(egui::RichText::new(&member.name).font(semibold_font(14.0)).color(pal.text_main));
                                                                    let sub = if !member.custom_title.is_empty() {
                                                                        format!("{} • {}", member.role.label(), member.custom_title)
                                                                    } else {
                                                                        member.role.label().to_string()
                                                                    };
                                                                    ui.label(egui::RichText::new(sub).size(12.0).color(pal.accent_light));
                                                                });
                                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                    if ui.add(egui::Button::new(egui::RichText::new("Права ›").size(12.5).color(pal.accent_light)).fill(pal.bg_panel).rounding(10.0)).clicked() {
                                                                        self.editing_admin_idx = Some(m_idx);
                                                                    }
                                                                });
                                                            });
                                                        })
                                                        .response
                                                        .interact(egui::Sense::click());
                                                    if card.clicked() { self.editing_admin_idx = Some(m_idx); }
                                                    ui.add_space(6.0);
                                                }
                                                ui.add_space(8.0);
                                                if ui.add_sized(Vec2::new(220.0, 34.0), egui::Button::new(egui::RichText::new("+ Назначить из участников").color(Color32::WHITE)).fill(pal.accent_blue).rounding(17.0)).clicked() {
                                                    self.group_tab = GroupTab::Members;
                                                }
                                            }
                                        }
                                        GroupTab::Permissions => {
                                            ui.label(egui::RichText::new("Что могут делать обычные участники группы?").font(semibold_font(13.5)).color(pal.accent_light));
                                            ui.add_space(10.0);
                                            let mut perm_row = |ui: &mut egui::Ui, title: &str, flag: &mut bool| {
                                                ui.horizontal(|ui| {
                                                    ui.label(egui::RichText::new(title).size(14.0).color(pal.text_main));
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { draw_toggle_switch(ui, flag, &pal); });
                                                });
                                                ui.add_space(8.0);
                                            };
                                            perm_row(ui, "Отправка текстовых сообщений", &mut chat.permissions.send_messages);
                                            perm_row(ui, "Отправка фото, видео и файлов", &mut chat.permissions.send_media);
                                            perm_row(ui, "Создание опросов", &mut chat.permissions.send_polls);
                                            perm_row(ui, "Добавление новых участников", &mut chat.permissions.invite_members);
                                            ui.add_space(14.0);
                                            ui.label(egui::RichText::new("Медленный режим (Slow Mode)").font(semibold_font(13.5)).color(pal.accent_light));
                                            ui.add_space(8.0);
                                            ui.horizontal(|ui| {
                                                for (secs, lbl) in [(0, "Выкл"), (10, "10 сек"), (30, "30 сек"), (60, "1 мин")] {
                                                    let active = chat.permissions.slow_mode_secs == secs;
                                                    if ui
                                                        .add_sized(
                                                            Vec2::new(82.0, 30.0),
                                                            egui::Button::new(egui::RichText::new(lbl).size(12.5).color(if active { Color32::WHITE } else { pal.text_muted }))
                                                                .fill(if active { pal.accent_blue } else { pal.bg_input })
                                                                .rounding(15.0),
                                                        )
                                                        .clicked()
                                                    {
                                                        chat.permissions.slow_mode_secs = secs;
                                                    }
                                                }
                                            });
                                        }
                                    });
                                });
                            });
                            if close_modal {
                                self.active_modal = ActiveModal::None;
                                self.editing_admin_idx = None;
                            }
                        });
                }
            }

            ActiveModal::CreatePoll => {
                egui::Window::new("Создать опрос")
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .default_width(340.0)
                    .min_width(340.0)
                    .max_width(340.0)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none().inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Новый опрос").font(semibold_font(17.0)).color(pal.text_main));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.add(egui::Button::new(egui::RichText::new("✕").color(pal.text_muted)).fill(Color32::TRANSPARENT).rounding(14.0)).clicked() {
                                        self.active_modal = ActiveModal::None;
                                    }
                                });
                            });
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new("Вопрос").size(12.5).color(pal.text_muted));
                            ui.add(egui::TextEdit::singleline(&mut self.poll_question).hint_text("Во сколько катаем?").desired_width(292.0).margin(Vec2::new(10.0, 8.0)));
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new("Варианты ответа").size(12.5).color(pal.text_muted));
                            ui.add(egui::TextEdit::singleline(&mut self.poll_option_1).hint_text("Вариант 1").desired_width(292.0).margin(Vec2::new(10.0, 8.0)));
                            ui.add_space(6.0);
                            ui.add(egui::TextEdit::singleline(&mut self.poll_option_2).hint_text("Вариант 2").desired_width(292.0).margin(Vec2::new(10.0, 8.0)));
                            ui.add_space(14.0);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add_sized(Vec2::new(110.0, 34.0), egui::Button::new(egui::RichText::new("Создать").color(Color32::WHITE)).fill(pal.accent_blue).rounding(17.0)).clicked() {
                                    let q = self.poll_question.trim().to_string();
                                    if !q.is_empty() {
                                        let o1 = if self.poll_option_1.trim().is_empty() { "Да".into() } else { self.poll_option_1.trim().to_string() };
                                        let o2 = if self.poll_option_2.trim().is_empty() { "Нет".into() } else { self.poll_option_2.trim().to_string() };
                                        self.pending_attachment = Some(Attachment {
                                            kind: AttachmentKind::Poll,
                                            title: q,
                                            subtitle: format!("• {}   • {}", o1, o2),
                                            local_path: None,
                                            drive_file_id: None,
                                        });
                                        send_poll_now = true;
                                        self.poll_question.clear();
                                        self.poll_option_1.clear();
                                        self.poll_option_2.clear();
                                        self.active_modal = ActiveModal::None;
                                    }
                                }
                            });
                        });
                    });
            }
            ActiveModal::None => {}
        }

        if save_profile_now {
            self.publish_profile_update();
        }
        if let Some((g_idx, raw_user)) = invite_member_req {
            self.invite_or_add_member_to_group(ctx, g_idx, raw_user);
        }
        if let Some((g_idx, m_idx)) = open_member_prof_req {
            self.open_member_profile(g_idx, m_idx);
        }
        if let Some(target_email) = dm_from_profile_email {
            self.open_direct_chat(ctx, target_email);
        }
        if let Some((title, desc, tag, col, av_tex, av_path, is_pub, link)) = create_group_req {
            self.create_synced_group(ctx, title, desc, tag, col, av_tex, av_path, is_pub, link);
        }
        if let Some((from_c, m_i, to_c)) = forward_req {
            self.forward_message(ctx, from_c, m_i, to_c);
        }
        if send_poll_now {
            self.send_message(ctx);
        }
        if let Some(msg) = toast_msg {
            self.show_toast(msg);
        }
        if pick_user_avatar {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Выберите аватарку профиля")
                .add_filter("Изображения", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file()
            {
                self.upload_and_publish_user_avatar(ctx, path);
            }
        } else if clear_user_avatar {
            self.clear_and_publish_user_avatar();
        }
        if pick_new_group_avatar {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Выберите аватарку для новой группы")
                .add_filter("Изображения", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file()
            {
                if let Some(tex) = load_circular_avatar(ctx, &path, "new_group_av") {
                    self.new_group_avatar = Some(tex);
                    self.new_group_avatar_path = Some(path);
                } else {
                    self.show_toast("Не удалось открыть изображение");
                }
            }
        }
        if let Some(g_idx) = pick_existing_group_avatar {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Выберите новую аватарку группы")
                .add_filter("Изображения", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file()
            {
                self.upload_and_publish_group_avatar(ctx, g_idx, path);
            }
        }
        if let Some(del_idx) = delete_group_idx {
            if del_idx < self.chats.len() {
                let removed = self.chats.remove(del_idx);
                if self.selected_chat == Some(del_idx) {
                    self.selected_chat = if self.chats.is_empty() { None } else { Some(0) };
                } else if let Some(sel) = self.selected_chat {
                    if sel > del_idx {
                        self.selected_chat = Some(sel - 1);
                    }
                }
                self.show_toast(format!("Группа «{}» удалена", removed.title));
            }
        }

        if let Some((msg, started)) = self.toast.clone() {
            let age = started.elapsed().as_secs_f32();
            if age < 2.2 {
                ctx.request_repaint();
                let alpha = if age < 0.15 {
                    age / 0.15
                } else if age > 1.8 {
                    ((2.2 - age) / 0.4).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                egui::Area::new(egui::Id::new("egramm_toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(140.0, -74.0))
                    .order(egui::Order::Foreground)
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(with_alpha(Color32::from_rgb(20, 30, 42), 0.92 * alpha))
                            .rounding(Rounding::same(18.0))
                            .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(msg).size(13.0).color(with_alpha(Color32::WHITE, alpha)));
                            });
                    });
            } else {
                self.toast = None;
            }
        }

        self.show_call_window(ctx);
        self.show_lightbox(ctx);
        self.show_intro_animation(ctx);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Egramm")
            .with_inner_size([1020.0, 680.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Egramm",
        options,
        Box::new(|cc| Ok(Box::new(EgrammApp::new(cc)))),
    )
}