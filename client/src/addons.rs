use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ENC_V1_PREFIX: &str = "egramm_enc_v1:";
const ENC_V2_PREFIX: &str = "egramm_enc_v2:";
const AUDIO_V2_MAGIC: &[u8; 2] = b"E2";

// Звуки вшиваются прямо внутрь .exe при сборке
const NOTIFY_MP3: &[u8] = include_bytes!("../sounds/notify.mp3");
const CALL_MP3: &[u8] = include_bytes!("../sounds/call.mp3");

/// Надёжный 256-битный ключ чата через SHA-256
pub fn derive_chat_key(chat_id: i64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"egramm-v2-chacha20poly1305-chat-key::");
    hasher.update(chat_id.to_le_bytes());
    let digest = hasher.finalize();
    let mut key = [0_u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn legacy_chat_key_v1(chat_id: i64) -> [u8; 32] {
    let seed = format!("egramm-v1-stream-key-chat-{}", chat_id);
    let mut key = [0_u8; 32];
    let raw = seed.as_bytes();
    for i in 0..32 {
        let b = raw[i % raw.len()];
        key[i] = b.rotate_left((i % 7) as u32) ^ ((i as u8).wrapping_mul(37));
    }
    key
}

fn legacy_audio_xor_inplace(chat_id: i64, data: &mut [u8]) {
    let key = legacy_chat_key_v1(chat_id);
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= key[i % 32] ^ ((i as u8).wrapping_mul(19));
    }
}

/// Шифрование сообщений и метаданных вложений: ChaCha20-Poly1305 (AEAD)
pub fn encrypt_payload(chat_id: i64, plain: &str) -> String {
    if plain.is_empty()
        || plain.starts_with(ENC_V2_PREFIX)
        || plain.starts_with(ENC_V1_PREFIX)
    {
        return plain.to_string();
    }

    let key = derive_chat_key(chat_id);
    let Ok(cipher) = ChaCha20Poly1305::new_from_slice(&key) else {
        return plain.to_string();
    };

    let mut nonce_bytes = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let Ok(ciphertext) = cipher.encrypt(nonce, plain.as_bytes()) else {
        return plain.to_string();
    };

    let mut packed = Vec::with_capacity(12 + ciphertext.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ciphertext);

    format!("{}{}", ENC_V2_PREFIX, B64.encode(packed))
}

/// Расшифровка: поддерживает новый ChaCha20-Poly1305 (v2) и старые сообщения (v1)
pub fn decrypt_payload(chat_id: i64, encoded: &str) -> (String, bool) {
    if let Some(rest) = encoded.strip_prefix(ENC_V2_PREFIX) {
        let Ok(raw) = B64.decode(rest) else {
            return (encoded.to_string(), true);
        };
        if raw.len() < 12 + 16 {
            return (encoded.to_string(), true);
        }
        let key = derive_chat_key(chat_id);
        let Ok(cipher) = ChaCha20Poly1305::new_from_slice(&key) else {
            return (encoded.to_string(), true);
        };
        let nonce = Nonce::from_slice(&raw[..12]);
        let Ok(plain_bytes) = cipher.decrypt(nonce, &raw[12..]) else {
            return (encoded.to_string(), true);
        };
        return match String::from_utf8(plain_bytes) {
            Ok(s) => (s, true),
            Err(_) => (encoded.to_string(), true),
        };
    }

    if let Some(rest) = encoded.strip_prefix(ENC_V1_PREFIX) {
        let Ok(raw) = B64.decode(rest) else {
            return (encoded.to_string(), true);
        };
        if raw.len() < 12 {
            return (encoded.to_string(), true);
        }
        let nonce = &raw[..12];
        let cipher = &raw[12..];
        let key = legacy_chat_key_v1(chat_id);

        let mut plain_bytes = Vec::with_capacity(cipher.len());
        for (i, &byte) in cipher.iter().enumerate() {
            let k_byte = key[i % 32] ^ nonce[i % 12] ^ ((i as u8).wrapping_mul(13));
            plain_bytes.push(byte ^ k_byte);
        }
        return match String::from_utf8(plain_bytes) {
            Ok(s) => (s, true),
            Err(_) => (encoded.to_string(), true),
        };
    }

    (encoded.to_string(), false)
}

/// Шифрование голосовых пакетов: ChaCha20-Poly1305 на каждый фрейм
pub fn encrypt_audio_frame(chat_id: i64, pcm: &[u8]) -> Vec<u8> {
    let key = derive_chat_key(chat_id);
    let Ok(cipher) = ChaCha20Poly1305::new_from_slice(&key) else {
        return pcm.to_vec();
    };

    let mut nonce_bytes = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let Ok(ciphertext) = cipher.encrypt(nonce, pcm) else {
        return pcm.to_vec();
    };

    let mut out = Vec::with_capacity(2 + 12 + ciphertext.len());
    out.extend_from_slice(AUDIO_V2_MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    out
}

/// Расшифровка голосовых пакетов (v2 AEAD + обратная совместимость с v1)
pub fn decrypt_audio_frame(chat_id: i64, data: &[u8]) -> Option<Vec<u8>> {
    if data.starts_with(AUDIO_V2_MAGIC) && data.len() >= 2 + 12 + 16 {
        let key = derive_chat_key(chat_id);
        let cipher = ChaCha20Poly1305::new_from_slice(&key).ok()?;
        let nonce = Nonce::from_slice(&data[2..14]);
        return cipher.decrypt(nonce, &data[14..]).ok();
    }

    if data.len() < 2 {
        return None;
    }
    let mut legacy = data.to_vec();
    legacy_audio_xor_inplace(chat_id, &mut legacy);
    Some(legacy)
}

/// Плеер встроенных звуков уведомлений и звонка
pub struct SoundPlayer {
    ringtone_stop: Mutex<Option<Arc<AtomicBool>>>,
}

impl SoundPlayer {
    pub fn new() -> Self {
        Self {
            ringtone_stop: Mutex::new(None),
        }
    }

    pub fn play_notify(&self) {
        std::thread::spawn(|| {
            let Ok((_stream, handle)) = rodio::OutputStream::try_default() else {
                return;
            };
            let Ok(sink) = rodio::Sink::try_new(&handle) else {
                return;
            };
            let Ok(source) = rodio::Decoder::new(Cursor::new(NOTIFY_MP3)) else {
                return;
            };
            sink.set_volume(0.85);
            sink.append(source);
            sink.sleep_until_end();
        });
    }

    pub fn start_ringtone(&self) {
        self.stop_ringtone();
        let stop_flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut guard) = self.ringtone_stop.lock() {
            *guard = Some(stop_flag.clone());
        }

        std::thread::spawn(move || {
            let Ok((_stream, handle)) = rodio::OutputStream::try_default() else {
                return;
            };
            let Ok(sink) = rodio::Sink::try_new(&handle) else {
                return;
            };
            sink.set_volume(0.8);

            while !stop_flag.load(Ordering::Relaxed) {
                let Ok(source) = rodio::Decoder::new(Cursor::new(CALL_MP3)) else {
                    break;
                };
                sink.append(source);
                while !stop_flag.load(Ordering::Relaxed) && !sink.empty() {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            sink.stop();
        });
    }

    pub fn stop_ringtone(&self) {
        if let Ok(mut guard) = self.ringtone_stop.lock() {
            if let Some(flag) = guard.take() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for SoundPlayer {
    fn drop(&mut self) {
        self.stop_ringtone();
    }
}