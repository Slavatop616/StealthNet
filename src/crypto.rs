use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use std::{fs, path::Path};
use x25519_dalek::{PublicKey, StaticSecret};

pub struct Identity {
    secret: StaticSecret,
    public: PublicKey,
}

impl Identity {
    pub fn load_or_generate(path: &Path) -> Result<Self> {
        if path.exists() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("failed to read identity key {}", path.display()))?;
            let bytes = STANDARD
                .decode(raw.trim())
                .with_context(|| format!("identity key is not valid base64 in {}", path.display()))?;
            if bytes.len() != 32 {
                return Err(anyhow!("identity key must decode to 32 bytes"));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            let secret = StaticSecret::from(key);
            let public = PublicKey::from(&secret);
            Ok(Self { secret, public })
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            let mut key = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            let secret = StaticSecret::from(key);
            let public = PublicKey::from(&secret);
            fs::write(path, STANDARD.encode(key))
                .with_context(|| format!("failed to write identity key {}", path.display()))?;
            Ok(Self { secret, public })
        }
    }

    pub fn public_key_base64(&self) -> String {
        STANDARD.encode(self.public.as_bytes())
    }

    pub fn derive_shared_key(&self, peer_public_b64: &str, context: &[u8]) -> Result<[u8; 32]> {
        let raw = STANDARD
            .decode(peer_public_b64.trim())
            .context("peer public key is not valid base64")?;
        if raw.len() != 32 {
            return Err(anyhow!("peer public key must decode to 32 bytes"));
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&raw);
        let peer = PublicKey::from(bytes);
        let shared = self.secret.diffie_hellman(&peer);
        let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
        let mut out = [0u8; 32];
        hk.expand(context, &mut out)
            .map_err(|_| anyhow!("hkdf expand failed"))?;
        Ok(out)
    }
}

pub fn encrypt(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<([u8; 12], Vec<u8>)> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            chacha20poly1305::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| anyhow!("encryption failed"))?;
    Ok((nonce, ciphertext))
}

pub fn decrypt(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            chacha20poly1305::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| anyhow!("decryption failed"))?;
    Ok(plaintext)
}
