//! Public contract for AI providers. Async via `Pin<Box<dyn Future>>` so the
//! UI can call `Task::perform` and never block.

use crate::types::*;
use std::future::Future;
use std::pin::Pin;

pub type AiResult<T> = Result<T, AiError>;
pub type AiFuture<T> = Pin<Box<dyn Future<Output = AiResult<T>> + Send + 'static>>;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("model not loaded")]
    ModelNotLoaded,
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("disabled by user")]
    Disabled,
    #[error("io: {0}")]
    Io(String),
}

/// Mensajes de coaching cortos para los puntos clave de la sesión.
pub trait Coach: Send + Sync {
    fn coaching_message(&self, trigger: CoachingTrigger, ctx: &FocusContext) -> AiFuture<String>;
    fn is_ready(&self) -> bool;
}

/// Clasifica si el contexto activo (proceso + título) es trabajo o distracción.
pub trait DistractionClassifier: Send + Sync {
    fn classify(&self, sample: &WindowSample) -> AiFuture<ClassificationResult>;
    fn is_ready(&self) -> bool;
}

/// Resumen del día (fin de jornada).
pub trait Summarizer: Send + Sync {
    fn daily_summary(&self, ctx: &DaySummaryContext) -> AiFuture<String>;
    fn is_ready(&self) -> bool;
}
