use axum::{
    body::Body,
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Multipart, Path, Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{broadcast, RwLock},
};
use uuid::Uuid;

const MAX_CALL_PARTICIPANTS: usize = 4;
const MAX_PENDING_AUTHS: usize = 500;
const MAX_WS_FRAME_BYTES: usize = 768 * 1024;
const MAX_MESSAGE_LEN: usize = 16_000;
const MAX_AUDIO_B64_LEN: usize = 16_384;
const MAX_SCREEN_B64_LEN: usize = 512 * 1024;

struct PendingAuth {
    created_at: Instant,
    completed: Option<CompletedUser>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CompletedUser {
    name: String,
    email: String,
    session_token: String,
    avatar_file_id: Option<String>,
    bio: String,
}

#[derive(Serialize)]
struct StartAuthResponse {
    state: String,
    auth_url: String,
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Deserialize)]
struct PollQuery {
    state: String,
}

#[derive(Serialize)]
struct PollResponse {
    status: String,
    name: Option<String>,
    email: Option<String>,
    session_token: Option<String>,
    avatar_file_id: Option<String>,
    bio: Option<String>,
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    name: Option<String>,
    email: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredAttachment {
    kind: String,
    title: String,
    subtitle: String,
    drive_file_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ReactionDto {
    emoji: String,
    users: Vec<String>,
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
    #[serde(default)]
    reply_to_id: Option<i64>,
    #[serde(default)]
    reply_to_sender: Option<String>,
    #[serde(default)]
    reply_to_preview: Option<String>,
    #[serde(default)]
    reactions: Vec<ReactionDto>,
    attachment: Option<StoredAttachment>,
}

#[derive(Clone, Serialize, Deserialize)]
struct GroupMemberDto {
    name: String,
    tag: String,
    email: String,
    avatar_file_id: Option<String>,
    bio: String,
    online: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct ChatDto {
    id: i64,
    invite_code: String,
    title: String,
    subtitle: String,
    avatar_tag: String,
    is_group: bool,
    description: String,
    avatar_file_id: Option<String>,
    peer_email: Option<String>,
    peer_avatar_id: Option<String>,
    peer_bio: String,
    peer_online: bool,
    #[serde(default)]
    creator_email: Option<String>,
    members: Vec<GroupMemberDto>,
    messages: Vec<ChatMessageDto>,
}

#[derive(Deserialize)]
struct AuthTokenQuery {
    session_token: String,
}

#[derive(Deserialize)]
struct OptionalTokenQuery {
    session_token: Option<String>,
}

#[derive(Deserialize)]
struct SearchUsersQuery {
    session_token: String,
    q: String,
}

#[derive(Serialize)]
struct UserSearchDto {
    name: String,
    tag: String,
    email: String,
    avatar_file_id: Option<String>,
    bio: String,
    online: bool,
}

#[derive(Serialize)]
struct SearchUsersResponse {
    users: Vec<UserSearchDto>,
    groups: Vec<ChatDto>,
}

#[derive(Deserialize)]
struct CreateChatRequest {
    session_token: String,
    title: String,
    description: String,
    avatar_tag: String,
    avatar_file_id: Option<String>,
    member_emails: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct InviteMemberRequest {
    session_token: String,
    chat_id: i64,
    target_query: String,
}

#[derive(Serialize)]
struct InviteMemberResponse {
    chat: ChatDto,
    added_member: GroupMemberDto,
}

#[derive(Deserialize)]
struct JoinChatRequest {
    session_token: String,
    chat_id: i64,
}

#[derive(Deserialize)]
struct OpenDmRequest {
    session_token: String,
    target_email: String,
}

#[derive(Serialize)]
struct SyncResponse {
    chats: Vec<ChatDto>,
    online_emails: Vec<String>,
}

#[derive(Serialize)]
struct UploadResponse {
    drive_file_id: String,
    title: String,
    subtitle: String,
    kind: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum WsEvent {
    #[serde(rename = "auth_ws")]
    AuthWs { session_token: String },
    #[serde(rename = "presence_update")]
    PresenceUpdate {
        email: String,
        online: bool,
        online_emails: Vec<String>,
    },
    #[serde(rename = "profile_updated")]
    ProfileUpdated {
        #[serde(default)]
        session_token: String,
        #[serde(default)]
        email: String,
        name: String,
        tag: String,
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
        caller_email: String,
        #[serde(default)]
        caller_name: String,
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

struct ActiveCall {
    call_id: String,
    is_group: bool,
    participants: HashSet<String>,
}

#[derive(Clone)]
struct ServerBroadcast {
    target_emails: Option<HashSet<String>>,
    exclude_email: Option<String>,
    payload: String,
}

struct AppState {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    upload_dir: PathBuf,
    http: reqwest::Client,
    pending: RwLock<HashMap<String, PendingAuth>>,
    online_counts: Mutex<HashMap<String, usize>>,
    active_calls: Mutex<HashMap<i64, ActiveCall>>,
    db: Mutex<Connection>,
    ws_tx: broadcast::Sender<ServerBroadcast>,
}

fn generate_group_code(chat_id: i64) -> String {
    let u = Uuid::new_v4();
    let bytes = u.as_bytes();
    let num: u32 = ((bytes[0] as u32) << 16)
        | ((bytes[1] as u32) << 8)
        | (bytes[2] as u32)
        ^ ((chat_id as u32).wrapping_mul(2654435761));
    format!("EGR-{:06}", num % 1_000_000)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.trim().chars().take(max_chars).collect()
}

fn extract_session_token(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    if let Some(val) = headers
        .get("x-session-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(val.to_string());
    }
    query_token
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn init_database(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS users (
            email TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            tag TEXT NOT NULL,
            avatar_file_id TEXT,
            bio TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS chats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            invite_code TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL,
            subtitle TEXT NOT NULL,
            avatar_tag TEXT NOT NULL,
            is_group INTEGER NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            avatar_file_id TEXT,
            creator_email TEXT
        );

        CREATE TABLE IF NOT EXISTS chat_members (
            chat_id INTEGER NOT NULL,
            user_email TEXT NOT NULL,
            PRIMARY KEY (chat_id, user_email)
        );

        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chat_id INTEGER NOT NULL,
            sender_email TEXT NOT NULL,
            sender_name TEXT NOT NULL,
            text TEXT NOT NULL,
            time TEXT NOT NULL,
            encrypted INTEGER NOT NULL DEFAULT 1,
            is_read INTEGER NOT NULL DEFAULT 0,
            reply_to_id INTEGER,
            reply_to_sender TEXT,
            reply_to_preview TEXT,
            att_kind TEXT,
            att_title TEXT,
            att_subtitle TEXT,
            att_drive_id TEXT
        );

        CREATE TABLE IF NOT EXISTS message_reactions (
            message_id INTEGER NOT NULL,
            user_email TEXT NOT NULL,
            emoji TEXT NOT NULL,
            PRIMARY KEY (message_id, user_email, emoji)
        );

        CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id, id);
        CREATE INDEX IF NOT EXISTS idx_chat_members_user ON chat_members(user_email);
        CREATE INDEX IF NOT EXISTS idx_sessions_email ON sessions(email);
        CREATE INDEX IF NOT EXISTS idx_reactions_msg ON message_reactions(message_id);
        ",
    )?;

    let _ = conn.execute("ALTER TABLE users ADD COLUMN avatar_file_id TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE users ADD COLUMN bio TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN invite_code TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE chats ADD COLUMN description TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute("ALTER TABLE chats ADD COLUMN avatar_file_id TEXT", []);
    let _ = conn.execute("ALTER TABLE chats ADD COLUMN creator_email TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE messages ADD COLUMN encrypted INTEGER NOT NULL DEFAULT 1",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE messages ADD COLUMN is_read INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN reply_to_id INTEGER", []);
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN reply_to_sender TEXT", []);
    let _ = conn.execute("ALTER TABLE messages ADD COLUMN reply_to_preview TEXT", []);

    if let Ok(mut stmt) = conn.prepare("SELECT id, invite_code FROM chats WHERE is_group = 1") {
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default()))
            })
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .collect();
        for (cid, code) in rows {
            if code.trim().is_empty() {
                let new_code = generate_group_code(cid);
                let _ = conn.execute(
                    "UPDATE chats SET invite_code = ?1 WHERE id = ?2",
                    params![new_code, cid],
                );
            }
        }
    }

    let _ = conn.execute(
        "DELETE FROM messages WHERE chat_id IN (SELECT id FROM chats WHERE id NOT IN (SELECT DISTINCT chat_id FROM chat_members))",
        [],
    );
    let _ = conn.execute(
        "DELETE FROM chats WHERE id NOT IN (SELECT DISTINCT chat_id FROM chat_members)",
        [],
    );

    Ok(())
}

fn load_reactions_for_message(db: &Connection, message_id: i64) -> Vec<ReactionDto> {
    let Ok(mut stmt) = db.prepare(
        "SELECT emoji, user_email FROM message_reactions WHERE message_id = ?1 ORDER BY rowid ASC",
    ) else {
        return Vec::new();
    };
    let mut grouped: Vec<ReactionDto> = Vec::new();
    if let Ok(rows) = stmt.query_map(params![message_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    }) {
        for (emoji, user_email) in rows.filter_map(Result::ok) {
            if let Some(existing) = grouped.iter_mut().find(|rx| rx.emoji == emoji) {
                if !existing.users.contains(&user_email) {
                    existing.users.push(user_email);
                }
            } else {
                grouped.push(ReactionDto {
                    emoji,
                    users: vec![user_email],
                });
            }
        }
    }
    grouped
}

impl AppState {
    fn online_set(&self) -> HashSet<String> {
        self.online_counts
            .lock()
            .map(|m| {
                m.iter()
                    .filter(|(_, &cnt)| cnt > 0)
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn send_ws(
        &self,
        target_emails: Option<HashSet<String>>,
        exclude_email: Option<String>,
        event: &WsEvent,
    ) {
        if let Ok(payload) = serde_json::to_string(event) {
            let _ = self.ws_tx.send(ServerBroadcast {
                target_emails,
                exclude_email,
                payload,
            });
        }
    }

    fn resolve_session_email(&self, session_token: &str) -> Option<(String, String)> {
        if session_token.trim().is_empty() {
            return None;
        }
        let db = self.db.lock().ok()?;
        db.query_row(
            "SELECT COALESCE(u.name, s.name), s.email
             FROM sessions s
             LEFT JOIN users u ON u.email = s.email
             WHERE s.token = ?1",
            params![session_token],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
            .ok()
    }

    fn verify_member_with_info(
        &self,
        session_token: &str,
        chat_id: i64,
    ) -> Option<(String, String, bool, HashSet<String>)> {
        let (name, email) = self.resolve_session_email(session_token)?;
        let db = self.db.lock().ok()?;

        let is_member: bool = db
            .query_row(
                "SELECT COUNT(*) FROM chat_members WHERE chat_id = ?1 AND user_email = ?2",
                params![chat_id, email],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !is_member {
            return None;
        }

        let is_group: bool = db
            .query_row(
                "SELECT is_group FROM chats WHERE id = ?1",
                params![chat_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|v| v != 0)
            .unwrap_or(false);

        let mut members = HashSet::new();
        if let Ok(mut stmt) =
            db.prepare("SELECT user_email FROM chat_members WHERE chat_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![chat_id], |r| r.get::<_, String>(0)) {
                for em in rows.filter_map(Result::ok) {
                    members.insert(em);
                }
            }
        }

        Some((name, email, is_group, members))
    }

    fn chat_member_emails(&self, chat_id: i64) -> HashSet<String> {
        let Ok(db) = self.db.lock() else {
            return HashSet::new();
        };
        let mut members = HashSet::new();
        if let Ok(mut stmt) =
            db.prepare("SELECT user_email FROM chat_members WHERE chat_id = ?1")
        {
            if let Ok(rows) = stmt.query_map(params![chat_id], |r| r.get::<_, String>(0)) {
                for em in rows.filter_map(Result::ok) {
                    members.insert(em);
                }
            }
        }
        members
    }
}

fn format_size(bytes: u64) -> String {
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

fn detect_kind(file_name: &str) -> &'static str {
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "mp4" | "mov" | "mkv" => "media",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => "music",
        _ => "document",
    }
}

fn sanitize_file_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("file.bin");
    let cleaned: String = base
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .take(120)
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "file.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

fn guess_content_type(file_id_or_name: &str) -> &'static str {
    let ext = file_id_or_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let client_id = env::var("GOOGLE_CLIENT_ID").expect("Не найден GOOGLE_CLIENT_ID в .env");
    let client_secret =
        env::var("GOOGLE_CLIENT_SECRET").expect("Не найден GOOGLE_CLIENT_SECRET в .env");
    let redirect_uri = env::var("REDIRECT_URI")
        .unwrap_or_else(|_| "https://185.70.186.123.nip.io/auth/google/callback".to_string());

    let upload_dir = env::var("UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("uploads"));

    fs::create_dir_all(&upload_dir)
        .await
        .expect("Не удалось создать папку для загрузок файлов");

    let conn = Connection::open("egramm.db").expect("Не удалось открыть базу egramm.db");
    init_database(&conn).expect("Ошибка инициализации таблиц SQLite");

    let (ws_tx, _) = broadcast::channel::<ServerBroadcast>(4096);

    let state = Arc::new(AppState {
        client_id,
        client_secret,
        redirect_uri,
        upload_dir,
        http: reqwest::Client::new(),
        pending: RwLock::new(HashMap::new()),
        online_counts: Mutex::new(HashMap::new()),
        active_calls: Mutex::new(HashMap::new()),
        db: Mutex::new(conn),
        ws_tx,
    });

    let app = Router::new()
        .route("/auth/google/start", post(start_google_auth))
        .route("/auth/google/callback", get(google_callback))
        .route("/auth/google/poll", get(poll_google_auth))
        .route("/api/sync", get(sync_chats))
        .route("/api/chats", post(create_chat))
        .route("/api/chats/invite", post(invite_member_to_chat))
        .route("/api/chats/join", post(join_chat_by_code))
        .route("/api/dm", post(open_dm_chat))
        .route("/api/users/search", get(search_users))
        .route("/files/upload", post(upload_file))
        .route("/files/:file_id", get(download_file))
        .route("/ws", get(ws_handler))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .with_state(state);

    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|_| panic!("Порт {} занят", bind_addr));
    println!("Egramm Server запущен на http://{}", bind_addr);
    axum::serve(listener, app).await.unwrap();
}

async fn start_google_auth(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StartAuthResponse>, StatusCode> {
    let auth_state = Uuid::new_v4().to_string();

    {
        let mut pending = state.pending.write().await;
        pending.retain(|_, entry| entry.created_at.elapsed().as_secs() <= 180);
        if pending.len() >= MAX_PENDING_AUTHS {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        pending.insert(
            auth_state.clone(),
            PendingAuth {
                created_at: Instant::now(),
                completed: None,
            },
        );
    }

    let encoded_redirect = state
        .redirect_uri
        .trim()
        .replace(':', "%3A")
        .replace('/', "%2F");

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}&prompt=select_account",
        state.client_id.trim(),
        encoded_redirect,
        auth_state
    );

    Ok(Json(StartAuthResponse {
        state: auth_state,
        auth_url,
    }))
}

async fn google_callback(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CallbackQuery>,
) -> Html<&'static str> {
    if !state.pending.read().await.contains_key(&query.state) {
        return Html("<h2>Сессия входа истекла или не найдена</h2>");
    }

    let token_res = state
        .http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", query.code.as_str()),
            ("client_id", state.client_id.trim()),
            ("client_secret", state.client_secret.trim()),
            ("redirect_uri", state.redirect_uri.trim()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;

    let Ok(resp) = token_res else {
        return Html("<h2>Ошибка соединения с Google</h2>");
    };
    let Ok(tokens) = resp.json::<GoogleTokenResponse>().await else {
        return Html("<h2>Не удалось получить токен Google</h2>");
    };

    let user_res = state
        .http
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&tokens.access_token)
        .send()
        .await;

    let Ok(u_resp) = user_res else {
        return Html("<h2>Ошибка получения профиля</h2>");
    };
    let Ok(user_info) = u_resp.json::<GoogleUserInfo>().await else {
        return Html("<h2>Не удалось прочитать профиль Google</h2>");
    };

    let google_name = user_info
        .name
        .unwrap_or_else(|| user_info.email.split('@').next().unwrap_or("User").to_string());
    let nick = user_info.email.split('@').next().unwrap_or("user");
    let default_tag = format!("@{}", nick);
    let session_token = Uuid::new_v4().to_string();

    let (final_name, existing_avatar, existing_bio): (String, Option<String>, String) =
        if let Ok(db) = state.db.lock() {
            let existing: Option<(String, Option<String>, String)> = db
                .query_row(
                    "SELECT name, avatar_file_id, bio FROM users WHERE email = ?1",
                    params![user_info.email],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        ))
                    },
                )
                .ok();

            let (name_to_use, av, bio) = match existing {
                Some((stored_name, av, bio)) if !stored_name.trim().is_empty() => {
                    (stored_name, av, bio)
                }
                Some((_, av, bio)) => (google_name.clone(), av, bio),
                None => {
                    let _ = db.execute(
                        "INSERT OR IGNORE INTO users (email, name, tag, avatar_file_id, bio) VALUES (?1, ?2, ?3, NULL, '')",
                        params![user_info.email, google_name, default_tag],
                    );
                    (google_name.clone(), None, String::new())
                }
            };

            let _ = db.execute(
                "INSERT OR REPLACE INTO sessions (token, name, email) VALUES (?1, ?2, ?3)",
                params![session_token, name_to_use, user_info.email],
            );
            (name_to_use, av, bio)
        } else {
            (google_name, None, String::new())
        };

    if let Some(entry) = state.pending.write().await.get_mut(&query.state) {
        entry.completed = Some(CompletedUser {
            name: final_name,
            email: user_info.email,
            session_token,
            avatar_file_id: existing_avatar,
            bio: existing_bio,
        });
    }

    Html(
        r#"<!DOCTYPE html>
<html lang="ru">
<head><meta charset="utf-8"><title>Egramm</title></head>
<body style="font-family: Segoe UI, sans-serif; background: #eef5fc; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0;">
  <div style="background: white; padding: 36px 44px; border-radius: 20px; box-shadow: 0 10px 30px rgba(30,80,150,0.08); text-align: center;">
    <div style="width: 56px; height: 56px; border-radius: 28px; background: #2b87e8; color: white; font-size: 28px; font-weight: bold; line-height: 56px; margin: 0 auto 16px;">E</div>
    <h2 style="color: #14263d; margin: 0 0 8px;">Вход выполнен!</h2>
    <p style="color: #647c99; margin: 0;">Можете закрыть эту вкладку и вернуться в приложение Egramm.</p>
  </div>
</body>
</html>"#,
    )
}

async fn poll_google_auth(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PollQuery>,
) -> Json<PollResponse> {
    let mut map = state.pending.write().await;

    if let Some(entry) = map.get(&query.state) {
        if entry.created_at.elapsed().as_secs() > 180 {
            map.remove(&query.state);
            return Json(PollResponse {
                status: "expired".into(),
                name: None,
                email: None,
                session_token: None,
                avatar_file_id: None,
                bio: None,
            });
        }

        if let Some(user) = entry.completed.clone() {
            map.remove(&query.state);
            return Json(PollResponse {
                status: "completed".into(),
                name: Some(user.name),
                email: Some(user.email),
                session_token: Some(user.session_token),
                avatar_file_id: user.avatar_file_id,
                bio: Some(user.bio),
            });
        }

        return Json(PollResponse {
            status: "pending".into(),
            name: None,
            email: None,
            session_token: None,
            avatar_file_id: None,
            bio: None,
        });
    }

    Json(PollResponse {
        status: "not_found".into(),
        name: None,
        email: None,
        session_token: None,
        avatar_file_id: None,
        bio: None,
    })
}

async fn search_users(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchUsersQuery>,
) -> Result<Json<SearchUsersResponse>, StatusCode> {
    let online_now = state.online_set();
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let my_email: String = db
        .query_row(
            "SELECT email FROM sessions WHERE token = ?1",
            params![query.session_token],
            |r| r.get(0),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let needle = truncate_chars(&query.q, 80).to_lowercase();
    if needle.is_empty() {
        return Ok(Json(SearchUsersResponse {
            users: Vec::new(),
            groups: Vec::new(),
        }));
    }

    let pattern = format!("%{}%", needle);
    let mut stmt = db
        .prepare(
            "SELECT name, tag, email, avatar_file_id, bio FROM users WHERE email != ?1 AND (LOWER(name) LIKE ?2 OR LOWER(tag) LIKE ?2 OR LOWER(email) LIKE ?2) LIMIT 20",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let users: Vec<UserSearchDto> = stmt
        .query_map(params![my_email, pattern], |r| {
            let em: String = r.get(2)?;
            Ok(UserSearchDto {
                name: r.get(0)?,
                tag: r.get(1)?,
                online: online_now.contains(&em),
                email: em,
                avatar_file_id: r.get(3)?,
                bio: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(Result::ok)
        .collect();

    let mut groups: Vec<ChatDto> = Vec::new();
    if let Ok(mut g_stmt) = db.prepare(
        "SELECT id FROM chats WHERE is_group = 1 AND (LOWER(invite_code) LIKE ?1 OR LOWER(title) LIKE ?1) LIMIT 15",
    ) {
        let g_ids: Vec<i64> = g_stmt
            .query_map(params![pattern], |r| r.get(0))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .collect();
        for gid in g_ids {
            if let Ok(dto) = load_single_chat_for_user(&db, gid, &my_email, &online_now) {
                groups.push(dto);
            }
        }
    }

    Ok(Json(SearchUsersResponse { users, groups }))
}

async fn join_chat_by_code(
    State(state): State<Arc<AppState>>,
    Json(req): Json<JoinChatRequest>,
) -> Result<Json<ChatDto>, (StatusCode, String)> {
    let online_now = state.online_set();
    let (dto, allowed_emails) = {
        let db = state
            .db
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка базы".into()))?;

        let my_email: String = db
            .query_row(
                "SELECT email FROM sessions WHERE token = ?1",
                params![req.session_token],
                |r| r.get(0),
            )
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Неверная сессия".into()))?;

        let is_grp: i64 = db
            .query_row(
                "SELECT is_group FROM chats WHERE id = ?1",
                params![req.chat_id],
                |r| r.get(0),
            )
            .map_err(|_| (StatusCode::NOT_FOUND, "Группа не найдена".into()))?;

        if is_grp == 0 {
            return Err((StatusCode::BAD_REQUEST, "Это личный чат".into()));
        }

        let _ = db.execute(
            "INSERT OR IGNORE INTO chat_members (chat_id, user_email) VALUES (?1, ?2)",
            params![req.chat_id, my_email],
        );

        let member_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM chat_members WHERE chat_id = ?1",
                params![req.chat_id],
                |r| r.get(0),
            )
            .unwrap_or(1);

        let _ = db.execute(
            "UPDATE chats SET subtitle = ?1 WHERE id = ?2",
            params![format!("{} участн.", member_count), req.chat_id],
        );

        let mut m_stmt = db
            .prepare("SELECT user_email FROM chat_members WHERE chat_id = ?1")
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let allowed: Vec<String> = m_stmt
            .query_map(params![req.chat_id], |r| r.get(0))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .filter_map(Result::ok)
            .collect();

        let chat_dto = load_single_chat_for_user(&db, req.chat_id, &my_email, &online_now)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        (chat_dto, allowed)
    };

    let targets: HashSet<String> = allowed_emails.iter().cloned().collect();
    state.send_ws(
        Some(targets),
        None,
        &WsEvent::NewChat {
            chat: dto.clone(),
            allowed_emails,
        },
    );

    Ok(Json(dto))
}

async fn open_dm_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenDmRequest>,
) -> Result<Json<ChatDto>, (StatusCode, String)> {
    let online_now = state.online_set();
    let (dto_for_me, peer_notify) = {
        let db = state
            .db
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка базы".into()))?;

        let (my_name, my_email): (String, String) = db
            .query_row(
                "SELECT COALESCE(u.name, s.name), s.email
                 FROM sessions s LEFT JOIN users u ON u.email = s.email
                 WHERE s.token = ?1",
                params![req.session_token],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Неверная сессия".into()))?;

        let target_email = req.target_email.trim().to_string();
        if target_email.is_empty() || target_email == my_email {
            return Err((StatusCode::BAD_REQUEST, "Некорректный получатель".into()));
        }

        let (my_avatar_id, my_bio): (Option<String>, String) = db
            .query_row(
                "SELECT avatar_file_id, bio FROM users WHERE email = ?1",
                params![my_email],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    ))
                },
            )
            .unwrap_or((None, String::new()));

        let (target_name, _target_tag, target_avatar_id, target_bio): (
            String,
            String,
            Option<String>,
            String,
        ) = db
            .query_row(
                "SELECT name, tag, avatar_file_id, bio FROM users WHERE email = ?1",
                params![target_email],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    ))
                },
            )
            .map_err(|_| (StatusCode::NOT_FOUND, "Пользователь не найден".into()))?;

        let existing_id: Option<i64> = db
            .query_row(
                "SELECT c.id FROM chats c
                 JOIN chat_members m1 ON c.id = m1.chat_id AND m1.user_email = ?1
                 JOIN chat_members m2 ON c.id = m2.chat_id AND m2.user_email = ?2
                 WHERE c.is_group = 0 LIMIT 1",
                params![my_email, target_email],
                |r| r.get(0),
            )
            .ok();

        if let Some(id) = existing_id {
            let dto = load_single_chat_for_user(&db, id, &my_email, &online_now)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            (dto, None)
        } else {
            let avatar_tag: String = target_name
                .chars()
                .take(2)
                .collect::<String>()
                .to_uppercase();
            let target_is_on = online_now.contains(&target_email);
            let sub_str = if target_is_on { "в сети" } else { "не в сети" };

            db.execute(
                "INSERT INTO chats (invite_code, title, subtitle, avatar_tag, is_group, description) VALUES ('', ?1, ?2, ?3, 0, '')",
                params![target_name, sub_str, avatar_tag],
            )
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            let id = db.last_insert_rowid();

            let _ = db.execute(
                "INSERT OR IGNORE INTO chat_members (chat_id, user_email) VALUES (?1, ?2)",
                params![id, my_email],
            );
            let _ = db.execute(
                "INSERT OR IGNORE INTO chat_members (chat_id, user_email) VALUES (?1, ?2)",
                params![id, target_email],
            );

            let dto_for_me = load_single_chat_for_user(&db, id, &my_email, &online_now)
                .unwrap_or(ChatDto {
                    id,
                    invite_code: String::new(),
                    title: target_name,
                    subtitle: sub_str.into(),
                    avatar_tag,
                    is_group: false,
                    description: String::new(),
                    avatar_file_id: target_avatar_id.clone(),
                    peer_email: Some(target_email.clone()),
                    peer_avatar_id: target_avatar_id,
                    peer_bio: target_bio,
                    peer_online: target_is_on,
                    creator_email: None,
                    members: Vec::new(),
                    messages: Vec::new(),
                });

            let my_is_on = online_now.contains(&my_email);
            let dto_for_peer = load_single_chat_for_user(&db, id, &target_email, &online_now)
                .unwrap_or(ChatDto {
                    id,
                    invite_code: String::new(),
                    title: my_name.clone(),
                    subtitle: if my_is_on {
                        "в сети".into()
                    } else {
                        "не в сети".into()
                    },
                    avatar_tag: my_name.chars().take(2).collect::<String>().to_uppercase(),
                    is_group: false,
                    description: String::new(),
                    avatar_file_id: my_avatar_id.clone(),
                    peer_email: Some(my_email.clone()),
                    peer_avatar_id: my_avatar_id,
                    peer_bio: my_bio,
                    peer_online: my_is_on,
                    creator_email: None,
                    members: Vec::new(),
                    messages: Vec::new(),
                });

            (dto_for_me, Some((target_email, dto_for_peer)))
        }
    };

    if let Some((target_email, dto_for_peer)) = peer_notify {
        let mut targets = HashSet::new();
        targets.insert(target_email.clone());
        state.send_ws(
            Some(targets),
            None,
            &WsEvent::NewChat {
                chat: dto_for_peer,
                allowed_emails: vec![target_email],
            },
        );
    }

    Ok(Json(dto_for_me))
}

async fn create_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateChatRequest>,
) -> Result<Json<ChatDto>, (StatusCode, String)> {
    let online_now = state.online_set();
    let title = truncate_chars(&req.title, 80);
    if title.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Название не может быть пустым".into(),
        ));
    }
    let description = truncate_chars(&req.description, 400);

    let (created_chat, allowed_emails) = {
        let db = state
            .db
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка базы".into()))?;

        let creator_email: String = db
            .query_row(
                "SELECT email FROM sessions WHERE token = ?1",
                params![req.session_token],
                |r| r.get(0),
            )
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Неверная сессия".into()))?;

        let mut members = vec![creator_email.clone()];
        if let Some(extra) = req.member_emails {
            for e in extra.into_iter().take(50) {
                let clean = e.trim().to_lowercase();
                let exists: bool = db
                    .query_row(
                        "SELECT COUNT(*) FROM users WHERE LOWER(email) = ?1",
                        params![clean],
                        |r| r.get::<_, i64>(0),
                    )
                    .map(|c| c > 0)
                    .unwrap_or(false);
                if exists && !members.iter().any(|m| m.to_lowercase() == clean) {
                    members.push(clean);
                }
            }
        }

        let subtitle = format!("{} участн.", members.len());
        let avatar_tag = if req.avatar_tag.trim().is_empty() {
            title.chars().take(2).collect::<String>().to_uppercase()
        } else {
            truncate_chars(&req.avatar_tag, 4)
        };

        db.execute(
            "INSERT INTO chats (invite_code, title, subtitle, avatar_tag, is_group, description, avatar_file_id, creator_email) VALUES ('', ?1, ?2, ?3, 1, ?4, ?5, ?6)",
            params![title, subtitle, avatar_tag, description, req.avatar_file_id, creator_email],
        )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let id = db.last_insert_rowid();
        let code = generate_group_code(id);
        let _ = db.execute(
            "UPDATE chats SET invite_code = ?1 WHERE id = ?2",
            params![code, id],
        );

        for email in &members {
            let _ = db.execute(
                "INSERT OR IGNORE INTO chat_members (chat_id, user_email) VALUES (?1, ?2)",
                params![id, email],
            );
        }

        let dto = load_single_chat_for_user(&db, id, &creator_email, &online_now)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        (dto, members)
    };

    let targets: HashSet<String> = allowed_emails.iter().cloned().collect();
    state.send_ws(
        Some(targets),
        None,
        &WsEvent::NewChat {
            chat: created_chat.clone(),
            allowed_emails,
        },
    );

    Ok(Json(created_chat))
}

async fn invite_member_to_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InviteMemberRequest>,
) -> Result<Json<InviteMemberResponse>, (StatusCode, String)> {
    let online_now = state.online_set();
    let needle = truncate_chars(&req.target_query, 100).to_lowercase();
    if needle.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Укажите @ник или почту".into()));
    }

    let (updated_chat, added_member, allowed_emails) = {
        let db = state
            .db
            .lock()
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Ошибка базы".into()))?;

        let my_email: String = db
            .query_row(
                "SELECT email FROM sessions WHERE token = ?1",
                params![req.session_token],
                |r| r.get(0),
            )
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Неверная сессия".into()))?;

        let in_chat: bool = db
            .query_row(
                "SELECT COUNT(*) FROM chat_members WHERE chat_id = ?1 AND user_email = ?2",
                params![req.chat_id, my_email],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if !in_chat {
            return Err((StatusCode::FORBIDDEN, "Вы не состоите в этой группе".into()));
        }

        let clean_needle = needle.trim_start_matches('@');
        let found: Option<(String, String, String, Option<String>, String)> = db
            .query_row(
                "SELECT name, tag, email, avatar_file_id, bio FROM users
                 WHERE LOWER(email) = ?1 OR LOWER(tag) = ?2 OR LOWER(tag) = ?3 OR LOWER(name) = ?1
                 LIMIT 1",
                params![needle, needle, format!("@{}", clean_needle)],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    ))
                },
            )
            .ok();

        let (u_name, u_tag, u_email, u_avatar, u_bio) = found.ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Пользователь не найден в Egramm".into(),
            )
        })?;

        let _ = db.execute(
            "INSERT OR IGNORE INTO chat_members (chat_id, user_email) VALUES (?1, ?2)",
            params![req.chat_id, u_email],
        );

        let member_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM chat_members WHERE chat_id = ?1",
                params![req.chat_id],
                |r| r.get(0),
            )
            .unwrap_or(1);
        let _ = db.execute(
            "UPDATE chats SET subtitle = ?1 WHERE id = ?2",
            params![format!("{} участн.", member_count), req.chat_id],
        );

        let mut m_stmt = db
            .prepare("SELECT user_email FROM chat_members WHERE chat_id = ?1")
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let allowed: Vec<String> = m_stmt
            .query_map(params![req.chat_id], |r| r.get(0))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .filter_map(Result::ok)
            .collect();

        let chat_dto = load_single_chat_for_user(&db, req.chat_id, &my_email, &online_now)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let added = GroupMemberDto {
            name: u_name,
            tag: u_tag,
            online: online_now.contains(&u_email),
            email: u_email,
            avatar_file_id: u_avatar,
            bio: u_bio,
        };

        (chat_dto, added, allowed)
    };

    let targets: HashSet<String> = allowed_emails.iter().cloned().collect();
    state.send_ws(
        Some(targets),
        None,
        &WsEvent::NewChat {
            chat: updated_chat.clone(),
            allowed_emails,
        },
    );

    Ok(Json(InviteMemberResponse {
        chat: updated_chat,
        added_member,
    }))
}

fn load_single_chat_for_user(
    db: &Connection,
    chat_id: i64,
    my_email: &str,
    online_now: &HashSet<String>,
) -> rusqlite::Result<ChatDto> {
    let (
        mut title,
        subtitle,
        mut avatar_tag,
        is_group_i64,
        description,
        group_avatar_id,
        invite_code,
        creator_email,
    ): (
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
        String,
        Option<String>,
    ) = db.query_row(
        "SELECT title, subtitle, avatar_tag, is_group, description, avatar_file_id, invite_code, creator_email FROM chats WHERE id = ?1",
        params![chat_id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                r.get(7)?,
            ))
        },
    )?;

    let is_group = is_group_i64 != 0;
    let final_code = if is_group && invite_code.trim().is_empty() {
        generate_group_code(chat_id)
    } else {
        invite_code
    };

    let mut peer_email = None;
    let mut peer_avatar_id = None;
    let mut peer_bio = String::new();
    let mut peer_online = false;

    if !is_group {
        if let Ok((p_name, p_email, p_avatar, p_bio)) = db.query_row(
            "SELECT u.name, u.email, u.avatar_file_id, u.bio FROM chat_members cm
             JOIN users u ON cm.user_email = u.email
             WHERE cm.chat_id = ?1 AND cm.user_email != ?2 LIMIT 1",
            params![chat_id, my_email],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            },
        ) {
            title = p_name.clone();
            avatar_tag = p_name.chars().take(2).collect::<String>().to_uppercase();
            peer_online = online_now.contains(&p_email);
            peer_email = Some(p_email);
            peer_avatar_id = p_avatar;
            peer_bio = p_bio;
        }
    }

    let mut members: Vec<GroupMemberDto> = Vec::new();
    if let Ok(mut mem_stmt) = db.prepare(
        "SELECT u.name, u.tag, u.email, u.avatar_file_id, u.bio
         FROM chat_members cm
         JOIN users u ON cm.user_email = u.email
         WHERE cm.chat_id = ?1",
    ) {
        if let Ok(rows) = mem_stmt.query_map(params![chat_id], |r| {
            let em: String = r.get(2)?;
            Ok(GroupMemberDto {
                name: r.get(0)?,
                tag: r.get(1)?,
                online: online_now.contains(&em),
                email: em,
                avatar_file_id: r.get(3)?,
                bio: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        }) {
            members = rows.filter_map(Result::ok).collect();
        }
    }

    let mut msg_stmt = db.prepare(
        "SELECT id, chat_id, sender_email, sender_name, text, time, encrypted, is_read, reply_to_id, reply_to_sender, reply_to_preview, att_kind, att_title, att_subtitle, att_drive_id
         FROM messages WHERE chat_id = ?1 ORDER BY id ASC",
    )?;

    let messages: Vec<ChatMessageDto> = msg_stmt
        .query_map(params![chat_id], |row| {
            let msg_id: i64 = row.get(0)?;
            let att_kind: Option<String> = row.get(11)?;
            let attachment = att_kind.map(|kind| StoredAttachment {
                kind,
                title: row
                    .get::<_, Option<String>>(12)
                    .unwrap_or_default()
                    .unwrap_or_default(),
                subtitle: row
                    .get::<_, Option<String>>(13)
                    .unwrap_or_default()
                    .unwrap_or_default(),
                drive_file_id: row.get(14).unwrap_or(None),
            });

            Ok(ChatMessageDto {
                id: msg_id,
                chat_id: row.get(1)?,
                sender_email: row.get(2)?,
                sender_name: row.get(3)?,
                text: row.get(4)?,
                time: row.get(5)?,
                encrypted: row.get::<_, i64>(6).unwrap_or(1) != 0,
                is_read: row.get::<_, i64>(7).unwrap_or(0) != 0,
                reply_to_id: row.get(8).unwrap_or(None),
                reply_to_sender: row.get(9).unwrap_or(None),
                reply_to_preview: row.get(10).unwrap_or(None),
                reactions: load_reactions_for_message(db, msg_id),
                attachment,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    Ok(ChatDto {
        id: chat_id,
        invite_code: final_code,
        title,
        subtitle,
        avatar_tag,
        is_group,
        description,
        avatar_file_id: group_avatar_id,
        peer_email,
        peer_avatar_id,
        peer_bio,
        peer_online,
        creator_email,
        members,
        messages,
    })
}

async fn sync_chats(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuthTokenQuery>,
) -> Result<Json<SyncResponse>, StatusCode> {
    let online_now = state.online_set();
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let my_email: String = db
        .query_row(
            "SELECT email FROM sessions WHERE token = ?1",
            params![query.session_token],
            |r| r.get(0),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let mut id_stmt = db
        .prepare("SELECT chat_id FROM chat_members WHERE user_email = ?1 ORDER BY chat_id ASC")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let chat_ids: Vec<i64> = id_stmt
        .query_map(params![my_email], |r| r.get(0))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(Result::ok)
        .collect();

    let mut chats = Vec::new();
    for cid in chat_ids {
        if let Ok(dto) = load_single_chat_for_user(&db, cid, &my_email, &online_now) {
            chats.push(dto);
        }
    }

    let online_emails: Vec<String> = online_now.into_iter().collect();

    Ok(Json(SyncResponse {
        chats,
        online_emails,
    }))
}

async fn upload_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<OptionalTokenQuery>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let token = extract_session_token(&headers, query.session_token.as_deref())
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Требуется авторизация".into()))?;
    if state.resolve_session_email(&token).is_none() {
        return Err((StatusCode::UNAUTHORIZED, "Недействительная сессия".into()));
    }

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        if field.name() == Some("file") {
            let raw_name = field.file_name().unwrap_or("file.bin").to_string();
            let safe_name = sanitize_file_name(&raw_name);
            let data = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

            let size_str = format_size(data.len() as u64);
            let kind = detect_kind(&safe_name).to_string();
            let stored_id = format!("{}_{}", Uuid::new_v4(), safe_name);
            let target_path = state.upload_dir.join(&stored_id);

            let mut out_file = fs::File::create(&target_path).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Ошибка создания файла на диске: {}", e),
                )
            })?;

            out_file.write_all(&data).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Ошибка записи файла на диске: {}", e),
                )
            })?;

            return Ok(Json(UploadResponse {
                drive_file_id: stored_id,
                title: safe_name,
                subtitle: size_str,
                kind,
            }));
        }
    }

    Err((StatusCode::BAD_REQUEST, "Поле file не найдено".into()))
}

async fn download_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<OptionalTokenQuery>,
    Path(file_id): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let token = extract_session_token(&headers, query.session_token.as_deref())
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Требуется авторизация".into()))?;
    if state.resolve_session_email(&token).is_none() {
        return Err((StatusCode::UNAUTHORIZED, "Недействительная сессия".into()));
    }

    if file_id.is_empty()
        || file_id.len() > 255
        || file_id.contains('/')
        || file_id.contains('\\')
        || file_id.contains("..")
        || file_id.contains('\0')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Некорректный идентификатор файла".into(),
        ));
    }

    let file_path = state.upload_dir.join(&file_id);
    let bytes = fs::read(&file_path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Файл не найден".into()))?;

    let content_type = guess_content_type(&file_id);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "private, max-age=86400")
        .body(Body::from(bytes))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, state))
}

fn broadcast_presence(state: &AppState, email: &str, online: bool) {
    let list: Vec<String> = state.online_set().into_iter().collect();
    state.send_ws(
        None,
        None,
        &WsEvent::PresenceUpdate {
            email: email.to_string(),
            online,
            online_emails: list,
        },
    );
}

fn leave_call_and_notify(state: &AppState, chat_id: i64, actor_email: &str) {
    let action: Option<(String, Vec<String>)> = {
        let Ok(mut calls) = state.active_calls.lock() else {
            return;
        };
        let Some(call) = calls.get_mut(&chat_id) else {
            return;
        };

        let call_id = call.call_id.clone();
        let was_in = call.participants.remove(actor_email);

        if was_in {
            if call.participants.len() <= 1 {
                calls.remove(&chat_id);
                Some((call_id, Vec::new()))
            } else {
                let remaining: Vec<String> = call.participants.iter().cloned().collect();
                Some((call_id, remaining))
            }
        } else if !call.is_group {
            calls.remove(&chat_id);
            Some((call_id, Vec::new()))
        } else {
            None
        }
    };

    if let Some((call_id, remaining)) = action {
        let targets = state.chat_member_emails(chat_id);
        state.send_ws(
            Some(targets.clone()),
            Some(actor_email.to_string()),
            &WsEvent::CallScreenStop {
                session_token: String::new(),
                chat_id,
                call_id: call_id.clone(),
                sender_email: actor_email.to_string(),
            },
        );
        state.send_ws(
            Some(targets),
            None,
            &WsEvent::CallHangup {
                session_token: String::new(),
                chat_id,
                call_id,
                actor_email: actor_email.to_string(),
                participants: remaining,
            },
        );
    }
}

fn bind_socket_session(state: &AppState, authed_email: &mut Option<String>, session_token: &str) {
    let Some((_, em)) = state.resolve_session_email(session_token) else {
        return;
    };
    if authed_email.as_deref() == Some(em.as_str()) {
        return;
    }
    if let Some(prev) = authed_email.take() {
        let mut still_on = false;
        if let Ok(mut map) = state.online_counts.lock() {
            if let Some(c) = map.get_mut(&prev) {
                *c = c.saturating_sub(1);
                still_on = *c > 0;
            }
        }
        if !still_on {
            broadcast_presence(state, &prev, false);
        }
    }
    if let Ok(mut map) = state.online_counts.lock() {
        let c = map.entry(em.clone()).or_insert(0);
        *c += 1;
    }
    *authed_email = Some(em.clone());
    broadcast_presence(state, &em, true);
}

async fn handle_ws_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.ws_tx.subscribe();
    let mut authed_email: Option<String> = None;
    let mut socket_call_chats: HashSet<i64> = HashSet::new();
    let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(10));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            maybe_msg = socket.recv() => {
                let msg = match maybe_msg {
                    Some(Ok(m)) => m,
                    _ => break,
                };
                if let WsMessage::Close(_) = msg {
                    break;
                }
                if let WsMessage::Ping(payload) = msg {
                    if socket.send(WsMessage::Pong(payload)).await.is_err() {
                        break;
                    }
                    continue;
                }
                if let WsMessage::Text(raw_text) = msg {
                    if raw_text.len() > MAX_WS_FRAME_BYTES {
                        continue;
                    }
                    if let Ok(event) = serde_json::from_str::<WsEvent>(&raw_text) {
                        match event {
                            WsEvent::AuthWs { session_token } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                            }
                            WsEvent::DeleteGroup { session_token, chat_id } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let deleted_targets = {
                                    let Some((_, _, is_group, members)) =
                                        state.verify_member_with_info(&session_token, chat_id)
                                    else {
                                        continue;
                                    };
                                    if !is_group {
                                        continue;
                                    }
                                    let Ok(db) = state.db.lock() else { continue };
                                    let _ = db.execute(
                                        "DELETE FROM message_reactions WHERE message_id IN (SELECT id FROM messages WHERE chat_id = ?1)",
                                        params![chat_id],
                                    );
                                    let _ = db.execute(
                                        "DELETE FROM messages WHERE chat_id = ?1",
                                        params![chat_id],
                                    );
                                    let _ = db.execute(
                                        "DELETE FROM chat_members WHERE chat_id = ?1",
                                        params![chat_id],
                                    );
                                    let ok = db
                                        .execute("DELETE FROM chats WHERE id = ?1", params![chat_id])
                                        .map(|rows| rows > 0)
                                        .unwrap_or(false);
                                    if ok { Some(members) } else { None }
                                };
                                if let Some(targets) = deleted_targets {
                                    if let Ok(mut calls) = state.active_calls.lock() {
                                        calls.remove(&chat_id);
                                    }
                                    state.send_ws(
                                        Some(targets),
                                        None,
                                        &WsEvent::ChatDeleted { chat_id },
                                    );
                                }
                            }
                            WsEvent::ProfileUpdated { session_token, name, tag, bio, .. } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let updated = {
                                    let Some((_, email)) = state.resolve_session_email(&session_token) else { continue };
                                    let Ok(db) = state.db.lock() else { continue };
                                    let clean_name = truncate_chars(&name, 64);
                                    let clean_tag = truncate_chars(&tag, 40);
                                    let clean_bio = truncate_chars(&bio, 300);
                                    if clean_name.is_empty() {
                                        continue;
                                    }
                                    let _ = db.execute(
                                        "UPDATE users SET name = ?1, tag = ?2, bio = ?3 WHERE email = ?4",
                                        params![clean_name, clean_tag, clean_bio, email],
                                    );
                                    let _ = db.execute(
                                        "UPDATE sessions SET name = ?1 WHERE email = ?2",
                                        params![clean_name, email],
                                    );
                                    (email, clean_name, clean_tag, clean_bio)
                                };
                                state.send_ws(
                                    None,
                                    None,
                                    &WsEvent::ProfileUpdated {
                                        session_token: String::new(),
                                        email: updated.0,
                                        name: updated.1,
                                        tag: updated.2,
                                        bio: updated.3,
                                    },
                                );
                            }
                            WsEvent::SendMessage {
                                session_token,
                                chat_id,
                                text,
                                encrypted,
                                reply_to_id,
                                reply_to_sender,
                                reply_to_preview,
                                attachment,
                            } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                if text.len() > MAX_MESSAGE_LEN {
                                    continue;
                                }
                                let is_enc = encrypted.unwrap_or(true);
                                let created = {
                                    let Some((sender_name, sender_email, _, members)) =
                                        state.verify_member_with_info(&session_token, chat_id)
                                    else {
                                        continue;
                                    };
                                    let Ok(db) = state.db.lock() else { continue };

                                    let time_str = Local::now().format("%H:%M").to_string();

                                    let (ak, at, asub, adrive) = match &attachment {
                                        Some(a) => (
                                            Some(a.kind.clone()),
                                            Some(a.title.clone()),
                                            Some(a.subtitle.clone()),
                                            a.drive_file_id.clone(),
                                        ),
                                        None => (None, None, None, None),
                                    };

                                    if db.execute(
                                        "INSERT INTO messages (chat_id, sender_email, sender_name, text, time, encrypted, is_read, reply_to_id, reply_to_sender, reply_to_preview, att_kind, att_title, att_subtitle, att_drive_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                                        params![
                                            chat_id,
                                            sender_email,
                                            sender_name,
                                            text,
                                            time_str,
                                            if is_enc { 1 } else { 0 },
                                            reply_to_id,
                                            reply_to_sender,
                                            reply_to_preview,
                                            ak,
                                            at,
                                            asub,
                                            adrive
                                        ],
                                    ).is_ok() {
                                        let id = db.last_insert_rowid();
                                        Some((
                                            ChatMessageDto {
                                                id,
                                                chat_id,
                                                sender_email,
                                                sender_name,
                                                text,
                                                time: time_str,
                                                encrypted: is_enc,
                                                is_read: false,
                                                reply_to_id,
                                                reply_to_sender,
                                                reply_to_preview,
                                                reactions: Vec::new(),
                                                attachment,
                                            },
                                            members,
                                        ))
                                    } else {
                                        None
                                    }
                                };

                                if let Some((new_msg, members)) = created {
                                    state.send_ws(
                                        Some(members),
                                        None,
                                        &WsEvent::NewMessage { message: new_msg },
                                    );
                                }
                            }
                            WsEvent::ToggleReaction {
                                session_token,
                                chat_id,
                                message_id,
                                emoji,
                            } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let clean_emoji = truncate_chars(&emoji, 16);
                                if clean_emoji.is_empty() {
                                    continue;
                                }
                                let updated = {
                                    let Some((_, user_email, _, members)) =
                                        state.verify_member_with_info(&session_token, chat_id)
                                    else {
                                        continue;
                                    };
                                    let Ok(db) = state.db.lock() else { continue };

                                    let msg_exists: bool = db
                                        .query_row(
                                            "SELECT COUNT(*) FROM messages WHERE id = ?1 AND chat_id = ?2",
                                            params![message_id, chat_id],
                                            |r| r.get::<_, i64>(0),
                                        )
                                        .map(|c| c > 0)
                                        .unwrap_or(false);
                                    if !msg_exists {
                                        continue;
                                    }

                                    let already: bool = db
                                        .query_row(
                                            "SELECT COUNT(*) FROM message_reactions WHERE message_id = ?1 AND user_email = ?2 AND emoji = ?3",
                                            params![message_id, user_email, clean_emoji],
                                            |r| r.get::<_, i64>(0),
                                        )
                                        .map(|c| c > 0)
                                        .unwrap_or(false);

                                    if already {
                                        let _ = db.execute(
                                            "DELETE FROM message_reactions WHERE message_id = ?1 AND user_email = ?2 AND emoji = ?3",
                                            params![message_id, user_email, clean_emoji],
                                        );
                                    } else {
                                        let _ = db.execute(
                                            "INSERT OR IGNORE INTO message_reactions (message_id, user_email, emoji) VALUES (?1, ?2, ?3)",
                                            params![message_id, user_email, clean_emoji],
                                        );
                                    }

                                    let reactions = load_reactions_for_message(&db, message_id);
                                    (reactions, members)
                                };

                                state.send_ws(
                                    Some(updated.1),
                                    None,
                                    &WsEvent::ReactionUpdated {
                                        chat_id,
                                        message_id,
                                        reactions: updated.0,
                                    },
                                );
                            }
                            WsEvent::MarkRead { session_token, chat_id } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let result = {
                                    let Some((_, reader, _, members)) =
                                        state.verify_member_with_info(&session_token, chat_id)
                                    else {
                                        continue;
                                    };
                                    let Ok(db) = state.db.lock() else { continue };

                                    let updated = db
                                        .execute(
                                            "UPDATE messages SET is_read = 1 WHERE chat_id = ?1 AND sender_email != ?2 AND is_read = 0",
                                            params![chat_id, reader],
                                        )
                                        .unwrap_or(0);

                                    if updated > 0 {
                                        Some((reader, members))
                                    } else {
                                        None
                                    }
                                };

                                if let Some((reader, members)) = result {
                                    state.send_ws(
                                        Some(members),
                                        None,
                                        &WsEvent::MessagesRead {
                                            chat_id,
                                            reader_email: reader,
                                        },
                                    );
                                }
                            }
                            WsEvent::DeleteMessage { session_token, chat_id, message_id, for_everyone } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                if !for_everyone {
                                    continue;
                                }
                                let deleted_members = {
                                    let Some((_, user_email, _, members)) =
                                        state.verify_member_with_info(&session_token, chat_id)
                                    else {
                                        continue;
                                    };
                                    let Ok(db) = state.db.lock() else { continue };

                                    let ok = db
                                        .execute(
                                            "DELETE FROM messages WHERE id = ?1 AND chat_id = ?2 AND sender_email = ?3",
                                            params![message_id, chat_id, user_email],
                                        )
                                        .map(|rows| rows > 0)
                                        .unwrap_or(false);
                                    if ok { Some(members) } else { None }
                                };

                                if let Some(members) = deleted_members {
                                    state.send_ws(
                                        Some(members),
                                        None,
                                        &WsEvent::MessageDeleted { chat_id, message_id },
                                    );
                                }
                            }
                            WsEvent::AvatarUpdated { session_token, avatar_file_id, .. } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let user_email = {
                                    let Some((_, email)) = state.resolve_session_email(&session_token) else { continue };
                                    let Ok(db) = state.db.lock() else { continue };
                                    let _ = db.execute(
                                        "UPDATE users SET avatar_file_id = ?1 WHERE email = ?2",
                                        params![avatar_file_id, email],
                                    );
                                    email
                                };
                                state.send_ws(
                                    None,
                                    None,
                                    &WsEvent::AvatarUpdated {
                                        session_token: String::new(),
                                        email: user_email,
                                        avatar_file_id,
                                    },
                                );
                            }
                            WsEvent::GroupAvatarUpdated { session_token, chat_id, avatar_file_id } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let members_opt = {
                                    let Some((_, _, _, members)) =
                                        state.verify_member_with_info(&session_token, chat_id)
                                    else {
                                        continue;
                                    };
                                    let Ok(db) = state.db.lock() else { continue };
                                    let _ = db.execute(
                                        "UPDATE chats SET avatar_file_id = ?1 WHERE id = ?2",
                                        params![avatar_file_id, chat_id],
                                    );
                                    Some(members)
                                };
                                if let Some(members) = members_opt {
                                    state.send_ws(
                                        Some(members),
                                        None,
                                        &WsEvent::GroupAvatarUpdated {
                                            session_token: String::new(),
                                            chat_id,
                                            avatar_file_id,
                                        },
                                    );
                                }
                            }
                            WsEvent::CallInvite { session_token, chat_id, call_id, .. } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let Some((caller_name, caller_email, is_group, members)) =
                                    state.verify_member_with_info(&session_token, chat_id)
                                else {
                                    continue;
                                };

                                enum InviteOutcome {
                                    Started(String, Vec<String>),
                                    JoinedExisting(String, Vec<String>),
                                    Full,
                                }

                                let outcome = {
                                    let Ok(mut calls) = state.active_calls.lock() else { continue };
                                    if let Some(existing) = calls.get_mut(&chat_id) {
                                        if !existing.participants.contains(&caller_email)
                                            && existing.participants.len() >= MAX_CALL_PARTICIPANTS
                                        {
                                            InviteOutcome::Full
                                        } else {
                                            existing.participants.insert(caller_email.clone());
                                            let list: Vec<String> = existing.participants.iter().cloned().collect();
                                            InviteOutcome::JoinedExisting(existing.call_id.clone(), list)
                                        }
                                    } else {
                                        let final_cid = if call_id.trim().is_empty() {
                                            Uuid::new_v4().to_string()
                                        } else {
                                            call_id
                                        };
                                        let mut set = HashSet::new();
                                        set.insert(caller_email.clone());
                                        calls.insert(
                                            chat_id,
                                            ActiveCall {
                                                call_id: final_cid.clone(),
                                                is_group,
                                                participants: set,
                                            },
                                        );
                                        InviteOutcome::Started(final_cid, vec![caller_email.clone()])
                                    }
                                };

                                match outcome {
                                    InviteOutcome::Full => {
                                        let mut only_caller = HashSet::new();
                                        only_caller.insert(caller_email);
                                        state.send_ws(
                                            Some(only_caller),
                                            None,
                                            &WsEvent::CallRejected {
                                                chat_id,
                                                reason: format!(
                                                    "Комната звонка заполнена (макс. {} участн.)",
                                                    MAX_CALL_PARTICIPANTS
                                                ),
                                            },
                                        );
                                    }
                                    InviteOutcome::JoinedExisting(existing_cid, participants) => {
                                        socket_call_chats.insert(chat_id);
                                        state.send_ws(
                                            Some(members),
                                            None,
                                            &WsEvent::CallAccept {
                                                session_token: String::new(),
                                                chat_id,
                                                call_id: existing_cid,
                                                peer_email: caller_email,
                                                participants,
                                            },
                                        );
                                    }
                                    InviteOutcome::Started(final_cid, participants) => {
                                        socket_call_chats.insert(chat_id);
                                        state.send_ws(
                                            Some(members),
                                            Some(caller_email.clone()),
                                            &WsEvent::CallInvite {
                                                session_token: String::new(),
                                                chat_id,
                                                call_id: final_cid,
                                                caller_email,
                                                caller_name,
                                                participants,
                                            },
                                        );
                                    }
                                }
                            }
                            WsEvent::CallAccept { session_token, chat_id, call_id, .. } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let Some((_, peer_email, _, members)) =
                                    state.verify_member_with_info(&session_token, chat_id)
                                else {
                                    continue;
                                };

                                enum AcceptOutcome {
                                    Joined(String, Vec<String>),
                                    Full,
                                    Ended,
                                }

                                let outcome = {
                                    let Ok(mut calls) = state.active_calls.lock() else { continue };
                                    if let Some(existing) = calls.get_mut(&chat_id) {
                                        if !existing.participants.contains(&peer_email)
                                            && existing.participants.len() >= MAX_CALL_PARTICIPANTS
                                        {
                                            AcceptOutcome::Full
                                        } else {
                                            existing.participants.insert(peer_email.clone());
                                            let list: Vec<String> = existing.participants.iter().cloned().collect();
                                            AcceptOutcome::Joined(existing.call_id.clone(), list)
                                        }
                                    } else {
                                        AcceptOutcome::Ended
                                    }
                                };

                                match outcome {
                                    AcceptOutcome::Joined(final_cid, participants) => {
                                        socket_call_chats.insert(chat_id);
                                        state.send_ws(
                                            Some(members),
                                            None,
                                            &WsEvent::CallAccept {
                                                session_token: String::new(),
                                                chat_id,
                                                call_id: final_cid,
                                                peer_email,
                                                participants,
                                            },
                                        );
                                    }
                                    AcceptOutcome::Full => {
                                        let mut only_peer = HashSet::new();
                                        only_peer.insert(peer_email);
                                        state.send_ws(
                                            Some(only_peer),
                                            None,
                                            &WsEvent::CallRejected {
                                                chat_id,
                                                reason: format!(
                                                    "Комната звонка заполнена (макс. {} участн.)",
                                                    MAX_CALL_PARTICIPANTS
                                                ),
                                            },
                                        );
                                    }
                                    AcceptOutcome::Ended => {
                                        let mut only_peer = HashSet::new();
                                        only_peer.insert(peer_email.clone());
                                        state.send_ws(
                                            Some(only_peer),
                                            None,
                                            &WsEvent::CallHangup {
                                                session_token: String::new(),
                                                chat_id,
                                                call_id,
                                                actor_email: peer_email,
                                                participants: Vec::new(),
                                            },
                                        );
                                    }
                                }
                            }
                            WsEvent::CallHangup { session_token, chat_id, .. } => {
                                bind_socket_session(&state, &mut authed_email, &session_token);
                                let Some((_, actor_email, _, _)) =
                                    state.verify_member_with_info(&session_token, chat_id)
                                else {
                                    continue;
                                };
                                socket_call_chats.remove(&chat_id);
                                leave_call_and_notify(&state, chat_id, &actor_email);
                            }
                            WsEvent::CallAudio { session_token, chat_id, call_id, sample_rate, channels, pcm_b64, .. } => {
                                if pcm_b64.len() > MAX_AUDIO_B64_LEN {
                                    continue;
                                }
                                if authed_email.is_none() {
                                    bind_socket_session(&state, &mut authed_email, &session_token);
                                }
                                let Some(sender_email) = authed_email.clone() else {
                                    continue;
                                };

                                let targets: Option<HashSet<String>> = state
                                    .active_calls
                                    .lock()
                                    .ok()
                                    .and_then(|calls| {
                                        let call = calls.get(&chat_id)?;
                                        if call.participants.contains(&sender_email) {
                                            Some(call.participants.clone())
                                        } else {
                                            None
                                        }
                                    });

                                if let Some(targets) = targets {
                                    state.send_ws(
                                        Some(targets),
                                        Some(sender_email.clone()),
                                        &WsEvent::CallAudio {
                                            session_token: String::new(),
                                            chat_id,
                                            call_id,
                                            sender_email,
                                            sample_rate,
                                            channels,
                                            pcm_b64,
                                        },
                                    );
                                }
                            }
                            WsEvent::CallScreenFrame {
                                session_token,
                                chat_id,
                                call_id,
                                sender_name,
                                width,
                                height,
                                jpeg_b64,
                                ..
                            } => {
                                if jpeg_b64.is_empty() || jpeg_b64.len() > MAX_SCREEN_B64_LEN {
                                    continue;
                                }
                                if authed_email.is_none() {
                                    bind_socket_session(&state, &mut authed_email, &session_token);
                                }
                                let Some(sender_email) = authed_email.clone() else {
                                    continue;
                                };

                                let targets: Option<HashSet<String>> = state
                                    .active_calls
                                    .lock()
                                    .ok()
                                    .and_then(|calls| {
                                        let call = calls.get(&chat_id)?;
                                        if call.participants.contains(&sender_email) {
                                            Some(call.participants.clone())
                                        } else {
                                            None
                                        }
                                    });

                                if let Some(targets) = targets {
                                    let clean_name = if sender_name.trim().is_empty() {
                                        sender_email.clone()
                                    } else {
                                        truncate_chars(&sender_name, 64)
                                    };
                                    state.send_ws(
                                        Some(targets),
                                        Some(sender_email.clone()),
                                        &WsEvent::CallScreenFrame {
                                            session_token: String::new(),
                                            chat_id,
                                            call_id,
                                            sender_email,
                                            sender_name: clean_name,
                                            width,
                                            height,
                                            jpeg_b64,
                                        },
                                    );
                                }
                            }
                            WsEvent::CallScreenStop {
                                session_token,
                                chat_id,
                                call_id,
                                ..
                            } => {
                                if authed_email.is_none() {
                                    bind_socket_session(&state, &mut authed_email, &session_token);
                                }
                                let Some(sender_email) = authed_email.clone() else {
                                    continue;
                                };

                                let targets: Option<HashSet<String>> = state
                                    .active_calls
                                    .lock()
                                    .ok()
                                    .and_then(|calls| {
                                        let call = calls.get(&chat_id)?;
                                        if call.participants.contains(&sender_email) {
                                            Some(call.participants.clone())
                                        } else {
                                            None
                                        }
                                    });

                                if let Some(targets) = targets {
                                    state.send_ws(
                                        Some(targets),
                                        Some(sender_email.clone()),
                                        &WsEvent::CallScreenStop {
                                            session_token: String::new(),
                                            chat_id,
                                            call_id,
                                            sender_email,
                                        },
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            recv_res = rx.recv() => {
                match recv_res {
                    Ok(broadcast_msg) => {
                        let Some(ref my_email) = authed_email else {
                            continue;
                        };
                        if broadcast_msg.exclude_email.as_deref() == Some(my_email.as_str()) {
                            continue;
                        }
                        if let Some(ref targets) = broadcast_msg.target_emails {
                            if !targets.contains(my_email) {
                                continue;
                            }
                        }
                        if socket.send(WsMessage::Text(broadcast_msg.payload)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            _ = ping_interval.tick() => {
                if socket.send(WsMessage::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
        }
    }

    if let Some(em) = authed_email {
        for chat_id in socket_call_chats {
            leave_call_and_notify(&state, chat_id, &em);
        }

        let mut still_online = false;
        if let Ok(mut map) = state.online_counts.lock() {
            if let Some(c) = map.get_mut(&em) {
                *c = c.saturating_sub(1);
                still_online = *c > 0;
            }
        }
        if !still_online {
            broadcast_presence(&state, &em, false);
        }
    }
}