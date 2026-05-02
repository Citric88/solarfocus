//! # SolarFocus Intelligence
//!
//! Trait-only contracts for AI capabilities: coaching, distraction classification,
//! and daily summarization. **Zero infra dependencies** — implementations live in
//! `apps/desktop/src/infra/{llm,classifier,window_watch}.rs`.
//!
//! Phase 1 of v1.2 ships the traits + mock impls. Real LLM lands in Phase 3.

pub mod traits;
pub mod types;
pub mod prompts;
pub mod mock;

pub use traits::{AiError, AiFuture, AiResult, Coach, DistractionClassifier, Summarizer};
pub use types::*;
pub use mock::{MockCoach, MockClassifier, MockSummarizer};
