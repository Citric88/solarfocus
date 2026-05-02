#![allow(dead_code)] // Phase 2 stub.

//! # Encriptación (Phase 2 — pendiente)
//!
//! Stub que indica si el usuario tiene activado `ENCRYPT_LOGS=true`.
//! La implementación real (AES-GCM) se introducirá en phase 2 junto con
//! el dep `aes-gcm`. Por ahora la función solo lee la variable de entorno.

pub fn is_encryption_enabled() -> bool {
    std::env::var("ENCRYPT_LOGS").is_ok_and(|v| v == "true")
}
