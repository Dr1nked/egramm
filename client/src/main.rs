#![windows_subsystem = "windows"]

mod addons;
mod models;
mod ui_helpers;

use addons::{
    clear_session, decrypt_payload, encrypt_payload, load_session, save_session, SavedSession,
    SoundPlayer,
};
use base64::{Engine as _};
use chrono::Local;
use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Rounding, Stroke,
    Vec2,
};
use models::{
    apply_theme_style, call_encryption_fingerprint, drive_cache_dir, ease_out_cubic,
    generate_call_id, get_or_load_image, is_image_name, load_circular_avatar,
    load_circular_avatar_from_bytes, notify_windows_message, semibold_font, with_alpha,
    ActiveModal, AdminRights, Attachment, AttachmentKind, CallStatus, Chat, ChatDto,
    ChatMessageDto, CreateChatRequest, EgrammApp, GroupMember, GroupPermissions,
    GroupRole, GroupTab, InviteMemberRequest, InviteMemberResponse, JoinChatRequest, Message,
    NetEvent, OpenDmRequest, PollResponse, ReplyTarget, ScreenShareEngine,
    SearchUsersResponse, SharedAudioBridge, SharedScreenBridge, StartAuthResponse,
    StoredAttachment, SyncResponse, ThemePalette, UploadResponse, ViewedProfile,
    VoiceEngine, WsWireEvent, AUTH_BLUE, DANGER_RED, GROUP_COLORS, MAX_CALL_PARTICIPANTS,
    ONLINE_GREEN, SERVER_URL, WS_URL,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ui_helpers::{
    draw_avatar_circle, draw_call_circle_btn, draw_call_fingerprint_glyph, draw_copy_icon,
    draw_drawer_row, draw_exit_fullscreen_icon, draw_forward_icon, draw_fullscreen_icon,
    draw_group_icon, draw_hangup_icon, draw_lock_icon, draw_logout_icon, draw_menu_icon,
    draw_mic_icon, draw_paperclip_button, draw_person_icon, draw_phone_icon,
    draw_reaction_avatar_badge, draw_read_ticks, draw_reply_icon, draw_reply_quote_bar,
    draw_screen_share_icon, draw_send_button, draw_sliders_icon, draw_speaker_icon,
    draw_theme_slider_row, draw_toggle_switch, draw_trash_icon, draw_video_play_icon,
};

impl Default for EgrammApp {
    fn default() -> Self {
        let now = Instant::now();
        let (net_tx, net_rx) = mpsc::channel();

        Self {
            reply_target: None,
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
            screen_bridge: Arc::new(SharedScreenBridge::new()),
            screen_engine: None,
            screen_tex: None,
            screen_last_seq: 0,
            screen_streamer_email: String::new(),
            screen_streamer_name: String::new(),
            screen_frame_size: (0, 0),
            screen_watching: false,
            screen_fullscreen: false,
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

        let mut app = Self::default();
        app.screen_bridge.set_repaint_ctx(cc.egui_ctx.clone());
        if let Some(saved) = load_session() {
            app.username = saved.name;
            app.user_email = saved.email.clone();
            app.user_avatar_id = saved.avatar_file_id.clone();
            app.user_bio = saved.bio;
            app.audio_bridge.set_my_email(saved.email.clone());
            app.screen_bridge.set_my_email(saved.email.clone());
            let nick = saved.email.split('@').next().unwrap_or("user");
            app.user_tag = format!("@{}", nick);
            app.session_token = Some(saved.session_token);
            app.is_logged_in = true;
            app.has_played_intro = true;
            if let Some(fid) = saved.avatar_file_id {
                app.request_avatar_download(&cc.egui_ctx, &fid);
            }
            app.start_realtime_sync(cc.egui_ctx.clone());
        }
        let palette = app.palette();
        apply_theme_style(&cc.egui_ctx, &palette, app.is_dark_mode);
        app
    }

    pub fn palette(&self) -> ThemePalette {
        if self.is_dark_mode {
            ThemePalette::dark()
        } else {
            ThemePalette::light()
        }
    }

    fn show_toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn stop_screen_share(&mut self) {
        if let Some(engine) = self.screen_engine.take() {
            engine.stop();
        }
        self.screen_bridge.is_sharing.store(false, Ordering::Relaxed);
    }

    fn clear_remote_screen_state(&mut self) {
        self.screen_bridge.clear_all();
        self.screen_tex = None;
        self.screen_last_seq = 0;
        self.screen_streamer_email.clear();
        self.screen_streamer_name.clear();
        self.screen_frame_size = (0, 0);
        self.screen_watching = false;
        self.screen_fullscreen = false;
    }

    fn toggle_screen_share(&mut self, ctx: &egui::Context) {
        if self.screen_engine.is_some() {
            self.stop_screen_share();
            self.clear_remote_screen_state();
            self.show_toast("Демонстрация экрана остановлена");
            return;
        }

        let CallStatus::Connected {
            chat_id,
            call_id,
            ..
        } = self.call_status.clone()
        else {
            self.show_toast("Демонстрация доступна во время активного звонка");
            return;
        };

        let (Some(token), Some(ws_tx)) = (self.session_token.clone(), self.ws_out_tx.clone())
        else {
            self.show_toast("Нет подключения к серверу");
            return;
        };

        match ScreenShareEngine::start(
            ws_tx,
            token,
            chat_id,
            call_id,
            self.user_email.clone(),
            self.username.clone(),
            self.screen_bridge.clone(),
            ctx.clone(),
        ) {
            Ok(engine) => {
                self.screen_engine = Some(engine);
                self.screen_watching = true;
                self.call_window_expanded = true;
                self.show_toast("Демонстрация экрана запущена");
            }
            Err(e) => {
                self.show_toast(e);
            }
        }
    }

    fn sync_screen_texture(&mut self, ctx: &egui::Context) {
        let Some(frame) = self.screen_bridge.take_if_newer(self.screen_last_seq) else {
            return;
        };
        self.screen_last_seq = frame.seq;
        self.screen_streamer_email = frame.sender_email;
        self.screen_streamer_name = frame.sender_name;
        self.screen_frame_size = (frame.width, frame.height);

        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([frame.width, frame.height], &frame.rgba);

        if let Some(ref mut tex) = self.screen_tex {
            tex.set(color_image, egui::TextureOptions::LINEAR);
        } else {
            self.screen_tex = Some(ctx.load_texture(
                "egramm_call_screen_share",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }
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

    fn send_delete_group(&self, chat_id: i64) {
        let (Some(token), Some(ws_tx)) = (&self.session_token, &self.ws_out_tx) else {
            return;
        };
        let wire = WsWireEvent::DeleteGroup {
            session_token: token.clone(),
            chat_id,
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
        self.screen_bridge.set_active_chat(Some(sc_id));
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
        self.screen_bridge.set_active_chat(Some(chat_id));

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
        self.stop_screen_share();
        self.clear_remote_screen_state();

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
        self.screen_bridge.set_active_chat(None);
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
        let is_me = m.sender_email.eq_ignore_ascii_case(&self.user_email);
        let (text, was_enc) = if m.encrypted {
            decrypt_payload(m.chat_id, &m.text)
        } else {
            (m.text, false)
        };

        let reply_to_preview = m.reply_to_preview.map(|prev| {
            if m.encrypted {
                decrypt_payload(m.chat_id, &prev).0
            } else {
                prev
            }
        });

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
            reply_to_id: m.reply_to_id,
            reply_to_sender: m.reply_to_sender,
            reply_to_preview,
            reactions: m.reactions,
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
            .filter(|m| !m.sender_email.eq_ignore_ascii_case(&self.user_email) && !m.is_read)
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
                let is_owner = gm.email.eq_ignore_ascii_case(&self.user_email)
                    || sc
                    .creator_email
                    .as_deref()
                    .map(|ce| ce.eq_ignore_ascii_case(&gm.email))
                    .unwrap_or(false);
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

    #[allow(clippy::too_many_arguments)]
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
                    reply_to_id: None,
                    reply_to_sender: None,
                    reply_to_preview: None,
                    attachment: wire_att,
                };
                if let Ok(json) = serde_json::to_string(&wire) {
                    let _ = ws_tx.send(json);
                }
                ctx.request_repaint();
            } else {
                dst_chat.messages.push(Message {
                    reply_to_id: None,
                    reply_to_sender: None,
                    reply_to_preview: None,
                    reactions: Vec::new(),
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

    pub fn cancel_google_login(&mut self) {
        self.auth_attempt = self.auth_attempt.wrapping_add(1);
        self.is_authenticating = false;
        self.auth_url = None;
        self.auth_error = None;
    }

    pub fn start_google_login(&mut self) {
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
        let screen_ws = self.screen_bridge.clone();

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

                    while let Some(item) = pending_queue.first().cloned() {
                        if ws.send(tungstenite::Message::Text(item.into())).is_err() {
                            disconnected = true;
                            pending_queue.retain(|q| {
                                !q.contains("\"type\":\"call_audio\"")
                                    && !q.contains("\"type\":\"call_screen_frame\"")
                            });
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
                                    WsWireEvent::ReactionUpdated {
                                        chat_id,
                                        message_id,
                                        reactions,
                                    } => {
                                        let _ = read_tx.send(NetEvent::ReactionUpdated {
                                            chat_id,
                                            message_id,
                                            reactions,
                                        });
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
                                    WsWireEvent::ChatDeleted { chat_id } => {
                                        let _ = read_tx.send(NetEvent::ChatDeleted(chat_id));
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
                                    WsWireEvent::CallScreenFrame {
                                        chat_id,
                                        sender_email,
                                        sender_name,
                                        jpeg_b64,
                                        ..
                                    } => {
                                        screen_ws.push_remote_frame(
                                            chat_id,
                                            &sender_email,
                                            &sender_name,
                                            &jpeg_b64,
                                        );
                                    }
                                    WsWireEvent::CallScreenStop {
                                        chat_id,
                                        sender_email,
                                        ..
                                    } => {
                                        let _ = read_tx.send(NetEvent::CallScreenStop {
                                            chat_id,
                                            sender_email,
                                        });
                                        read_ctx.request_repaint();
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
        let reply = self.reply_target.take();

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

            let reply_to_id = reply.as_ref().and_then(|r| r.message_id);
            let reply_to_sender = reply.as_ref().map(|r| r.sender.clone());
            let reply_to_preview = reply.map(|r| {
                if enc_flag {
                    encrypt_payload(server_chat_id, &r.preview)
                } else {
                    r.preview
                }
            });

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
                                        reply_to_id,
                                        reply_to_sender,
                                        reply_to_preview,
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
                        reply_to_id,
                        reply_to_sender,
                        reply_to_preview,
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
                    reply_to_id,
                    reply_to_sender,
                    reply_to_preview,
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
            reply_to_id: reply.as_ref().and_then(|r| r.message_id),
            reply_to_sender: reply.as_ref().map(|r| r.sender.clone()),
            reply_to_preview: reply.map(|r| r.preview),
            reactions: Vec::new(),
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

    fn show_screen_fullscreen_overlay(&mut self, ctx: &egui::Context) {
        if !self.screen_fullscreen || self.call_status == CallStatus::Idle {
            return;
        }
        let Some(tex) = self.screen_tex.clone() else {
            self.screen_fullscreen = false;
            return;
        };

        let screen = ctx.screen_rect();
        let mut exit_fs = false;
        let mut toggle_share = false;
        let is_sharing_me = self.screen_engine.is_some();
        let streamer = if self.screen_streamer_name.is_empty() {
            "Участник звонка".to_string()
        } else {
            self.screen_streamer_name.clone()
        };
        let (fw, fh) = self.screen_frame_size;

        egui::Area::new(egui::Id::new("egramm_screen_fullscreen_overlay"))
            .fixed_pos(screen.min)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                let (bg_rect, _) =
                    ui.allocate_exact_size(screen.size(), egui::Sense::click());
                ui.painter()
                    .rect_filled(bg_rect, Rounding::ZERO, Color32::from_rgb(7, 11, 17));

                let top_bar_h = 48.0;
                let avail_w = (screen.width() - 32.0).max(200.0);
                let avail_h = (screen.height() - top_bar_h - 24.0).max(160.0);
                let orig = tex.size_vec2();
                let scale = (avail_w / orig.x.max(1.0)).min(avail_h / orig.y.max(1.0));
                let disp = orig * scale;
                let video_center = Pos2::new(
                    screen.center().x,
                    screen.min.y + top_bar_h + (screen.height() - top_bar_h) * 0.5,
                );
                let video_rect = Rect::from_center_size(video_center, disp);

                ui.painter().rect_filled(
                    video_rect.expand(1.5),
                    Rounding::same(10.0),
                    Color32::from_rgb(36, 54, 76),
                );
                ui.painter().image(
                    tex.id(),
                    video_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );

                let top_rect = Rect::from_min_size(
                    Pos2::new(screen.min.x + 14.0, screen.min.y + 10.0),
                    Vec2::new(screen.width() - 28.0, 38.0),
                );
                ui.painter().rect_filled(
                    top_rect,
                    Rounding::same(14.0),
                    Color32::from_rgba_unmultiplied(18, 28, 40, 232),
                );
                ui.painter().rect_stroke(
                    top_rect,
                    Rounding::same(14.0),
                    Stroke::new(1.0, Color32::from_rgb(44, 66, 92)),
                );

                let live_badge = Rect::from_min_size(
                    Pos2::new(top_rect.min.x + 12.0, top_rect.center().y - 10.0),
                    Vec2::new(42.0, 20.0),
                );
                ui.painter()
                    .rect_filled(live_badge, Rounding::same(6.0), DANGER_RED);
                ui.painter().text(
                    live_badge.center(),
                    egui::Align2::CENTER_CENTER,
                    "LIVE",
                    semibold_font(10.5),
                    Color32::WHITE,
                );

                let title_str = if fw > 0 && fh > 0 {
                    format!("Экран: {}  •  {}x{}", streamer, fw, fh)
                } else {
                    format!("Экран: {}", streamer)
                };
                ui.painter().text(
                    Pos2::new(live_badge.max.x + 10.0, top_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    title_str,
                    semibold_font(13.5),
                    Color32::WHITE,
                );

                let exit_btn_rect = Rect::from_center_size(
                    Pos2::new(top_rect.max.x - 24.0, top_rect.center().y),
                    Vec2::splat(30.0),
                );
                let exit_resp = ui.interact(
                    exit_btn_rect,
                    egui::Id::new("fs_exit_btn"),
                    egui::Sense::click(),
                );
                if exit_resp.hovered() {
                    ui.painter().circle_filled(
                        exit_btn_rect.center(),
                        15.0,
                        Color32::from_rgb(40, 58, 80),
                    );
                }
                draw_exit_fullscreen_icon(
                    ui.painter(),
                    exit_btn_rect.center(),
                    Color32::WHITE,
                );
                if exit_resp
                    .on_hover_text("Выйти из полноэкранного режима (Esc)")
                    .clicked()
                {
                    exit_fs = true;
                }

                if is_sharing_me {
                    let stop_rect = Rect::from_center_size(
                        Pos2::new(exit_btn_rect.min.x - 76.0, top_rect.center().y),
                        Vec2::new(132.0, 26.0),
                    );
                    let stop_resp = ui.interact(
                        stop_rect,
                        egui::Id::new("fs_stop_share_btn"),
                        egui::Sense::click(),
                    );
                    ui.painter().rect_filled(
                        stop_rect,
                        Rounding::same(13.0),
                        if stop_resp.hovered() {
                            Color32::from_rgb(248, 112, 112)
                        } else {
                            DANGER_RED
                        },
                    );
                    ui.painter().text(
                        stop_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Остановить показ",
                        semibold_font(11.8),
                        Color32::WHITE,
                    );
                    if stop_resp.clicked() {
                        toggle_share = true;
                    }
                }
            });

        if exit_fs {
            self.screen_fullscreen = false;
        }
        if toggle_share {
            self.toggle_screen_share(ctx);
        }
    }

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
                let mut unique_participants: Vec<&String> = Vec::new();
                for p in participants {
                    if !unique_participants.iter().any(|u| u.eq_ignore_ascii_case(p)) {
                        unique_participants.push(p);
                    }
                }
                let cnt = unique_participants.len().max(1).min(MAX_CALL_PARTICIPANTS);
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
        let is_sharing_screen = self.screen_engine.is_some();
        let has_active_stream = self.screen_tex.is_some() && self.screen_bridge.is_stream_alive();
        let mut vol_pct = (self.audio_bridge.speaker_volume_f32() * 100.0).round() as i32;
        let mut mic_pct = (self.audio_bridge.mic_gain_f32() * 100.0).round() as i32;

        let mut do_minimize = false;
        let mut do_accept = false;
        let mut do_hangup = false;
        let mut do_toggle_mic = false;
        let mut do_toggle_spk = false;
        let mut do_toggle_screen = false;
        let mut do_enter_fullscreen = false;

        let win_w = if has_active_stream && self.screen_watching {
            500.0
        } else {
            390.0
        };
        let win_h = if is_incoming {
            375.0
        } else if has_active_stream && self.screen_watching {
            650.0
        } else if has_active_stream {
            545.0
        } else {
            480.0
        };

        egui::Window::new("egramm_tg_call_window")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_pos(Pos2::new(
                (ctx.screen_rect().center().x - win_w * 0.5).max(40.0),
                (ctx.screen_rect().center().y - win_h * 0.5).max(24.0),
            ))
            .fixed_size(Vec2::new(win_w, win_h))
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

                        if has_active_stream {
                            ui.add_space(10.0);
                            let streamer_title = if self.screen_streamer_name.is_empty() {
                                peer_name.clone()
                            } else {
                                self.screen_streamer_name.clone()
                            };

                            egui::Frame::none()
                                .fill(Color32::from_rgb(22, 34, 48))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(52, 82, 118)))
                                .rounding(Rounding::same(14.0))
                                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (v_ic, _) = ui.allocate_exact_size(
                                            Vec2::splat(22.0),
                                            egui::Sense::hover(),
                                        );
                                        draw_video_play_icon(
                                            ui.painter(),
                                            v_ic.center(),
                                            Color32::from_rgb(122, 194, 255),
                                        );

                                        let (live_r, _) = ui.allocate_exact_size(
                                            Vec2::new(38.0, 18.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            live_r,
                                            Rounding::same(5.0),
                                            DANGER_RED,
                                        );
                                        ui.painter().text(
                                            live_r.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "LIVE",
                                            semibold_font(9.8),
                                            Color32::WHITE,
                                        );

                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Экран: {}",
                                                streamer_title
                                            ))
                                                .font(semibold_font(13.0))
                                                .color(Color32::WHITE),
                                        );

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if self.screen_watching {
                                                    let (fs_r, fs_resp) = ui.allocate_exact_size(
                                                        Vec2::splat(26.0),
                                                        egui::Sense::click(),
                                                    );
                                                    if fs_resp.hovered() {
                                                        ui.painter().circle_filled(
                                                            fs_r.center(),
                                                            13.0,
                                                            Color32::from_rgb(38, 56, 78),
                                                        );
                                                    }
                                                    draw_fullscreen_icon(
                                                        ui.painter(),
                                                        fs_r.center(),
                                                        Color32::WHITE,
                                                    );
                                                    if fs_resp
                                                        .on_hover_text("На весь экран")
                                                        .clicked()
                                                    {
                                                        do_enter_fullscreen = true;
                                                    }
                                                }

                                                let watch_lbl = if self.screen_watching {
                                                    "Свернуть"
                                                } else {
                                                    "Смотреть"
                                                };
                                                if ui
                                                    .add_sized(
                                                        Vec2::new(82.0, 24.0),
                                                        egui::Button::new(
                                                            egui::RichText::new(watch_lbl)
                                                                .font(semibold_font(11.8))
                                                                .color(Color32::WHITE),
                                                        )
                                                            .fill(if self.screen_watching {
                                                                Color32::from_rgb(42, 60, 82)
                                                            } else {
                                                                AUTH_BLUE
                                                            })
                                                            .rounding(12.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.screen_watching = !self.screen_watching;
                                                }
                                            },
                                        );
                                    });

                                    if self.screen_watching {
                                        if let Some(ref tex) = self.screen_tex {
                                            ui.add_space(8.0);
                                            let max_preview_w = ui.available_width();
                                            let max_preview_h = 230.0;
                                            let orig = tex.size_vec2();
                                            let sc = (max_preview_w / orig.x.max(1.0))
                                                .min(max_preview_h / orig.y.max(1.0));
                                            let disp = orig * sc;
                                            ui.vertical_centered(|ui| {
                                                let img_resp = ui.add(
                                                    egui::Image::from_texture(
                                                        egui::load::SizedTexture::new(
                                                            tex.id(),
                                                            disp,
                                                        ),
                                                    )
                                                        .rounding(Rounding::same(10.0))
                                                        .sense(egui::Sense::click()),
                                                );
                                                if img_resp
                                                    .on_hover_text(
                                                        "Нажмите, чтобы открыть на весь экран",
                                                    )
                                                    .clicked()
                                                {
                                                    do_enter_fullscreen = true;
                                                }
                                            });
                                        }
                                    }
                                });
                        }

                        if !(has_active_stream && self.screen_watching) {
                            ui.add_space(12.0);
                            ui.vertical_centered(|ui| {
                                let (av_box, _) = ui
                                    .allocate_exact_size(Vec2::splat(126.0), egui::Sense::hover());
                                let c = av_box.center();
                                let p = ui.painter();

                                let base_r = 44.0;
                                let ring_col = if is_connected || is_incoming {
                                    ONLINE_GREEN
                                } else {
                                    AUTH_BLUE
                                };

                                if is_connected {
                                    let voice_boost = remote_lvl * 20.0;
                                    p.circle_filled(
                                        c,
                                        base_r + 13.0 + voice_boost,
                                        with_alpha(ring_col, 0.14),
                                    );
                                    p.circle_filled(
                                        c,
                                        base_r + 6.5 + voice_boost * 0.55,
                                        with_alpha(ring_col, 0.24),
                                    );
                                } else {
                                    let wave1 = (pulse_t * 1.4).fract();
                                    let wave2 = (pulse_t * 1.4 + 0.5).fract();
                                    p.circle_stroke(
                                        c,
                                        base_r + wave1 * 22.0,
                                        Stroke::new(2.0, with_alpha(ring_col, (1.0 - wave1) * 0.55)),
                                    );
                                    p.circle_stroke(
                                        c,
                                        base_r + wave2 * 22.0,
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
                                    28.0,
                                );

                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(&peer_name)
                                        .font(semibold_font(20.0))
                                        .color(Color32::WHITE),
                                );
                                ui.add_space(3.0);
                                ui.label(
                                    egui::RichText::new(&status_line)
                                        .font(semibold_font(12.8))
                                        .color(status_color),
                                );
                            });
                        } else {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&peer_name)
                                        .font(semibold_font(15.0))
                                        .color(Color32::WHITE),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(&status_line)
                                                .font(semibold_font(12.5))
                                                .color(status_color),
                                        );
                                    },
                                );
                            });
                        }

                        if !is_incoming {
                            ui.add_space(12.0);
                            egui::Frame::none()
                                .fill(Color32::from_rgb(24, 36, 51))
                                .rounding(Rounding::same(14.0))
                                .inner_margin(egui::Margin::symmetric(14.0, 9.0))
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

                                    ui.add_space(5.0);

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

                                    ui.add_space(6.0);
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

                        ui.add_space(16.0);

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
                            ui.columns(4, |cols| {
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

                                let scr_bg = if is_sharing_screen {
                                    ONLINE_GREEN
                                } else {
                                    Color32::from_rgb(35, 50, 70)
                                };
                                let scr_hover = if is_sharing_screen {
                                    Color32::from_rgb(88, 214, 138)
                                } else {
                                    Color32::from_rgb(48, 66, 92)
                                };
                                if draw_call_circle_btn(
                                    &mut cols[2],
                                    scr_bg,
                                    scr_hover,
                                    if is_sharing_screen { "Стоп" } else { "Экран" },
                                    |p, c| {
                                        draw_screen_share_icon(
                                            p,
                                            c,
                                            is_sharing_screen,
                                            Color32::WHITE,
                                        )
                                    },
                                ) {
                                    do_toggle_screen = true;
                                }

                                if draw_call_circle_btn(
                                    &mut cols[3],
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
        if do_toggle_screen {
            self.toggle_screen_share(ctx);
        }
        if do_enter_fullscreen {
            self.screen_fullscreen = true;
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
                        self.screen_bridge.set_my_email(email.clone());
                        let nick = email.split('@').next().unwrap_or("user");
                        self.user_tag = format!("@{}", nick);
                        self.session_token = Some(token.clone());
                        save_session(&SavedSession {
                            name: name.clone(),
                            email: email.clone(),
                            session_token: token,
                            avatar_file_id: self.user_avatar_id.clone(),
                            bio: self.user_bio.clone(),
                        });
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
                        if email.eq_ignore_ascii_case(&self.user_email) {
                            continue;
                        }
                        for chat in &mut self.chats {
                            if !chat.is_group
                                && chat
                                .peer_email
                                .as_deref()
                                .map(|pe| pe.eq_ignore_ascii_case(&email))
                                .unwrap_or(false)
                            {
                                chat.title = name.clone();
                                chat.avatar_tag =
                                    name.chars().take(2).collect::<String>().to_uppercase();
                                chat.peer_bio = bio.clone();
                            }
                            for m in &mut chat.members {
                                if m.email
                                    .as_deref()
                                    .map(|me| me.eq_ignore_ascii_case(&email))
                                    .unwrap_or(false)
                                {
                                    m.name = name.clone();
                                    m.tag = tag.clone();
                                    m.bio = bio.clone();
                                }
                            }
                        }
                        for u in &mut self.user_search_results {
                            if u.email.eq_ignore_ascii_case(&email) {
                                u.name = name.clone();
                                u.tag = tag.clone();
                                u.bio = bio.clone();
                            }
                        }
                        if let Some(vp) = &mut self.viewed_profile {
                            if vp.email.eq_ignore_ascii_case(&email) {
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
                        if email.eq_ignore_ascii_case(&self.user_email) {
                            self.user_avatar_id = avatar_file_id.clone();
                            if avatar_file_id.is_none() {
                                self.user_avatar = None;
                            }
                        }
                        for chat in &mut self.chats {
                            if chat
                                .peer_email
                                .as_deref()
                                .map(|pe| pe.eq_ignore_ascii_case(&email))
                                .unwrap_or(false)
                            {
                                chat.peer_avatar_id = avatar_file_id.clone();
                                if avatar_file_id.is_none() {
                                    chat.avatar_tex = None;
                                }
                            }
                            for m in &mut chat.members {
                                if m.email
                                    .as_deref()
                                    .map(|me| me.eq_ignore_ascii_case(&email))
                                    .unwrap_or(false)
                                {
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
                        if !allowed_emails
                            .iter()
                            .any(|em| em.eq_ignore_ascii_case(&self.user_email))
                        {
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
                    NetEvent::ChatDeleted(deleted_chat_id) => {
                        if let Some(del_pos) = self
                            .chats
                            .iter()
                            .position(|c| c.server_chat_id == Some(deleted_chat_id))
                        {
                            self.chats.remove(del_pos);
                            if self.selected_chat == Some(del_pos) {
                                self.selected_chat = if self.chats.is_empty() {
                                    None
                                } else {
                                    Some(0)
                                };
                            } else if let Some(sel) = self.selected_chat {
                                if sel > del_pos {
                                    self.selected_chat = Some(sel - 1);
                                }
                            }
                            if matches!(self.active_modal, ActiveModal::ManageGroup(idx) if idx == del_pos)
                            {
                                self.active_modal = ActiveModal::None;
                                self.editing_admin_idx = None;
                            } else if let ActiveModal::ManageGroup(ref mut idx) = self.active_modal {
                                if *idx > del_pos {
                                    *idx -= 1;
                                }
                            }
                        }
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
                        if !reader_email.eq_ignore_ascii_case(&self.user_email) {
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
                            if let CallStatus::Connected {
                                chat_id,
                                call_id,
                                ..
                            } = self.call_status.clone()
                            {
                                if let (Some(token), Some(ws_tx)) =
                                    (self.session_token.clone(), self.ws_out_tx.clone())
                                {
                                    let rejoin = WsWireEvent::CallAccept {
                                        session_token: token.clone(),
                                        chat_id,
                                        call_id: call_id.clone(),
                                        peer_email: self.user_email.clone(),
                                        participants: Vec::new(),
                                    };
                                    if let Ok(json) = serde_json::to_string(&rejoin) {
                                        let _ = ws_tx.send(json);
                                    }
                                    if let Some(old_engine) = self.voice_engine.take() {
                                        old_engine.stop();
                                    }
                                    match VoiceEngine::start(
                                        ws_tx,
                                        token,
                                        chat_id,
                                        call_id,
                                        self.user_email.clone(),
                                        self.audio_bridge.clone(),
                                    ) {
                                        Ok(new_engine) => {
                                            self.voice_engine = Some(new_engine);
                                        }
                                        Err(e) => {
                                            self.show_toast(format!("Перезапуск аудио: {}", e));
                                        }
                                    }
                                }
                            }
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
                    NetEvent::ReactionUpdated {
                        chat_id,
                        message_id,
                        reactions,
                    } => {
                        if let Some(chat) = self
                            .chats
                            .iter_mut()
                            .find(|c| c.server_chat_id == Some(chat_id))
                        {
                            if let Some(msg) = chat
                                .messages
                                .iter_mut()
                                .find(|m| m.server_id == Some(message_id))
                            {
                                msg.reactions = reactions;
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
                        if caller_email.eq_ignore_ascii_case(&self.user_email) {
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
                        if peer_email.eq_ignore_ascii_case(&self.user_email) {
                            continue;
                        }

                        let mut updated_parts = if participants.is_empty() {
                            vec![self.user_email.clone(), peer_email.clone()]
                        } else {
                            participants
                        };
                        if !self.user_email.is_empty()
                            && !updated_parts
                            .iter()
                            .any(|u| u.eq_ignore_ascii_case(&self.user_email))
                        {
                            updated_parts.insert(0, self.user_email.clone());
                        }
                        if !peer_email.is_empty()
                            && !updated_parts
                            .iter()
                            .any(|u| u.eq_ignore_ascii_case(&peer_email))
                        {
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
                        if from_email.eq_ignore_ascii_case(&self.user_email) {
                            continue;
                        }
                        self.audio_bridge.remove_sender(&from_email);
                        self.screen_bridge.clear_sender(&from_email);
                        if self.screen_streamer_email.eq_ignore_ascii_case(&from_email) {
                            self.clear_remote_screen_state();
                        }

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
                                    participants
                                        .retain(|em| !em.eq_ignore_ascii_case(&from_email));
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
                    NetEvent::CallScreenStop {
                        chat_id,
                        sender_email,
                    } => {
                        let active_cid = match &self.call_status {
                            CallStatus::Outgoing { chat_id: c, .. }
                            | CallStatus::Incoming { chat_id: c, .. }
                            | CallStatus::Connected { chat_id: c, .. } => Some(*c),
                            CallStatus::Idle => None,
                        };
                        if active_cid == Some(chat_id) {
                            self.screen_bridge.clear_sender(&sender_email);
                            if self.screen_streamer_email.eq_ignore_ascii_case(&sender_email)
                                || sender_email.is_empty()
                            {
                                self.clear_remote_screen_state();
                            }
                        }
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

            self.sync_screen_texture(ctx);

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
                if self.screen_fullscreen {
                    self.screen_fullscreen = false;
                } else if self.intro_start.is_some() {
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
                ctx.request_repaint_after(Duration::from_millis(120));
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
                .frame(
                    egui::Frame::none()
                        .fill(pal.bg_panel)
                        .stroke(Stroke::new(1.0, pal.border_line)),
                )
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(10.0, 10.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let menu_btn = ui.add_sized(
                                    Vec2::splat(34.0),
                                    egui::Button::new(
                                        egui::RichText::new("☰").size(17.0).color(pal.text_muted),
                                    )
                                        .fill(Color32::TRANSPARENT)
                                        .rounding(17.0),
                                );
                                if menu_btn.on_hover_text("Меню Egramm").clicked() {
                                    self.menu_open = !self.menu_open;
                                    self.attach_menu_open = false;
                                    self.msg_context_menu = None;
                                }

                                let search_margin_x = 10.0;
                                let search_w =
                                    (ui.available_width() - search_margin_x * 2.0 - 2.0).max(80.0);
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
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 64.0),
                                egui::Sense::click(),
                            );
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
                            draw_avatar_circle(
                                painter,
                                avatar_rect,
                                &chat.avatar_tex,
                                &chat.avatar_tag,
                                chat.avatar_color,
                                14.0,
                            );

                            let title_color = if is_selected {
                                pal.text_on_selected
                            } else {
                                pal.text_main
                            };
                            let sub_color = if is_selected {
                                with_alpha(pal.text_on_selected, 0.85)
                            } else {
                                pal.text_muted
                            };

                            let text_x = rect.min.x + 68.0;
                            painter.text(
                                Pos2::new(text_x, rect.min.y + 13.0),
                                egui::Align2::LEFT_TOP,
                                &chat.title,
                                semibold_font(14.5),
                                title_color,
                            );

                            let last_time = chat.messages.last().map(|m| m.time.as_str()).unwrap_or("");
                            painter.text(
                                Pos2::new(rect.max.x - 12.0, rect.min.y + 14.0),
                                egui::Align2::RIGHT_TOP,
                                last_time,
                                FontId::proportional(11.5),
                                sub_color,
                            );

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
                            painter.text(
                                Pos2::new(text_x, rect.min.y + 36.0),
                                egui::Align2::LEFT_TOP,
                                preview_text,
                                FontId::proportional(13.0),
                                sub_color,
                            );

                            if chat.unread > 0 {
                                let badge_center = Pos2::new(rect.max.x - 22.0, rect.min.y + 43.0);
                                painter.circle_filled(
                                    badge_center,
                                    10.5,
                                    if is_selected {
                                        Color32::WHITE
                                    } else {
                                        pal.accent_blue
                                    },
                                );
                                painter.text(
                                    badge_center,
                                    egui::Align2::CENTER_CENTER,
                                    chat.unread.to_string(),
                                    semibold_font(11.0),
                                    if is_selected {
                                        pal.bg_selected
                                    } else {
                                        Color32::WHITE
                                    },
                                );
                            }

                            if response.clicked() {
                                chat.unread = 0;
                                clicked_chat = Some(idx);
                            }
                        }

                        if !self.search_query.trim().is_empty()
                            && !self.group_search_results.is_empty()
                        {
                            ui.add_space(8.0);
                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(14.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("Группы по коду / названию")
                                            .font(semibold_font(12.0))
                                            .color(pal.accent_light),
                                    );
                                });

                            for (g_i, grp) in self.group_search_results.iter().enumerate() {
                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 58.0),
                                    egui::Sense::click(),
                                );
                                let p = ui.painter();
                                if resp.hovered() {
                                    p.rect_filled(rect, Rounding::ZERO, pal.bg_hover);
                                }
                                let av_rect = Rect::from_center_size(
                                    Pos2::new(rect.min.x + 34.0, rect.center().y),
                                    Vec2::splat(40.0),
                                );
                                let g_tex = grp
                                    .avatar_file_id
                                    .as_ref()
                                    .and_then(|fid| self.avatar_tex_cache.get(fid).cloned());
                                draw_avatar_circle(
                                    p,
                                    av_rect,
                                    &g_tex,
                                    &grp.avatar_tag,
                                    GROUP_COLORS[(g_i + 1) % GROUP_COLORS.len()],
                                    13.5,
                                );
                                let text_x = rect.min.x + 66.0;
                                let code_lbl = if grp.invite_code.is_empty() {
                                    format!("EGR-{:06}", grp.id.abs() % 1_000_000)
                                } else {
                                    grp.invite_code.clone()
                                };
                                p.text(
                                    Pos2::new(text_x, rect.min.y + 11.0),
                                    egui::Align2::LEFT_TOP,
                                    &grp.title,
                                    semibold_font(14.0),
                                    pal.text_main,
                                );
                                p.text(
                                    Pos2::new(text_x, rect.min.y + 32.0),
                                    egui::Align2::LEFT_TOP,
                                    format!("{} • {}", code_lbl, grp.subtitle),
                                    FontId::proportional(12.0),
                                    pal.text_muted,
                                );
                                if resp.clicked() {
                                    join_found_group = Some(grp.clone());
                                }
                            }
                        }

                        if !self.search_query.trim().is_empty()
                            && !self.user_search_results.is_empty()
                        {
                            ui.add_space(8.0);
                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(14.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("Люди в Egramm")
                                            .font(semibold_font(12.0))
                                            .color(pal.accent_light),
                                    );
                                });

                            for (u_idx, user) in self.user_search_results.iter().enumerate() {
                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 58.0),
                                    egui::Sense::click(),
                                );
                                let p = ui.painter();
                                if resp.hovered() {
                                    p.rect_filled(rect, Rounding::ZERO, pal.bg_hover);
                                }
                                let av_rect = Rect::from_center_size(
                                    Pos2::new(rect.min.x + 34.0, rect.center().y),
                                    Vec2::splat(40.0),
                                );
                                let init: String =
                                    user.name.chars().take(2).collect::<String>().to_uppercase();
                                let u_tex = user
                                    .avatar_file_id
                                    .as_ref()
                                    .and_then(|fid| self.avatar_tex_cache.get(fid).cloned());
                                draw_avatar_circle(
                                    p,
                                    av_rect,
                                    &u_tex,
                                    &init,
                                    GROUP_COLORS[(u_idx + 2) % GROUP_COLORS.len()],
                                    13.5,
                                );
                                let text_x = rect.min.x + 66.0;
                                p.text(
                                    Pos2::new(text_x, rect.min.y + 11.0),
                                    egui::Align2::LEFT_TOP,
                                    &user.name,
                                    semibold_font(14.0),
                                    pal.text_main,
                                );
                                let st = if user.online { "в сети" } else { "не в сети" };
                                p.text(
                                    Pos2::new(text_x, rect.min.y + 32.0),
                                    egui::Align2::LEFT_TOP,
                                    format!("{} • {}", user.tag, st),
                                    FontId::proportional(12.0),
                                    pal.text_muted,
                                );
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
                            CallStatus::Connected { .. } | CallStatus::Incoming { .. } => {
                                Color32::from_rgb(35, 134, 78)
                            }
                            _ => Color32::from_rgb(34, 96, 174),
                        };
                        egui::TopBottomPanel::top("tg_minimized_call_bar")
                            .frame(
                                egui::Frame::none()
                                    .fill(banner_col)
                                    .inner_margin(egui::Margin::symmetric(14.0, 6.0)),
                            )
                            .show_inside(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let title = match &self.call_status {
                                        CallStatus::Outgoing { peer_name, .. } => {
                                            format!("Вызов: {}...  (нажмите, чтобы открыть)", peer_name)
                                        }
                                        CallStatus::Incoming { caller_name, .. } => {
                                            format!("Входящий звонок: {}", caller_name)
                                        }
                                        CallStatus::Connected {
                                            peer_name,
                                            started_at,
                                            participants,
                                            ..
                                        } => {
                                            let s = started_at.elapsed().as_secs();
                                            let mut uniq: Vec<&String> = Vec::new();
                                            for p in participants {
                                                if !uniq.iter().any(|u| u.eq_ignore_ascii_case(p)) {
                                                    uniq.push(p);
                                                }
                                            }
                                            let cnt = uniq.len().max(1).min(MAX_CALL_PARTICIPANTS);
                                            format!(
                                                "В звонке: {} ({}/{})  •  {:02}:{:02}  (открыть)",
                                                peer_name,
                                                cnt,
                                                MAX_CALL_PARTICIPANTS,
                                                s / 60,
                                                s % 60
                                            )
                                        }
                                        CallStatus::Idle => String::new(),
                                    };
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(title)
                                                    .font(semibold_font(12.8))
                                                    .color(Color32::WHITE),
                                            )
                                                .fill(Color32::TRANSPARENT),
                                        )
                                        .clicked()
                                    {
                                        expand_call_window_now = true;
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_sized(
                                                    Vec2::new(86.0, 24.0),
                                                    egui::Button::new(
                                                        egui::RichText::new("Сбросить")
                                                            .size(12.0)
                                                            .color(Color32::WHITE),
                                                    )
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
                                                        egui::Button::new(
                                                            egui::RichText::new("Принять")
                                                                .size(12.0)
                                                                .color(Color32::WHITE),
                                                        )
                                                            .fill(Color32::from_rgb(24, 102, 58))
                                                            .rounding(12.0),
                                                    )
                                                    .clicked()
                                                {
                                                    accept_call_now = true;
                                                }
                                            } else {
                                                let muted =
                                                    self.audio_bridge.mic_muted.load(Ordering::Relaxed);
                                                if ui
                                                    .add_sized(
                                                        Vec2::new(88.0, 24.0),
                                                        egui::Button::new(
                                                            egui::RichText::new(if muted {
                                                                "Мик. выкл"
                                                            } else {
                                                                "Мик. вкл"
                                                            })
                                                                .size(12.0)
                                                                .color(Color32::WHITE),
                                                        )
                                                            .fill(Color32::from_rgba_unmultiplied(
                                                                0, 0, 0, 55,
                                                            ))
                                                            .rounding(12.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.audio_bridge
                                                        .mic_muted
                                                        .store(!muted, Ordering::Relaxed);
                                                }
                                            }
                                        },
                                    );
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
                                        let (av_rect, av_resp) =
                                            ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::click());
                                        draw_avatar_circle(
                                            ui.painter(),
                                            av_rect,
                                            &chat.avatar_tex,
                                            &chat.avatar_tag,
                                            chat.avatar_color,
                                            12.5,
                                        );
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
                                                ui.label(
                                                    egui::RichText::new(&chat.title)
                                                        .font(semibold_font(15.5))
                                                        .color(pal.text_main),
                                                );
                                                if chat.server_chat_id.is_some() {
                                                    let (sub_text, sub_col) = if !self.ws_connected {
                                                        ("подключение...".into(), pal.text_muted)
                                                    } else if chat.is_group {
                                                        (
                                                            format!("{} • {}", chat.invite_code, chat.subtitle),
                                                            pal.text_muted,
                                                        )
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
                                                    ui.label(
                                                        egui::RichText::new(sub_text)
                                                            .size(12.2)
                                                            .color(sub_col),
                                                    );
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

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if is_group {
                                                    let (s_rect, s_resp) = ui.allocate_exact_size(
                                                        Vec2::splat(34.0),
                                                        egui::Sense::click(),
                                                    );
                                                    if s_resp.hovered() {
                                                        ui.painter().circle_filled(
                                                            s_rect.center(),
                                                            17.0,
                                                            pal.bg_hover,
                                                        );
                                                    }
                                                    draw_sliders_icon(
                                                        ui.painter(),
                                                        s_rect.center(),
                                                        if s_resp.hovered() {
                                                            pal.accent_light
                                                        } else {
                                                            pal.text_muted
                                                        },
                                                    );
                                                    if s_resp
                                                        .on_hover_text("Управление группой")
                                                        .clicked()
                                                    {
                                                        open_group_manage = Some(chat_idx);
                                                    }
                                                }

                                                if !is_saved {
                                                    let (c_rect, c_resp) = ui.allocate_exact_size(
                                                        Vec2::splat(34.0),
                                                        egui::Sense::click(),
                                                    );
                                                    if in_call {
                                                        ui.painter().circle_filled(
                                                            c_rect.center(),
                                                            17.0,
                                                            Color32::from_rgba_unmultiplied(
                                                                68, 198, 122, 48,
                                                            ),
                                                        );
                                                    } else if c_resp.hovered() {
                                                        ui.painter().circle_filled(
                                                            c_rect.center(),
                                                            17.0,
                                                            pal.bg_hover,
                                                        );
                                                    }
                                                    draw_phone_icon(
                                                        ui.painter(),
                                                        c_rect.center(),
                                                        if in_call {
                                                            ONLINE_GREEN
                                                        } else if c_resp.hovered() {
                                                            pal.accent_light
                                                        } else {
                                                            pal.text_muted
                                                        },
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
                                            },
                                        );
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
                                if let Some(rep) = self.reply_target.clone() {
                                    let mut cancel_reply = false;
                                    egui::Frame::none()
                                        .fill(pal.bg_input)
                                        .rounding(Rounding::same(10.0))
                                        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let (ic_r, _) = ui.allocate_exact_size(
                                                    Vec2::splat(20.0),
                                                    egui::Sense::hover(),
                                                );
                                                draw_reply_icon(
                                                    ui.painter(),
                                                    ic_r.center(),
                                                    pal.accent_light,
                                                );
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "В ответ {}",
                                                            rep.sender
                                                        ))
                                                            .font(semibold_font(12.0))
                                                            .color(pal.accent_light),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(&rep.preview)
                                                            .size(12.0)
                                                            .color(pal.text_muted),
                                                    );
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        if ui
                                                            .add(
                                                                egui::Button::new(
                                                                    egui::RichText::new("✕")
                                                                        .color(pal.text_muted),
                                                                )
                                                                    .fill(Color32::TRANSPARENT)
                                                                    .rounding(12.0),
                                                            )
                                                            .clicked()
                                                        {
                                                            cancel_reply = true;
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    if cancel_reply {
                                        self.reply_target = None;
                                    }
                                    ui.add_space(6.0);
                                }

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
                                                        ui.label(
                                                            egui::RichText::new(att.kind.tag())
                                                                .font(semibold_font(10.5))
                                                                .color(Color32::WHITE),
                                                        );
                                                    });
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(&att.title)
                                                            .font(semibold_font(13.0))
                                                            .color(pal.text_main),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(&att.subtitle)
                                                            .size(11.5)
                                                            .color(pal.text_muted),
                                                    );
                                                });
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        if ui
                                                            .add(
                                                                egui::Button::new(
                                                                    egui::RichText::new("✕")
                                                                        .color(pal.text_muted),
                                                                )
                                                                    .fill(Color32::TRANSPARENT)
                                                                    .rounding(12.0),
                                                            )
                                                            .clicked()
                                                        {
                                                            remove_att = true;
                                                        }
                                                    },
                                                );
                                            });
                                        });
                                    if remove_att {
                                        self.pending_attachment = None;
                                    }
                                    ui.add_space(6.0);
                                }

                                ui.horizontal(|ui| {
                                    let clip_resp =
                                        draw_paperclip_button(ui, self.attach_menu_open, &pal);
                                    self.attach_btn_rect = clip_resp.rect;
                                    if clip_resp.on_hover_text("Прикрепить файл").clicked() {
                                        self.attach_menu_open = !self.attach_menu_open;
                                        self.msg_context_menu = None;
                                    }

                                    let send_w = 36.0;
                                    let margin_x = 12.0;
                                    let gap = ui.spacing().item_spacing.x;
                                    let input_width = (ui.available_width()
                                        - send_w
                                        - gap
                                        - margin_x * 2.0
                                        - 4.0)
                                        .max(80.0);

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

                                    if text_resp.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    {
                                        send_now = true;
                                        text_resp.request_focus();
                                    }

                                    let is_ready = !self.input_text.trim().is_empty()
                                        || self.pending_attachment.is_some();
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
                            .frame(
                                egui::Frame::none()
                                    .fill(pal.bg_chat)
                                    .inner_margin(egui::Margin::symmetric(16.0, 12.0)),
                            )
                            .show_inside(ui, |ui| {
                                let ease_t = ease_out_cubic(switch_elapsed / 0.18);
                                let chat_alpha = ease_t;
                                ui.add_space((1.0 - ease_t) * 10.0);

                                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                                    if let Some(chat) = self.chats.get(chat_idx) {
                                        for (msg_idx, msg) in chat.messages.iter().enumerate() {
                                            let msg_ease = ease_out_cubic(
                                                msg.created_at.elapsed().as_secs_f32() / 0.18,
                                            );
                                            let alpha = chat_alpha * msg_ease;

                                            let align = if msg.is_me {
                                                egui::Layout::right_to_left(egui::Align::TOP)
                                            } else {
                                                egui::Layout::left_to_right(egui::Align::TOP)
                                            };

                                            let mut bubble_rect = Rect::NOTHING;

                                            ui.with_layout(align, |ui| {
                                                let bg = with_alpha(
                                                    if msg.is_me {
                                                        pal.bubble_out
                                                    } else {
                                                        pal.bubble_in
                                                    },
                                                    alpha,
                                                );
                                                let bubble = egui::Frame::none()
                                                    .fill(bg)
                                                    .rounding(Rounding::same(14.0))
                                                    .inner_margin(egui::Margin::symmetric(13.0, 8.0))
                                                    .show(ui, |ui| {
                                                        ui.set_max_width(430.0);
                                                        ui.with_layout(
                                                            egui::Layout::top_down(egui::Align::LEFT),
                                                            |ui| {
                                                                if !msg.is_me {
                                                                    let sender_lbl = ui.add(
                                                                        egui::Label::new(
                                                                            egui::RichText::new(
                                                                                &msg.sender,
                                                                            )
                                                                                .font(semibold_font(13.0))
                                                                                .color(with_alpha(
                                                                                    pal.accent_light,
                                                                                    alpha,
                                                                                )),
                                                                        )
                                                                            .sense(egui::Sense::click()),
                                                                    );
                                                                    if sender_lbl
                                                                        .on_hover_text("Открыть профиль")
                                                                        .clicked()
                                                                        && !chat.is_group
                                                                    {
                                                                        open_peer_profile_for =
                                                                            Some(chat_idx);
                                                                    }
                                                                }

                                                                if let Some(ref r_prev) =
                                                                    msg.reply_to_preview
                                                                {
                                                                    let r_sender = msg
                                                                        .reply_to_sender
                                                                        .as_deref()
                                                                        .unwrap_or("Сообщение");
                                                                    draw_reply_quote_bar(
                                                                        ui,
                                                                        r_sender,
                                                                        r_prev,
                                                                        &pal,
                                                                        alpha,
                                                                    );
                                                                    ui.add_space(5.0);
                                                                }

                                                                if let Some(att) = &msg.attachment {
                                                                    let img_tex = att
                                                                        .local_path
                                                                        .as_ref()
                                                                        .filter(|p| {
                                                                            p.to_str()
                                                                                .map(is_image_name)
                                                                                .unwrap_or(false)
                                                                        })
                                                                        .and_then(|p| {
                                                                            get_or_load_image(
                                                                                ctx,
                                                                                &mut self.image_cache,
                                                                                p,
                                                                            )
                                                                        });

                                                                    if let Some(tex) = img_tex {
                                                                        let orig = tex.size_vec2();
                                                                        let scale = (340.0_f32 / orig.x)
                                                                            .min(280.0_f32 / orig.y)
                                                                            .min(1.0);
                                                                        let disp = orig * scale;
                                                                        let img_resp = ui.add(
                                                                            egui::Image::from_texture(
                                                                                egui::load::SizedTexture::new(
                                                                                    tex.id(),
                                                                                    disp,
                                                                                ),
                                                                            )
                                                                                .rounding(Rounding::same(
                                                                                    10.0,
                                                                                ))
                                                                                .tint(with_alpha(
                                                                                    Color32::WHITE,
                                                                                    alpha,
                                                                                ))
                                                                                .sense(egui::Sense::click()),
                                                                        );
                                                                        if img_resp
                                                                            .on_hover_text(
                                                                                "Нажмите, чтобы приблизить фото",
                                                                            )
                                                                            .clicked()
                                                                        {
                                                                            if let Some(p) =
                                                                                &att.local_path
                                                                            {
                                                                                opened_lightbox =
                                                                                    Some(p.clone());
                                                                            }
                                                                        }
                                                                    } else {
                                                                        let card_resp = egui::Frame::none()
                                                                            .fill(with_alpha(
                                                                                pal.card_inner,
                                                                                alpha,
                                                                            ))
                                                                            .rounding(Rounding::same(
                                                                                10.0,
                                                                            ))
                                                                            .inner_margin(
                                                                                egui::Margin::symmetric(
                                                                                    10.0, 8.0,
                                                                                ),
                                                                            )
                                                                            .show(ui, |ui| {
                                                                                ui.horizontal(|ui| {
                                                                                    let (ic_rect, _) =
                                                                                        ui.allocate_exact_size(
                                                                                            Vec2::splat(34.0),
                                                                                            egui::Sense::hover(),
                                                                                        );
                                                                                    ui.painter().circle_filled(
                                                                                        ic_rect.center(),
                                                                                        17.0,
                                                                                        with_alpha(
                                                                                            att.kind.badge_color(),
                                                                                            alpha,
                                                                                        ),
                                                                                    );
                                                                                    draw_menu_icon(
                                                                                        ui.painter(),
                                                                                        ic_rect.center(),
                                                                                        att.kind,
                                                                                        with_alpha(
                                                                                            Color32::WHITE,
                                                                                            alpha,
                                                                                        ),
                                                                                    );
                                                                                    ui.add_space(4.0);
                                                                                    ui.vertical(|ui| {
                                                                                        ui.label(
                                                                                            egui::RichText::new(&att.title)
                                                                                                .font(semibold_font(13.5))
                                                                                                .color(with_alpha(pal.text_main, alpha)),
                                                                                        );
                                                                                        ui.label(
                                                                                            egui::RichText::new(&att.subtitle)
                                                                                                .size(11.5)
                                                                                                .color(with_alpha(pal.accent_light, alpha)),
                                                                                        );
                                                                                    });
                                                                                });
                                                                            })
                                                                            .response
                                                                            .interact(egui::Sense::click());

                                                                        if card_resp.clicked() {
                                                                            if let Some(p) =
                                                                                &att.local_path
                                                                            {
                                                                                let _ = open::that(p);
                                                                            } else if let Some(id) =
                                                                                &att.drive_file_id
                                                                            {
                                                                                drive_open_request =
                                                                                    Some((
                                                                                        id.clone(),
                                                                                        att.title
                                                                                            .clone(),
                                                                                    ));
                                                                            }
                                                                        }
                                                                    }

                                                                    if !msg.text.is_empty() {
                                                                        ui.add_space(6.0);
                                                                    }
                                                                }

                                                                if !msg.text.is_empty() {
                                                                    ui.label(
                                                                        egui::RichText::new(&msg.text)
                                                                            .font(FontId::proportional(
                                                                                14.5,
                                                                            ))
                                                                            .color(with_alpha(
                                                                                pal.text_main,
                                                                                alpha,
                                                                            )),
                                                                    );
                                                                }

                                                                ui.add_space(3.0);
                                                                ui.horizontal(|ui| {
                                                                    if !msg.reactions.is_empty() {
                                                                        ui.spacing_mut().item_spacing =
                                                                            egui::vec2(5.0, 4.0);
                                                                        for (r_i, rxn) in
                                                                            msg.reactions.iter().enumerate()
                                                                        {
                                                                            let mine = rxn.users.iter().any(|u| {
                                                                                u.eq_ignore_ascii_case(
                                                                                    &self.user_email,
                                                                                )
                                                                            });
                                                                            let fill = if mine {
                                                                                with_alpha(
                                                                                    pal.accent_light,
                                                                                    alpha.max(0.82),
                                                                                )
                                                                            } else {
                                                                                with_alpha(
                                                                                    pal.card_inner,
                                                                                    alpha,
                                                                                )
                                                                            };
                                                                            let text_col = if mine {
                                                                                Color32::WHITE
                                                                            } else {
                                                                                with_alpha(pal.text_main, alpha)
                                                                            };

                                                                            let rxn_resp = egui::Frame::none()
                                                                                .fill(fill)
                                                                                .rounding(Rounding::same(11.0))
                                                                                .inner_margin(
                                                                                    egui::Margin::symmetric(
                                                                                        6.0, 2.0,
                                                                                    ),
                                                                                )
                                                                                .show(ui, |ui| {
                                                                                    ui.horizontal(|ui| {
                                                                                        ui.spacing_mut()
                                                                                            .item_spacing
                                                                                            .x = 3.0;
                                                                                        ui.label(
                                                                                            egui::RichText::new(
                                                                                                &rxn.emoji,
                                                                                            )
                                                                                                .size(12.5)
                                                                                                .color(text_col),
                                                                                        );
                                                                                        let av_tex = rxn
                                                                                            .users
                                                                                            .first()
                                                                                            .and_then(|u_em| {
                                                                                                if u_em.eq_ignore_ascii_case(&self.user_email) {
                                                                                                    self.user_avatar.clone()
                                                                                                } else {
                                                                                                    chat.members
                                                                                                        .iter()
                                                                                                        .find(|m| {
                                                                                                            m.email
                                                                                                                .as_deref()
                                                                                                                .map(|me| me.eq_ignore_ascii_case(u_em))
                                                                                                                .unwrap_or(false)
                                                                                                        })
                                                                                                        .and_then(|m| {
                                                                                                            m.avatar_tex.clone().or_else(|| {
                                                                                                                m.avatar_file_id
                                                                                                                    .as_ref()
                                                                                                                    .and_then(|fid| self.avatar_tex_cache.get(fid).cloned())
                                                                                                            })
                                                                                                        })
                                                                                                        .or_else(|| {
                                                                                                            if !chat.is_group
                                                                                                                && chat
                                                                                                                .peer_email
                                                                                                                .as_deref()
                                                                                                                .map(|pe| pe.eq_ignore_ascii_case(u_em))
                                                                                                                .unwrap_or(false)
                                                                                                            {
                                                                                                                chat.avatar_tex.clone()
                                                                                                            } else {
                                                                                                                None
                                                                                                            }
                                                                                                        })
                                                                                                }
                                                                                            });
                                                                                        let (badge_r, _) =
                                                                                            ui.allocate_exact_size(
                                                                                                Vec2::splat(15.0),
                                                                                                egui::Sense::hover(),
                                                                                            );
                                                                                        let who_lbl = rxn
                                                                                            .users
                                                                                            .first()
                                                                                            .map(|s| s.as_str())
                                                                                            .unwrap_or("U");
                                                                                        draw_reaction_avatar_badge(
                                                                                            ui.painter(),
                                                                                            badge_r,
                                                                                            &av_tex,
                                                                                            who_lbl,
                                                                                            GROUP_COLORS[(msg_idx + r_i) % GROUP_COLORS.len()],
                                                                                            alpha,
                                                                                        );
                                                                                    });
                                                                                })
                                                                                .response
                                                                                .interact(egui::Sense::click());

                                                                            if rxn_resp.clicked() {
                                                                                if let Some(m_id) = msg.server_id {
                                                                                    if let (
                                                                                        Some(sc_id),
                                                                                        Some(token),
                                                                                        Some(ws_tx),
                                                                                    ) = (
                                                                                        self.chats
                                                                                            .get(chat_idx)
                                                                                            .and_then(|c| {
                                                                                                c.server_chat_id
                                                                                            }),
                                                                                        self.session_token
                                                                                            .as_ref(),
                                                                                        self.ws_out_tx
                                                                                            .as_ref(),
                                                                                    ) {
                                                                                        let wire = WsWireEvent::ToggleReaction {
                                                                                            session_token: token.clone(),
                                                                                            chat_id: sc_id,
                                                                                            message_id: m_id,
                                                                                            emoji: rxn.emoji.clone(),
                                                                                        };
                                                                                        if let Ok(json) =
                                                                                            serde_json::to_string(&wire)
                                                                                        {
                                                                                            let _ = ws_tx.send(json);
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }

                                                                    ui.with_layout(
                                                                        egui::Layout::right_to_left(
                                                                            egui::Align::Center,
                                                                        ),
                                                                        |ui| {
                                                                            ui.spacing_mut()
                                                                                .item_spacing
                                                                                .x = 4.0;
                                                                            if msg.is_me {
                                                                                let (tick_rect, _) =
                                                                                    ui.allocate_exact_size(
                                                                                        Vec2::new(
                                                                                            18.0, 14.0,
                                                                                        ),
                                                                                        egui::Sense::hover(),
                                                                                    );
                                                                                draw_read_ticks(
                                                                                    ui.painter(),
                                                                                    tick_rect,
                                                                                    msg.is_read,
                                                                                    pal.text_muted,
                                                                                    alpha,
                                                                                );
                                                                            }
                                                                            ui.label(
                                                                                egui::RichText::new(&msg.time)
                                                                                    .size(11.0)
                                                                                    .color(with_alpha(
                                                                                        pal.text_muted,
                                                                                        alpha,
                                                                                    )),
                                                                            );
                                                                        },
                                                                    );
                                                                });
                                                            },
                                                        );
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
                                    ui.label(
                                        egui::RichText::new("Выберите чат, чтобы начать общение")
                                            .size(13.5)
                                            .color(pal.text_main),
                                    );
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
                let menu_w = 236.0;
                let row_h = 36.0;
                let bar_h = 38.0;
                let total_h = bar_h + 6.0 + row_h * 5.0 + 14.0;
                let screen = ctx.screen_rect();
                let base_pos = Pos2::new(
                    raw_pos.x.min(screen.max.x - menu_w - 8.0).max(8.0),
                    raw_pos.y.min(screen.max.y - total_h - 8.0).max(8.0),
                );
                let bar_rect = Rect::from_min_size(base_pos, Vec2::new(menu_w, bar_h));
                let menu_pos = Pos2::new(base_pos.x, base_pos.y + bar_h + 6.0);
                let menu_rect = Rect::from_min_size(menu_pos, Vec2::new(menu_w, row_h * 5.0 + 14.0));

                if opened_ctx_menu.is_none()
                    && ctx.input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked())
                {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !menu_rect.contains(pos) && !bar_rect.contains(pos) {
                            self.msg_context_menu = None;
                        }
                    }
                }

                let mut action = 0_u8;
                egui::Area::new(egui::Id::new("tg_msg_reaction_bar"))
                    .fixed_pos(bar_rect.min)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(pal.bg_popup)
                            .stroke(Stroke::new(1.0, pal.popup_border))
                            .rounding(Rounding::same(19.0))
                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.set_width(menu_w - 14.0);
                                let quick_emojis = ["❤️", "😭", "🤩", "🥰", "😱", "👍", "👌"];
                                let em_w = (menu_w - 14.0) / quick_emojis.len() as f32;
                                let (em_row_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(menu_w - 14.0, bar_h - 4.0),
                                    egui::Sense::hover(),
                                );
                                for (i, em) in quick_emojis.iter().enumerate() {
                                    let cell_rect = Rect::from_min_size(
                                        Pos2::new(
                                            em_row_rect.min.x + i as f32 * em_w,
                                            em_row_rect.min.y,
                                        ),
                                        Vec2::new(em_w, bar_h - 4.0),
                                    );
                                    let cell_resp = ui.interact(
                                        cell_rect,
                                        ui.make_persistent_id(("quick_rxn", c_idx, m_idx, i)),
                                        egui::Sense::click(),
                                    );
                                    if cell_resp.hovered() {
                                        ui.painter().circle_filled(
                                            cell_rect.center(),
                                            15.0,
                                            pal.bg_hover,
                                        );
                                    }
                                    ui.painter().text(
                                        cell_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        *em,
                                        FontId::proportional(16.0),
                                        pal.text_main,
                                    );
                                    if cell_resp.clicked() {
                                        if let Some(chat_ref) = self.chats.get(c_idx) {
                                            if let (
                                                Some(sc_id),
                                                Some(msg_ref),
                                                Some(token),
                                                Some(ws_tx),
                                            ) = (
                                                chat_ref.server_chat_id,
                                                chat_ref.messages.get(m_idx),
                                                self.session_token.as_ref(),
                                                self.ws_out_tx.as_ref(),
                                            ) {
                                                if let Some(m_id) = msg_ref.server_id {
                                                    let wire = WsWireEvent::ToggleReaction {
                                                        session_token: token.clone(),
                                                        chat_id: sc_id,
                                                        message_id: m_id,
                                                        emoji: (*em).to_string(),
                                                    };
                                                    if let Ok(json) = serde_json::to_string(&wire) {
                                                        let _ = ws_tx.send(json);
                                                    }
                                                }
                                            }
                                        }
                                        self.msg_context_menu = None;
                                    }
                                }
                            });
                    });

                egui::Area::new(egui::Id::new("tg_msg_context_menu"))
                    .fixed_pos(menu_pos)
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(pal.bg_popup)
                            .stroke(Stroke::new(1.0, pal.popup_border))
                            .rounding(Rounding::same(12.0))
                            .inner_margin(egui::Margin::symmetric(0.0, 6.0))
                            .show(ui, |ui| {
                                ui.set_width(menu_w);

                                let (rep_rect, rep_resp) =
                                    ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                if rep_resp.hovered() {
                                    ui.painter()
                                        .rect_filled(rep_rect, Rounding::ZERO, pal.bg_hover);
                                }
                                draw_reply_icon(
                                    ui.painter(),
                                    Pos2::new(rep_rect.min.x + 24.0, rep_rect.center().y),
                                    pal.text_main,
                                );
                                ui.painter().text(
                                    Pos2::new(rep_rect.min.x + 48.0, rep_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "Ответить",
                                    FontId::proportional(14.0),
                                    pal.text_main,
                                );
                                if rep_resp.clicked() {
                                    if let Some(chat_ref) = self.chats.get(c_idx) {
                                        if let Some(msg_ref) = chat_ref.messages.get(m_idx) {
                                            let preview = if !msg_ref.text.is_empty() {
                                                msg_ref.text.chars().take(60).collect()
                                            } else if let Some(att) = &msg_ref.attachment {
                                                format!("[{}] {}", att.kind.tag(), att.title)
                                            } else {
                                                "Сообщение".to_string()
                                            };
                                            self.reply_target = Some(ReplyTarget {
                                                message_id: msg_ref.server_id,
                                                sender: msg_ref.sender.clone(),
                                                preview,
                                            });
                                        }
                                    }
                                    self.msg_context_menu = None;
                                }

                                let (copy_rect, copy_resp) =
                                    ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                let copy_bg = if copy_resp.hovered() {
                                    pal.bg_hover
                                } else {
                                    pal.bg_popup
                                };
                                if copy_resp.hovered() {
                                    ui.painter()
                                        .rect_filled(copy_rect, Rounding::ZERO, pal.bg_hover);
                                }
                                draw_copy_icon(
                                    ui.painter(),
                                    Pos2::new(copy_rect.min.x + 24.0, copy_rect.center().y),
                                    pal.text_main,
                                    copy_bg,
                                );
                                ui.painter().text(
                                    Pos2::new(copy_rect.min.x + 48.0, copy_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "Копировать текст",
                                    FontId::proportional(14.0),
                                    pal.text_main,
                                );
                                if copy_resp.clicked() {
                                    action = 1;
                                }

                                let (fwd_rect, fwd_resp) =
                                    ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                if fwd_resp.hovered() {
                                    ui.painter()
                                        .rect_filled(fwd_rect, Rounding::ZERO, pal.bg_hover);
                                }
                                draw_forward_icon(
                                    ui.painter(),
                                    Pos2::new(fwd_rect.min.x + 24.0, fwd_rect.center().y),
                                    pal.accent_light,
                                );
                                ui.painter().text(
                                    Pos2::new(fwd_rect.min.x + 48.0, fwd_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "Переслать",
                                    FontId::proportional(14.0),
                                    pal.text_main,
                                );
                                if fwd_resp.clicked() {
                                    action = 4;
                                }

                                let (del_me_rect, del_me_resp) =
                                    ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                if del_me_resp.hovered() {
                                    ui.painter()
                                        .rect_filled(del_me_rect, Rounding::ZERO, pal.bg_hover);
                                }
                                draw_trash_icon(
                                    ui.painter(),
                                    Pos2::new(del_me_rect.min.x + 24.0, del_me_rect.center().y),
                                    pal.text_main,
                                );
                                ui.painter().text(
                                    Pos2::new(del_me_rect.min.x + 48.0, del_me_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "Удалить у меня",
                                    FontId::proportional(14.0),
                                    pal.text_main,
                                );
                                if del_me_resp.clicked() {
                                    action = 2;
                                }

                                let (del_all_rect, del_all_resp) =
                                    ui.allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                if del_all_resp.hovered() {
                                    ui.painter()
                                        .rect_filled(del_all_rect, Rounding::ZERO, pal.bg_hover);
                                }
                                draw_trash_icon(
                                    ui.painter(),
                                    Pos2::new(del_all_rect.min.x + 24.0, del_all_rect.center().y),
                                    DANGER_RED,
                                );
                                ui.painter().text(
                                    Pos2::new(del_all_rect.min.x + 48.0, del_all_rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    "Удалить у всех",
                                    FontId::proportional(14.0),
                                    DANGER_RED,
                                );
                                if del_all_resp.clicked() {
                                    action = 3;
                                }
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
                        self.active_modal = ActiveModal::ForwardMessage {
                            chat_idx: c_idx,
                            msg_idx: m_idx,
                        };
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
                let menu_pos = Pos2::new(
                    self.attach_btn_rect.min.x,
                    (self.attach_btn_rect.min.y - menu_h - 10.0).max(10.0),
                );
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
                                    let (row_rect, resp) = ui
                                        .allocate_exact_size(Vec2::new(menu_w, row_h), egui::Sense::click());
                                    let p = ui.painter();
                                    if resp.hovered() {
                                        p.rect_filled(row_rect, Rounding::ZERO, pal.bg_hover);
                                    }
                                    draw_menu_icon(
                                        p,
                                        Pos2::new(row_rect.min.x + 26.0, row_rect.center().y),
                                        kind,
                                        pal.text_main,
                                    );
                                    p.text(
                                        Pos2::new(row_rect.min.x + 52.0, row_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        label,
                                        FontId::proportional(14.0),
                                        pal.text_main,
                                    );
                                    if resp.clicked() {
                                        chosen_kind = Some(kind);
                                    }
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
                        let dim_size = Vec2::new(
                            (screen_rect.width() - 280.0).max(1.0),
                            screen_rect.height(),
                        );
                        let (rect, resp) = ui.allocate_exact_size(dim_size, egui::Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            Rounding::ZERO,
                            Color32::from_rgba_unmultiplied(0, 0, 0, 125),
                        );
                        if resp.clicked() {
                            self.menu_open = false;
                        }
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
                                    let (av_rect, av_resp) =
                                        ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::click());
                                    let initials: String =
                                        self.username.chars().take(2).collect::<String>().to_uppercase();
                                    draw_avatar_circle(
                                        ui.painter(),
                                        av_rect,
                                        &self.user_avatar,
                                        &initials,
                                        AUTH_BLUE,
                                        18.0,
                                    );
                                    if av_resp.on_hover_text("Открыть профиль").clicked() {
                                        self.menu_open = false;
                                        self.profile_editing = false;
                                        self.active_modal = ActiveModal::Profile;
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::TOP),
                                        |ui| {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("✕").color(pal.text_muted),
                                                    )
                                                        .fill(Color32::TRANSPARENT)
                                                        .rounding(14.0),
                                                )
                                                .clicked()
                                            {
                                                self.menu_open = false;
                                            }
                                        },
                                    );
                                });
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new(&self.username)
                                        .font(semibold_font(16.0))
                                        .color(pal.text_main),
                                );
                                if !self.user_email.is_empty() {
                                    ui.label(
                                        egui::RichText::new(&self.user_email)
                                            .size(12.0)
                                            .color(pal.text_muted),
                                    );
                                }
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(&self.user_tag)
                                        .size(12.5)
                                        .color(pal.accent_light),
                                );
                            });

                        ui.add_space(8.0);
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                            .show(ui, |ui| {
                                if draw_drawer_row(
                                    ui,
                                    "Мой профиль",
                                    pal.text_main,
                                    &pal,
                                    draw_person_icon,
                                )
                                    .clicked()
                                {
                                    self.menu_open = false;
                                    self.profile_editing = false;
                                    self.active_modal = ActiveModal::Profile;
                                }
                                ui.add_space(2.0);
                                if draw_drawer_row(
                                    ui,
                                    "Создать группу",
                                    pal.text_main,
                                    &pal,
                                    draw_group_icon,
                                )
                                    .clicked()
                                {
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
                                    [
                                        Pos2::new(ui.min_rect().min.x + 8.0, ui.cursor().min.y),
                                        Pos2::new(
                                            ui.min_rect().min.x + ui.available_width() - 8.0,
                                            ui.cursor().min.y,
                                        ),
                                    ],
                                    Stroke::new(1.0, pal.border_line),
                                );
                                ui.add_space(6.0);

                                egui::Frame::none()
                                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Тема")
                                                    .size(14.0)
                                                    .color(pal.text_main),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.allocate_ui_with_layout(
                                                        Vec2::new(96.0, 22.0),
                                                        egui::Layout::left_to_right(egui::Align::Center),
                                                        |ui| {
                                                            if draw_theme_slider_row(
                                                                ui,
                                                                self.is_dark_mode,
                                                                self.theme_anim,
                                                                &pal,
                                                                pal.bg_panel,
                                                            ) {
                                                                drawer_toggle_theme = true;
                                                            }
                                                        },
                                                    );
                                                },
                                            );
                                        });
                                    });

                                ui.add_space(6.0);
                                ui.painter().line_segment(
                                    [
                                        Pos2::new(ui.min_rect().min.x + 8.0, ui.cursor().min.y),
                                        Pos2::new(
                                            ui.min_rect().min.x + ui.available_width() - 8.0,
                                            ui.cursor().min.y,
                                        ),
                                    ],
                                    Stroke::new(1.0, pal.border_line),
                                );
                                ui.add_space(6.0);

                                if draw_drawer_row(
                                    ui,
                                    "Выйти из аккаунта",
                                    DANGER_RED,
                                    &pal,
                                    draw_logout_icon,
                                )
                                    .clicked()
                                {
                                    self.end_active_call(true);
                                    self.ws_loop_running.store(false, Ordering::SeqCst);
                                    self.audio_bridge.set_my_email(String::new());
                                    self.screen_bridge.set_my_email(String::new());
                                    self.menu_open = false;
                                    self.is_logged_in = false;
                                    self.session_token = None;
                                    clear_session();
                                    self.ws_out_tx = None;
                                    self.ws_connected = false;
                                    self.online_emails.clear();
                                    self.chats.retain(|c| c.server_chat_id.is_none());
                                    self.selected_chat = Some(0);
                                    self.auth_anim_start = Instant::now();
                                }
                            });

                        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("Egramm Beta")
                                            .size(11.5)
                                            .color(pal.text_muted),
                                    );
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
            #[allow(clippy::type_complexity)]
            let mut create_group_req: Option<(
                String,
                String,
                String,
                Color32,
                Option<egui::TextureHandle>,
                Option<PathBuf>,
                bool,
                String,
            )> = None;
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
                            .frame(
                                egui::Frame::none()
                                    .fill(pal.bg_panel)
                                    .stroke(Stroke::new(1.0, pal.popup_border))
                                    .rounding(Rounding::same(16.0)),
                            )
                            .order(egui::Order::Foreground)
                            .show(ctx, |ui| {
                                egui::Frame::none()
                                    .fill(pal.drawer_header)
                                    .rounding(Rounding {
                                        nw: 16.0,
                                        ne: 16.0,
                                        sw: 0.0,
                                        se: 0.0,
                                    })
                                    .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Профиль пользователя")
                                                    .font(semibold_font(16.5))
                                                    .color(pal.text_main),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                egui::RichText::new("✕")
                                                                    .size(15.0)
                                                                    .color(pal.text_muted),
                                                            )
                                                                .fill(Color32::TRANSPARENT)
                                                                .rounding(14.0),
                                                        )
                                                        .clicked()
                                                    {
                                                        self.active_modal = ActiveModal::None;
                                                    }
                                                },
                                            );
                                        });
                                        ui.add_space(12.0);
                                        ui.horizontal(|ui| {
                                            let (av_rect, _) =
                                                ui.allocate_exact_size(Vec2::splat(74.0), egui::Sense::hover());
                                            let initials: String =
                                                vp.name.chars().take(2).collect::<String>().to_uppercase();
                                            draw_avatar_circle(
                                                ui.painter(),
                                                av_rect,
                                                &vp.avatar_tex,
                                                &initials,
                                                vp.avatar_color,
                                                25.0,
                                            );
                                            ui.add_space(12.0);
                                            ui.vertical(|ui| {
                                                ui.add_space(8.0);
                                                ui.label(
                                                    egui::RichText::new(&vp.name)
                                                        .font(semibold_font(19.0))
                                                        .color(pal.text_main),
                                                );
                                                ui.label(
                                                    egui::RichText::new(if vp.online {
                                                        "в сети"
                                                    } else {
                                                        "не в сети"
                                                    })
                                                        .size(12.5)
                                                        .color(if vp.online {
                                                            ONLINE_GREEN
                                                        } else {
                                                            pal.text_muted
                                                        }),
                                                );
                                            });
                                        });
                                    });

                                egui::Frame::none()
                                    .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                    .show(ui, |ui| {
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
                                            ui.label(
                                                egui::RichText::new(k)
                                                    .size(11.5)
                                                    .color(pal.text_muted),
                                            );
                                            ui.label(
                                                egui::RichText::new(v)
                                                    .size(14.0)
                                                    .color(pal.text_main),
                                            );
                                            ui.add_space(10.0);
                                        }

                                        ui.add_space(14.0);
                                        if !vp.is_me && vp.email.contains('@') {
                                            if ui
                                                .add_sized(
                                                    Vec2::new(ui.available_width(), 36.0),
                                                    egui::Button::new(
                                                        egui::RichText::new("Написать сообщение")
                                                            .font(semibold_font(13.5))
                                                            .color(Color32::WHITE),
                                                    )
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
                        .frame(
                            egui::Frame::none()
                                .fill(pal.bg_panel)
                                .stroke(Stroke::new(1.0, pal.popup_border))
                                .rounding(Rounding::same(16.0)),
                        )
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .inner_margin(egui::Margin::same(16.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Переслать в чат")
                                                .font(semibold_font(16.5))
                                                .color(pal.text_main),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("✕")
                                                                .color(pal.text_muted),
                                                        )
                                                            .fill(Color32::TRANSPARENT)
                                                            .rounding(14.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.active_modal = ActiveModal::None;
                                                }
                                            },
                                        );
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
                                                        let (r, _) = ui.allocate_exact_size(
                                                            Vec2::splat(34.0),
                                                            egui::Sense::hover(),
                                                        );
                                                        draw_avatar_circle(
                                                            ui.painter(),
                                                            r,
                                                            &dst_chat.avatar_tex,
                                                            &dst_chat.avatar_tag,
                                                            dst_chat.avatar_color,
                                                            12.0,
                                                        );
                                                        ui.label(
                                                            egui::RichText::new(&dst_chat.title)
                                                                .font(semibold_font(13.5))
                                                                .color(pal.text_main),
                                                        );
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
                        .frame(
                            egui::Frame::none()
                                .fill(pal.bg_panel)
                                .stroke(Stroke::new(1.0, pal.popup_border))
                                .rounding(Rounding::same(16.0)),
                        )
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .fill(pal.drawer_header)
                                .rounding(Rounding {
                                    nw: 16.0,
                                    ne: 16.0,
                                    sw: 0.0,
                                    se: 0.0,
                                })
                                .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Мой профиль")
                                                .font(semibold_font(16.5))
                                                .color(pal.text_main),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("✕")
                                                                .size(15.0)
                                                                .color(pal.text_muted),
                                                        )
                                                            .fill(Color32::TRANSPARENT)
                                                            .rounding(14.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.active_modal = ActiveModal::None;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        let (av_rect, av_resp) =
                                            ui.allocate_exact_size(Vec2::splat(78.0), egui::Sense::click());
                                        let initials: String =
                                            self.username.chars().take(2).collect::<String>().to_uppercase();
                                        draw_avatar_circle(
                                            ui.painter(),
                                            av_rect,
                                            &self.user_avatar,
                                            &initials,
                                            AUTH_BLUE,
                                            26.0,
                                        );
                                        ui.painter().circle_filled(
                                            Pos2::new(av_rect.max.x - 10.0, av_rect.max.y - 10.0),
                                            8.5,
                                            pal.drawer_header,
                                        );
                                        ui.painter().circle_filled(
                                            Pos2::new(av_rect.max.x - 10.0, av_rect.max.y - 10.0),
                                            6.0,
                                            ONLINE_GREEN,
                                        );
                                        if av_resp.hovered() {
                                            ui.painter().circle_filled(
                                                av_rect.center(),
                                                39.0,
                                                Color32::from_rgba_unmultiplied(0, 0, 0, 130),
                                            );
                                            ui.painter().text(
                                                av_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "Фото",
                                                semibold_font(12.5),
                                                Color32::WHITE,
                                            );
                                        }
                                        if av_resp.clicked() {
                                            pick_user_avatar = true;
                                        }
                                        ui.add_space(10.0);
                                        ui.vertical(|ui| {
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(&self.username)
                                                    .font(semibold_font(19.0))
                                                    .color(pal.text_main),
                                            );
                                            ui.label(
                                                egui::RichText::new("в сети")
                                                    .size(12.5)
                                                    .color(ONLINE_GREEN),
                                            );
                                            ui.add_space(6.0);
                                            ui.horizontal(|ui| {
                                                if ui
                                                    .add_sized(
                                                        Vec2::new(118.0, 26.0),
                                                        egui::Button::new(
                                                            egui::RichText::new("Сменить фото")
                                                                .size(12.0)
                                                                .color(pal.accent_light),
                                                        )
                                                            .fill(pal.bg_input)
                                                            .rounding(13.0),
                                                    )
                                                    .clicked()
                                                {
                                                    pick_user_avatar = true;
                                                }
                                                if self.user_avatar.is_some()
                                                    && ui
                                                    .add_sized(
                                                        Vec2::new(72.0, 26.0),
                                                        egui::Button::new(
                                                            egui::RichText::new("Удалить")
                                                                .size(12.0)
                                                                .color(DANGER_RED),
                                                        )
                                                            .fill(Color32::TRANSPARENT)
                                                            .rounding(13.0),
                                                    )
                                                    .clicked()
                                                {
                                                    clear_user_avatar = true;
                                                }
                                            });
                                        });
                                    });
                                });

                            egui::Frame::none()
                                .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                .show(ui, |ui| {
                                    if self.profile_editing {
                                        ui.label(
                                            egui::RichText::new("Отображаемое имя")
                                                .size(12.0)
                                                .color(pal.text_muted),
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.username)
                                                .desired_width(344.0)
                                                .margin(Vec2::new(10.0, 7.0)),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new("Имя пользователя (@tag)")
                                                .size(12.0)
                                                .color(pal.text_muted),
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.user_tag)
                                                .desired_width(344.0)
                                                .margin(Vec2::new(10.0, 7.0)),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new("О себе")
                                                .size(12.0)
                                                .color(pal.text_muted),
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.user_bio)
                                                .hint_text("Расскажите о себе...")
                                                .desired_width(344.0)
                                                .margin(Vec2::new(10.0, 7.0)),
                                        );
                                    } else {
                                        for (k, v) in [
                                            ("Имя пользователя", self.user_tag.as_str()),
                                            (
                                                "О себе",
                                                if self.user_bio.is_empty() {
                                                    "Не заполнено"
                                                } else {
                                                    self.user_bio.as_str()
                                                },
                                            ),
                                            (
                                                "Google-аккаунт",
                                                if self.user_email.is_empty() {
                                                    "Не указан"
                                                } else {
                                                    self.user_email.as_str()
                                                },
                                            ),
                                        ] {
                                            ui.label(
                                                egui::RichText::new(k)
                                                    .size(11.5)
                                                    .color(pal.text_muted),
                                            );
                                            ui.label(
                                                egui::RichText::new(v)
                                                    .size(14.0)
                                                    .color(pal.text_main),
                                            );
                                            ui.add_space(10.0);
                                        }
                                    }
                                    ui.add_space(14.0);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if self.profile_editing {
                                                if ui
                                                    .add_sized(
                                                        Vec2::new(124.0, 34.0),
                                                        egui::Button::new(
                                                            egui::RichText::new("Сохранить")
                                                                .color(Color32::WHITE),
                                                        )
                                                            .fill(pal.accent_blue)
                                                            .rounding(17.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.profile_editing = false;
                                                    save_profile_now = true;
                                                    toast_msg = Some("Профиль сохранён".into());
                                                }
                                            } else if ui
                                                .add_sized(
                                                    Vec2::new(150.0, 34.0),
                                                    egui::Button::new(
                                                        egui::RichText::new("Редактировать")
                                                            .color(Color32::WHITE),
                                                    )
                                                        .fill(pal.accent_blue)
                                                        .rounding(17.0),
                                                )
                                                .clicked()
                                            {
                                                self.profile_editing = true;
                                            }
                                        },
                                    );
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
                        .frame(
                            egui::Frame::none()
                                .fill(pal.bg_panel)
                                .stroke(Stroke::new(1.0, pal.popup_border))
                                .rounding(Rounding::same(16.0)),
                        )
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .inner_margin(egui::Margin::same(18.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Новая группа")
                                                .font(semibold_font(17.5))
                                                .color(pal.text_main),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("✕")
                                                                .color(pal.text_muted),
                                                        )
                                                            .fill(Color32::TRANSPARENT)
                                                            .rounding(14.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.active_modal = ActiveModal::None;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(12.0);
                                    ui.horizontal(|ui| {
                                        let (av_rect, av_resp) =
                                            ui.allocate_exact_size(Vec2::splat(68.0), egui::Sense::click());
                                        let t: String = self
                                            .new_group_name
                                            .trim()
                                            .chars()
                                            .take(2)
                                            .collect::<String>()
                                            .to_uppercase();
                                        let preview_tag = if t.is_empty() { "GR".into() } else { t };
                                        draw_avatar_circle(
                                            ui.painter(),
                                            av_rect,
                                            &self.new_group_avatar,
                                            &preview_tag,
                                            GROUP_COLORS[self.new_group_color_idx],
                                            22.0,
                                        );
                                        if av_resp.hovered() {
                                            ui.painter().circle_filled(
                                                av_rect.center(),
                                                34.0,
                                                Color32::from_rgba_unmultiplied(0, 0, 0, 125),
                                            );
                                            ui.painter().text(
                                                av_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                "Фото",
                                                semibold_font(12.0),
                                                Color32::WHITE,
                                            );
                                        }
                                        if av_resp
                                            .on_hover_text("Выбрать аватарку группы")
                                            .clicked()
                                        {
                                            pick_new_group_avatar = true;
                                        }
                                        ui.add_space(10.0);
                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new("Название группы")
                                                    .size(12.0)
                                                    .color(pal.text_muted),
                                            );
                                            ui.add(
                                                egui::TextEdit::singleline(&mut self.new_group_name)
                                                    .hint_text("Например: CS2 Squad")
                                                    .desired_width(268.0)
                                                    .margin(Vec2::new(10.0, 8.0)),
                                            );
                                            ui.add_space(6.0);
                                            ui.horizontal(|ui| {
                                                for (ci, &col) in GROUP_COLORS.iter().enumerate() {
                                                    let (cr, resp) = ui.allocate_exact_size(
                                                        Vec2::splat(20.0),
                                                        egui::Sense::click(),
                                                    );
                                                    ui.painter().circle_filled(cr.center(), 8.5, col);
                                                    if self.new_group_color_idx == ci {
                                                        ui.painter().circle_stroke(
                                                            cr.center(),
                                                            10.0,
                                                            Stroke::new(1.8, Color32::WHITE),
                                                        );
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
                                    ui.label(
                                        egui::RichText::new("Описание (необязательно)")
                                            .size(12.0)
                                            .color(pal.text_muted),
                                    );
                                    ui.add(
                                        egui::TextEdit::multiline(&mut self.new_group_desc)
                                            .hint_text("О чём эта группа...")
                                            .desired_rows(2)
                                            .desired_width(354.0)
                                            .margin(Vec2::new(10.0, 8.0)),
                                    );
                                    ui.add_space(12.0);
                                    ui.label(
                                        egui::RichText::new("Тип группы")
                                            .size(12.0)
                                            .color(pal.text_muted),
                                    );
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        for (is_pub, lbl) in [(false, "Частная"), (true, "Публичная")] {
                                            let active = self.new_group_public == is_pub;
                                            if ui
                                                .add_sized(
                                                    Vec2::new(112.0, 30.0),
                                                    egui::Button::new(
                                                        egui::RichText::new(lbl).size(13.0).color(
                                                            if active {
                                                                Color32::WHITE
                                                            } else {
                                                                pal.text_muted
                                                            },
                                                        ),
                                                    )
                                                        .fill(if active {
                                                            pal.accent_blue
                                                        } else {
                                                            pal.bg_input
                                                        })
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
                                        ui.label(
                                            egui::RichText::new("Публичная ссылка (egramm.me/...)")
                                                .size(12.0)
                                                .color(pal.text_muted),
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.new_group_link)
                                                .hint_text("my_squad")
                                                .desired_width(354.0)
                                                .margin(Vec2::new(10.0, 7.0)),
                                        );
                                    }
                                    ui.add_space(18.0);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_sized(
                                                    Vec2::new(130.0, 36.0),
                                                    egui::Button::new(
                                                        egui::RichText::new("Создать")
                                                            .color(Color32::WHITE),
                                                    )
                                                        .fill(pal.accent_blue)
                                                        .rounding(18.0),
                                                )
                                                .clicked()
                                            {
                                                let title = self.new_group_name.trim().to_string();
                                                if !title.is_empty() {
                                                    let tag: String = title
                                                        .chars()
                                                        .take(2)
                                                        .collect::<String>()
                                                        .to_uppercase();
                                                    let link = if self.new_group_public {
                                                        let clean = self.new_group_link.trim();
                                                        if clean.is_empty() {
                                                            format!(
                                                                "egramm.me/group_{}",
                                                                self.chats.len() + 1
                                                            )
                                                        } else {
                                                            format!("egramm.me/{}", clean)
                                                        }
                                                    } else {
                                                        format!(
                                                            "egramm.me/+inv{:04}",
                                                            (self.chats.len() + 1) * 731
                                                        )
                                                    };
                                                    create_group_req = Some((
                                                        title,
                                                        self.new_group_desc.trim().to_string(),
                                                        if tag.is_empty() {
                                                            "GR".into()
                                                        } else {
                                                            tag
                                                        },
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
                                            if ui
                                                .add_sized(
                                                    Vec2::new(90.0, 36.0),
                                                    egui::Button::new(
                                                        egui::RichText::new("Отмена")
                                                            .color(pal.text_muted),
                                                    )
                                                        .fill(Color32::TRANSPARENT)
                                                        .rounding(18.0),
                                                )
                                                .clicked()
                                            {
                                                self.active_modal = ActiveModal::None;
                                            }
                                        },
                                    );
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
                            .frame(
                                egui::Frame::none()
                                    .fill(pal.bg_panel)
                                    .stroke(Stroke::new(1.0, pal.popup_border))
                                    .rounding(Rounding::same(16.0)),
                            )
                            .order(egui::Order::Foreground)
                            .show(ctx, |ui| {
                                let mut close_modal = false;
                                egui::Frame::none()
                                    .inner_margin(egui::Margin::symmetric(18.0, 14.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("Управление группой")
                                                    .font(semibold_font(17.5))
                                                    .color(pal.text_main),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                egui::RichText::new("✕")
                                                                    .size(15.0)
                                                                    .color(pal.text_muted),
                                                            )
                                                                .fill(Color32::TRANSPARENT)
                                                                .rounding(14.0),
                                                        )
                                                        .clicked()
                                                    {
                                                        close_modal = true;
                                                    }
                                                },
                                            );
                                        });
                                        ui.add_space(10.0);
                                        ui.horizontal(|ui| {
                                            for (tab, label) in [
                                                (GroupTab::Info, "Инфо"),
                                                (GroupTab::Members, "Участники"),
                                                (GroupTab::Admins, "Админы"),
                                                (GroupTab::Permissions, "Права"),
                                            ] {
                                                let active = self.group_tab == tab;
                                                if ui
                                                    .add_sized(
                                                        Vec2::new(108.0, 32.0),
                                                        egui::Button::new(
                                                            egui::RichText::new(label)
                                                                .font(semibold_font(13.0))
                                                                .color(if active {
                                                                    Color32::WHITE
                                                                } else {
                                                                    pal.text_muted
                                                                }),
                                                        )
                                                            .fill(if active {
                                                                pal.accent_blue
                                                            } else {
                                                                pal.bg_input
                                                            })
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
                                            egui::ScrollArea::vertical().max_height(442.0).show(
                                                ui,
                                                |ui| match self.group_tab {
                                                    GroupTab::Info => {
                                                        if !chat.invite_code.is_empty() {
                                                            ui.horizontal(|ui| {
                                                                ui.label(
                                                                    egui::RichText::new(format!(
                                                                        "Код группы: {}",
                                                                        chat.invite_code
                                                                    ))
                                                                        .font(semibold_font(12.5))
                                                                        .color(pal.accent_light),
                                                                );
                                                                if ui
                                                                    .add(
                                                                        egui::Button::new(
                                                                            egui::RichText::new(
                                                                                "Копировать код",
                                                                            )
                                                                                .size(11.5)
                                                                                .color(pal.text_muted),
                                                                        )
                                                                            .fill(pal.bg_input)
                                                                            .rounding(10.0),
                                                                    )
                                                                    .clicked()
                                                                {
                                                                    ctx.copy_text(
                                                                        chat.invite_code.clone(),
                                                                    );
                                                                    toast_msg = Some(
                                                                        "Код группы скопирован".into(),
                                                                    );
                                                                }
                                                            });
                                                            ui.add_space(8.0);
                                                        }
                                                        ui.horizontal(|ui| {
                                                            let (av_rect, av_resp) =
                                                                ui.allocate_exact_size(
                                                                    Vec2::splat(74.0),
                                                                    egui::Sense::click(),
                                                                );
                                                            draw_avatar_circle(
                                                                ui.painter(),
                                                                av_rect,
                                                                &chat.avatar_tex,
                                                                &chat.avatar_tag,
                                                                chat.avatar_color,
                                                                24.0,
                                                            );
                                                            if av_resp.hovered() {
                                                                ui.painter().circle_filled(
                                                                    av_rect.center(),
                                                                    37.0,
                                                                    Color32::from_rgba_unmultiplied(
                                                                        0, 0, 0, 125,
                                                                    ),
                                                                );
                                                                ui.painter().text(
                                                                    av_rect.center(),
                                                                    egui::Align2::CENTER_CENTER,
                                                                    "Фото",
                                                                    semibold_font(12.5),
                                                                    Color32::WHITE,
                                                                );
                                                            }
                                                            if av_resp.clicked() {
                                                                pick_existing_group_avatar = Some(g_idx);
                                                            }
                                                            ui.add_space(12.0);
                                                            ui.vertical(|ui| {
                                                                ui.label(
                                                                    egui::RichText::new("Название")
                                                                        .size(12.0)
                                                                        .color(pal.text_muted),
                                                                );
                                                                if ui
                                                                    .add(
                                                                        egui::TextEdit::singleline(
                                                                            &mut chat.title,
                                                                        )
                                                                            .desired_width(345.0)
                                                                            .margin(Vec2::new(10.0, 8.0)),
                                                                    )
                                                                    .changed()
                                                                {
                                                                    let tag: String = chat
                                                                        .title
                                                                        .trim()
                                                                        .chars()
                                                                        .take(2)
                                                                        .collect::<String>()
                                                                        .to_uppercase();
                                                                    if !tag.is_empty() {
                                                                        chat.avatar_tag = tag;
                                                                    }
                                                                }
                                                                ui.add_space(6.0);
                                                                ui.horizontal(|ui| {
                                                                    if ui
                                                                        .add_sized(
                                                                            Vec2::new(124.0, 26.0),
                                                                            egui::Button::new(
                                                                                egui::RichText::new(
                                                                                    "Сменить аватар",
                                                                                )
                                                                                    .size(12.0)
                                                                                    .color(pal.accent_light),
                                                                            )
                                                                                .fill(pal.bg_input)
                                                                                .rounding(13.0),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        pick_existing_group_avatar =
                                                                            Some(g_idx);
                                                                    }
                                                                    if chat.avatar_tex.is_some()
                                                                        && ui
                                                                        .add_sized(
                                                                            Vec2::new(82.0, 26.0),
                                                                            egui::Button::new(
                                                                                egui::RichText::new(
                                                                                    "Сбросить",
                                                                                )
                                                                                    .size(12.0)
                                                                                    .color(DANGER_RED),
                                                                            )
                                                                                .fill(Color32::TRANSPARENT)
                                                                                .rounding(13.0),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        chat.avatar_tex = None;
                                                                    }
                                                                });
                                                            });
                                                        });
                                                        ui.add_space(12.0);
                                                        ui.label(
                                                            egui::RichText::new("Цвет иконки")
                                                                .size(12.0)
                                                                .color(pal.text_muted),
                                                        );
                                                        ui.add_space(4.0);
                                                        ui.horizontal(|ui| {
                                                            for &col in &GROUP_COLORS {
                                                                let (cr, resp) = ui.allocate_exact_size(
                                                                    Vec2::splat(24.0),
                                                                    egui::Sense::click(),
                                                                );
                                                                ui.painter().circle_filled(
                                                                    cr.center(),
                                                                    10.0,
                                                                    col,
                                                                );
                                                                if chat.avatar_color == col
                                                                    && chat.avatar_tex.is_none()
                                                                {
                                                                    ui.painter().circle_stroke(
                                                                        cr.center(),
                                                                        11.5,
                                                                        Stroke::new(2.0, Color32::WHITE),
                                                                    );
                                                                }
                                                                if resp.clicked() {
                                                                    chat.avatar_color = col;
                                                                    chat.avatar_tex = None;
                                                                }
                                                            }
                                                        });
                                                        ui.add_space(12.0);
                                                        ui.label(
                                                            egui::RichText::new("Описание")
                                                                .size(12.0)
                                                                .color(pal.text_muted),
                                                        );
                                                        ui.add(
                                                            egui::TextEdit::multiline(
                                                                &mut chat.description,
                                                            )
                                                                .hint_text("Добавьте описание группы...")
                                                                .desired_rows(3)
                                                                .desired_width(440.0)
                                                                .margin(Vec2::new(10.0, 8.0)),
                                                        );
                                                        ui.add_space(12.0);
                                                        ui.horizontal(|ui| {
                                                            ui.label(
                                                                egui::RichText::new("Публичная группа")
                                                                    .size(13.5)
                                                                    .color(pal.text_main),
                                                            );
                                                            ui.with_layout(
                                                                egui::Layout::right_to_left(
                                                                    egui::Align::Center,
                                                                ),
                                                                |ui| {
                                                                    draw_toggle_switch(
                                                                        ui,
                                                                        &mut chat.is_public,
                                                                        &pal,
                                                                    );
                                                                },
                                                            );
                                                        });
                                                        ui.add_space(6.0);
                                                        ui.label(
                                                            egui::RichText::new(if chat.is_public {
                                                                "Публичная ссылка"
                                                            } else {
                                                                "Пригласительная ссылка"
                                                            })
                                                                .size(12.0)
                                                                .color(pal.text_muted),
                                                        );
                                                        ui.horizontal(|ui| {
                                                            ui.add(
                                                                egui::TextEdit::singleline(
                                                                    &mut chat.invite_link,
                                                                )
                                                                    .desired_width(332.0)
                                                                    .margin(Vec2::new(10.0, 7.0)),
                                                            );
                                                            if ui
                                                                .add_sized(
                                                                    Vec2::new(98.0, 32.0),
                                                                    egui::Button::new(
                                                                        egui::RichText::new("Копировать")
                                                                            .size(12.5)
                                                                            .color(pal.accent_light),
                                                                    )
                                                                        .fill(pal.bg_input)
                                                                        .rounding(14.0),
                                                                )
                                                                .clicked()
                                                            {
                                                                ctx.copy_text(chat.invite_link.clone());
                                                                toast_msg =
                                                                    Some("Ссылка скопирована".into());
                                                            }
                                                        });

                                                        let is_creator = chat.members.iter().any(|m| {
                                                            m.role == GroupRole::Owner
                                                                && m.email
                                                                .as_deref()
                                                                .unwrap_or(&m.tag)
                                                                .eq_ignore_ascii_case(
                                                                    &self.user_email,
                                                                )
                                                        });

                                                        if is_creator {
                                                            ui.add_space(18.0);
                                                            if ui
                                                                .add_sized(
                                                                    Vec2::new(165.0, 34.0),
                                                                    egui::Button::new(
                                                                        egui::RichText::new(
                                                                            "Удалить группу",
                                                                        )
                                                                            .color(DANGER_RED),
                                                                    )
                                                                        .fill(
                                                                            Color32::from_rgba_unmultiplied(
                                                                                235, 95, 95, 28,
                                                                            ),
                                                                        )
                                                                        .rounding(17.0),
                                                                )
                                                                .clicked()
                                                            {
                                                                delete_group_idx = Some(g_idx);
                                                                close_modal = true;
                                                            }
                                                        }
                                                    }
                                                    GroupTab::Members => {
                                                        ui.horizontal(|ui| {
                                                            ui.add(
                                                                egui::TextEdit::singleline(
                                                                    &mut self.add_member_input,
                                                                )
                                                                    .hint_text(
                                                                        "Введите @ник или почту зарегистрированного",
                                                                    )
                                                                    .desired_width(325.0)
                                                                    .margin(Vec2::new(10.0, 8.0)),
                                                            );
                                                            if ui
                                                                .add_sized(
                                                                    Vec2::new(106.0, 34.0),
                                                                    egui::Button::new(
                                                                        egui::RichText::new("Пригласить")
                                                                            .color(Color32::WHITE),
                                                                    )
                                                                        .fill(pal.accent_blue)
                                                                        .rounding(17.0),
                                                                )
                                                                .clicked()
                                                            {
                                                                let raw = self
                                                                    .add_member_input
                                                                    .trim()
                                                                    .to_string();
                                                                if !raw.is_empty() {
                                                                    invite_member_req =
                                                                        Some((g_idx, raw));
                                                                    self.add_member_input.clear();
                                                                }
                                                            }
                                                        });
                                                        ui.add_space(12.0);
                                                        let mut kick_idx: Option<usize> = None;
                                                        let mut promote_idx: Option<usize> = None;
                                                        for (m_idx, member) in
                                                            chat.members.iter().enumerate()
                                                        {
                                                            let card = egui::Frame::none()
                                                                .fill(pal.bg_input)
                                                                .rounding(Rounding::same(12.0))
                                                                .inner_margin(egui::Margin::symmetric(
                                                                    12.0, 8.0,
                                                                ))
                                                                .show(ui, |ui| {
                                                                    ui.horizontal(|ui| {
                                                                        let (r, _) =
                                                                            ui.allocate_exact_size(
                                                                                Vec2::splat(38.0),
                                                                                egui::Sense::hover(),
                                                                            );
                                                                        let init: String = member
                                                                            .name
                                                                            .chars()
                                                                            .take(2)
                                                                            .collect::<String>()
                                                                            .to_uppercase();
                                                                        draw_avatar_circle(
                                                                            ui.painter(),
                                                                            r,
                                                                            &member.avatar_tex,
                                                                            &init,
                                                                            member.avatar_color,
                                                                            13.0,
                                                                        );
                                                                        ui.vertical(|ui| {
                                                                            ui.horizontal(|ui| {
                                                                                ui.label(
                                                                                    egui::RichText::new(
                                                                                        &member.name,
                                                                                    )
                                                                                        .font(semibold_font(
                                                                                            14.0,
                                                                                        ))
                                                                                        .color(pal.text_main),
                                                                                );
                                                                                if member.role
                                                                                    != GroupRole::Member
                                                                                {
                                                                                    let badge =
                                                                                        if !member
                                                                                            .custom_title
                                                                                            .is_empty()
                                                                                        {
                                                                                            member
                                                                                                .custom_title
                                                                                                .as_str()
                                                                                        } else {
                                                                                            member
                                                                                                .role
                                                                                                .label()
                                                                                        };
                                                                                    ui.label(
                                                                                        egui::RichText::new(
                                                                                            badge,
                                                                                        )
                                                                                            .size(11.5)
                                                                                            .color(
                                                                                                pal.accent_light,
                                                                                            ),
                                                                                    );
                                                                                }
                                                                            });
                                                                            ui.label(
                                                                                egui::RichText::new(
                                                                                    format!(
                                                                                        "{} • {}",
                                                                                        member.tag,
                                                                                        if member.online {
                                                                                            "в сети"
                                                                                        } else {
                                                                                            "не в сети"
                                                                                        }
                                                                                    ),
                                                                                )
                                                                                    .size(11.5)
                                                                                    .color(pal.text_muted),
                                                                            );
                                                                        });
                                                                        if member.role
                                                                            != GroupRole::Owner
                                                                        {
                                                                            ui.with_layout(
                                                                                egui::Layout::right_to_left(
                                                                                    egui::Align::Center,
                                                                                ),
                                                                                |ui| {
                                                                                    if ui
                                                                                        .add(
                                                                                            egui::Button::new(
                                                                                                egui::RichText::new(
                                                                                                    "Удалить",
                                                                                                )
                                                                                                    .size(12.0)
                                                                                                    .color(DANGER_RED),
                                                                                            )
                                                                                                .fill(
                                                                                                    Color32::TRANSPARENT,
                                                                                                )
                                                                                                .rounding(10.0),
                                                                                        )
                                                                                        .clicked()
                                                                                    {
                                                                                        kick_idx =
                                                                                            Some(m_idx);
                                                                                    }
                                                                                    if member.role
                                                                                        == GroupRole::Member
                                                                                        && ui
                                                                                        .add(
                                                                                            egui::Button::new(
                                                                                                egui::RichText::new(
                                                                                                    "Сделать админом",
                                                                                                )
                                                                                                    .size(12.0)
                                                                                                    .color(
                                                                                                        pal.accent_light,
                                                                                                    ),
                                                                                            )
                                                                                                .fill(pal.bg_panel)
                                                                                                .rounding(10.0),
                                                                                        )
                                                                                        .clicked()
                                                                                    {
                                                                                        promote_idx =
                                                                                            Some(m_idx);
                                                                                    }
                                                                                },
                                                                            );
                                                                        }
                                                                    });
                                                                })
                                                                .response
                                                                .interact(egui::Sense::click());
                                                            if card
                                                                .on_hover_text(
                                                                    "Нажмите, чтобы посмотреть профиль",
                                                                )
                                                                .clicked()
                                                            {
                                                                open_member_prof_req =
                                                                    Some((g_idx, m_idx));
                                                            }
                                                            ui.add_space(6.0);
                                                        }
                                                        if let Some(p_idx) = promote_idx {
                                                            if let Some(m) = chat.members.get_mut(p_idx)
                                                            {
                                                                m.role = GroupRole::Admin;
                                                                m.rights = AdminRights::default_admin();
                                                                if m.custom_title.is_empty() {
                                                                    m.custom_title =
                                                                        "Администратор".into();
                                                                }
                                                                self.group_tab = GroupTab::Admins;
                                                                self.editing_admin_idx = Some(p_idx);
                                                                toast_msg = Some(format!(
                                                                    "{} назначен админом",
                                                                    m.name
                                                                ));
                                                            }
                                                        }
                                                        if let Some(k_idx) = kick_idx {
                                                            if k_idx < chat.members.len() {
                                                                let removed = chat.members.remove(k_idx);
                                                                chat.refresh_group_subtitle();
                                                                toast_msg = Some(format!(
                                                                    "{} исключён из группы",
                                                                    removed.name
                                                                ));
                                                            }
                                                        }
                                                    }
                                                    GroupTab::Admins => {
                                                        if let Some(a_idx) = self.editing_admin_idx {
                                                            if let Some(admin) =
                                                                chat.members.get_mut(a_idx)
                                                            {
                                                                ui.horizontal(|ui| {
                                                                    if ui
                                                                        .add(
                                                                            egui::Button::new(
                                                                                egui::RichText::new(
                                                                                    "← Назад",
                                                                                )
                                                                                    .color(pal.accent_light),
                                                                            )
                                                                                .fill(pal.bg_input)
                                                                                .rounding(12.0),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        self.editing_admin_idx = None;
                                                                    }
                                                                    ui.label(
                                                                        egui::RichText::new(format!(
                                                                            "Права: {}",
                                                                            admin.name
                                                                        ))
                                                                            .font(semibold_font(15.0))
                                                                            .color(pal.text_main),
                                                                    );
                                                                });
                                                                ui.add_space(10.0);
                                                                ui.label(
                                                                    egui::RichText::new("Должность")
                                                                        .size(12.0)
                                                                        .color(pal.text_muted),
                                                                );
                                                                ui.add(
                                                                    egui::TextEdit::singleline(
                                                                        &mut admin.custom_title,
                                                                    )
                                                                        .hint_text("Например: Модератор")
                                                                        .desired_width(430.0)
                                                                        .margin(Vec2::new(10.0, 7.0)),
                                                                );
                                                                ui.add_space(12.0);
                                                                let mut perm_row =
                                                                    |ui: &mut egui::Ui,
                                                                     title: &str,
                                                                     flag: &mut bool| {
                                                                        ui.horizontal(|ui| {
                                                                            ui.label(
                                                                                egui::RichText::new(
                                                                                    title,
                                                                                )
                                                                                    .size(13.5)
                                                                                    .color(pal.text_main),
                                                                            );
                                                                            ui.with_layout(
                                                                                egui::Layout::right_to_left(
                                                                                    egui::Align::Center,
                                                                                ),
                                                                                |ui| {
                                                                                    draw_toggle_switch(
                                                                                        ui, flag, &pal,
                                                                                    );
                                                                                },
                                                                            );
                                                                        });
                                                                        ui.add_space(6.0);
                                                                    };
                                                                perm_row(
                                                                    ui,
                                                                    "Изменение профиля группы",
                                                                    &mut admin.rights.change_info,
                                                                );
                                                                perm_row(
                                                                    ui,
                                                                    "Удаление сообщений",
                                                                    &mut admin.rights.delete_messages,
                                                                );
                                                                perm_row(
                                                                    ui,
                                                                    "Блокировка участников",
                                                                    &mut admin.rights.ban_users,
                                                                );
                                                                perm_row(
                                                                    ui,
                                                                    "Приглашение по ссылке",
                                                                    &mut admin.rights.invite_users,
                                                                );
                                                                perm_row(
                                                                    ui,
                                                                    "Закрепление сообщений",
                                                                    &mut admin.rights.pin_messages,
                                                                );
                                                                perm_row(
                                                                    ui,
                                                                    "Управление войс-чатами",
                                                                    &mut admin.rights.manage_voice,
                                                                );
                                                                perm_row(
                                                                    ui,
                                                                    "Назначение новых админов",
                                                                    &mut admin.rights.add_admins,
                                                                );

                                                                if admin.role == GroupRole::Admin {
                                                                    ui.add_space(10.0);
                                                                    if ui
                                                                        .add_sized(
                                                                            Vec2::new(190.0, 34.0),
                                                                            egui::Button::new(
                                                                                egui::RichText::new(
                                                                                    "Снять права админа",
                                                                                )
                                                                                    .color(DANGER_RED),
                                                                            )
                                                                                .fill(
                                                                                    Color32::from_rgba_unmultiplied(
                                                                                        235, 95, 95, 28,
                                                                                    ),
                                                                                )
                                                                                .rounding(17.0),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        admin.role = GroupRole::Member;
                                                                        admin.custom_title.clear();
                                                                        admin.rights =
                                                                            AdminRights::none();
                                                                        self.editing_admin_idx = None;
                                                                        toast_msg = Some(
                                                                            "Права администратора сняты"
                                                                                .into(),
                                                                        );
                                                                    }
                                                                }
                                                            } else {
                                                                self.editing_admin_idx = None;
                                                            }
                                                        } else {
                                                            for (m_idx, member) in
                                                                chat.members.iter().enumerate()
                                                            {
                                                                if member.role == GroupRole::Member {
                                                                    continue;
                                                                }
                                                                let card = egui::Frame::none()
                                                                    .fill(pal.bg_input)
                                                                    .rounding(Rounding::same(12.0))
                                                                    .inner_margin(
                                                                        egui::Margin::symmetric(
                                                                            12.0, 9.0,
                                                                        ),
                                                                    )
                                                                    .show(ui, |ui| {
                                                                        ui.horizontal(|ui| {
                                                                            let (r, _) =
                                                                                ui.allocate_exact_size(
                                                                                    Vec2::splat(38.0),
                                                                                    egui::Sense::hover(),
                                                                                );
                                                                            let init: String =
                                                                                member
                                                                                    .name
                                                                                    .chars()
                                                                                    .take(2)
                                                                                    .collect::<String>()
                                                                                    .to_uppercase();
                                                                            draw_avatar_circle(
                                                                                ui.painter(),
                                                                                r,
                                                                                &member.avatar_tex,
                                                                                &init,
                                                                                member.avatar_color,
                                                                                13.0,
                                                                            );
                                                                            ui.vertical(|ui| {
                                                                                ui.label(
                                                                                    egui::RichText::new(
                                                                                        &member.name,
                                                                                    )
                                                                                        .font(semibold_font(
                                                                                            14.0,
                                                                                        ))
                                                                                        .color(pal.text_main),
                                                                                );
                                                                                let sub = if !member
                                                                                    .custom_title
                                                                                    .is_empty()
                                                                                {
                                                                                    format!(
                                                                                        "{} • {}",
                                                                                        member
                                                                                            .role
                                                                                            .label(),
                                                                                        member
                                                                                            .custom_title
                                                                                    )
                                                                                } else {
                                                                                    member
                                                                                        .role
                                                                                        .label()
                                                                                        .to_string()
                                                                                };
                                                                                ui.label(
                                                                                    egui::RichText::new(
                                                                                        sub,
                                                                                    )
                                                                                        .size(12.0)
                                                                                        .color(pal.accent_light),
                                                                                );
                                                                            });
                                                                            ui.with_layout(
                                                                                egui::Layout::right_to_left(
                                                                                    egui::Align::Center,
                                                                                ),
                                                                                |ui| {
                                                                                    if ui
                                                                                        .add(
                                                                                            egui::Button::new(
                                                                                                egui::RichText::new(
                                                                                                    "Права ›",
                                                                                                )
                                                                                                    .size(12.5)
                                                                                                    .color(
                                                                                                        pal.accent_light,
                                                                                                    ),
                                                                                            )
                                                                                                .fill(pal.bg_panel)
                                                                                                .rounding(10.0),
                                                                                        )
                                                                                        .clicked()
                                                                                    {
                                                                                        self.editing_admin_idx =
                                                                                            Some(m_idx);
                                                                                    }
                                                                                },
                                                                            );
                                                                        });
                                                                    })
                                                                    .response
                                                                    .interact(egui::Sense::click());
                                                                if card.clicked() {
                                                                    self.editing_admin_idx =
                                                                        Some(m_idx);
                                                                }
                                                                ui.add_space(6.0);
                                                            }
                                                            ui.add_space(8.0);
                                                            if ui
                                                                .add_sized(
                                                                    Vec2::new(220.0, 34.0),
                                                                    egui::Button::new(
                                                                        egui::RichText::new(
                                                                            "+ Назначить из участников",
                                                                        )
                                                                            .color(Color32::WHITE),
                                                                    )
                                                                        .fill(pal.accent_blue)
                                                                        .rounding(17.0),
                                                                )
                                                                .clicked()
                                                            {
                                                                self.group_tab = GroupTab::Members;
                                                            }
                                                        }
                                                    }
                                                    GroupTab::Permissions => {
                                                        ui.label(
                                                            egui::RichText::new(
                                                                "Что могут делать обычные участники группы?",
                                                            )
                                                                .font(semibold_font(13.5))
                                                                .color(pal.accent_light),
                                                        );
                                                        ui.add_space(10.0);
                                                        let mut perm_row = |ui: &mut egui::Ui,
                                                                            title: &str,
                                                                            flag: &mut bool| {
                                                            ui.horizontal(|ui| {
                                                                ui.label(
                                                                    egui::RichText::new(title)
                                                                        .size(14.0)
                                                                        .color(pal.text_main),
                                                                );
                                                                ui.with_layout(
                                                                    egui::Layout::right_to_left(
                                                                        egui::Align::Center,
                                                                    ),
                                                                    |ui| {
                                                                        draw_toggle_switch(
                                                                            ui, flag, &pal,
                                                                        );
                                                                    },
                                                                );
                                                            });
                                                            ui.add_space(8.0);
                                                        };
                                                        perm_row(
                                                            ui,
                                                            "Отправка текстовых сообщений",
                                                            &mut chat.permissions.send_messages,
                                                        );
                                                        perm_row(
                                                            ui,
                                                            "Отправка фото, видео и файлов",
                                                            &mut chat.permissions.send_media,
                                                        );
                                                        perm_row(
                                                            ui,
                                                            "Создание опросов",
                                                            &mut chat.permissions.send_polls,
                                                        );
                                                        perm_row(
                                                            ui,
                                                            "Добавление новых участников",
                                                            &mut chat.permissions.invite_members,
                                                        );
                                                        ui.add_space(14.0);
                                                        ui.label(
                                                            egui::RichText::new(
                                                                "Медленный режим (Slow Mode)",
                                                            )
                                                                .font(semibold_font(13.5))
                                                                .color(pal.accent_light),
                                                        );
                                                        ui.add_space(8.0);
                                                        ui.horizontal(|ui| {
                                                            for (secs, lbl) in [
                                                                (0, "Выкл"),
                                                                (10, "10 сек"),
                                                                (30, "30 сек"),
                                                                (60, "1 мин"),
                                                            ] {
                                                                let active =
                                                                    chat.permissions.slow_mode_secs
                                                                        == secs;
                                                                if ui
                                                                    .add_sized(
                                                                        Vec2::new(82.0, 30.0),
                                                                        egui::Button::new(
                                                                            egui::RichText::new(lbl)
                                                                                .size(12.5)
                                                                                .color(if active {
                                                                                    Color32::WHITE
                                                                                } else {
                                                                                    pal.text_muted
                                                                                }),
                                                                        )
                                                                            .fill(if active {
                                                                                pal.accent_blue
                                                                            } else {
                                                                                pal.bg_input
                                                                            })
                                                                            .rounding(15.0),
                                                                    )
                                                                    .clicked()
                                                                {
                                                                    chat.permissions.slow_mode_secs =
                                                                        secs;
                                                                }
                                                            }
                                                        });
                                                    }
                                                },
                                            );
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
                            egui::Frame::none()
                                .inner_margin(egui::Margin::same(12.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Новый опрос")
                                                .font(semibold_font(17.0))
                                                .color(pal.text_main),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("✕")
                                                                .color(pal.text_muted),
                                                        )
                                                            .fill(Color32::TRANSPARENT)
                                                            .rounding(14.0),
                                                    )
                                                    .clicked()
                                                {
                                                    self.active_modal = ActiveModal::None;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(10.0);
                                    ui.label(
                                        egui::RichText::new("Вопрос")
                                            .size(12.5)
                                            .color(pal.text_muted),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.poll_question)
                                            .hint_text("Во сколько катаем?")
                                            .desired_width(292.0)
                                            .margin(Vec2::new(10.0, 8.0)),
                                    );
                                    ui.add_space(8.0);
                                    ui.label(
                                        egui::RichText::new("Варианты ответа")
                                            .size(12.5)
                                            .color(pal.text_muted),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.poll_option_1)
                                            .hint_text("Вариант 1")
                                            .desired_width(292.0)
                                            .margin(Vec2::new(10.0, 8.0)),
                                    );
                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.poll_option_2)
                                            .hint_text("Вариант 2")
                                            .desired_width(292.0)
                                            .margin(Vec2::new(10.0, 8.0)),
                                    );
                                    ui.add_space(14.0);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_sized(
                                                    Vec2::new(110.0, 34.0),
                                                    egui::Button::new(
                                                        egui::RichText::new("Создать")
                                                            .color(Color32::WHITE),
                                                    )
                                                        .fill(pal.accent_blue)
                                                        .rounding(17.0),
                                                )
                                                .clicked()
                                            {
                                                let q = self.poll_question.trim().to_string();
                                                if !q.is_empty() {
                                                    let o1 = if self.poll_option_1.trim().is_empty() {
                                                        "Да".into()
                                                    } else {
                                                        self.poll_option_1.trim().to_string()
                                                    };
                                                    let o2 = if self.poll_option_2.trim().is_empty() {
                                                        "Нет".into()
                                                    } else {
                                                        self.poll_option_2.trim().to_string()
                                                    };
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
                                        },
                                    );
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
                    if let Some(sc_id) = removed.server_chat_id {
                        self.send_delete_group(sc_id);
                    }
                    if self.selected_chat == Some(del_idx) {
                        self.selected_chat = if self.chats.is_empty() {
                            None
                        } else {
                            Some(0)
                        };
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
                                    ui.label(
                                        egui::RichText::new(msg)
                                            .size(13.0)
                                            .color(with_alpha(Color32::WHITE, alpha)),
                                    );
                                });
                        });
                } else {
                    self.toast = None;
                }
            }

            self.show_call_window(ctx);
            self.show_screen_fullscreen_overlay(ctx);
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
