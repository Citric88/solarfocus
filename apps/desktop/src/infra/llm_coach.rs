#![cfg(feature = "llm")]
//! `Coach` and `Summarizer` impls backed by the local LLM.

use crate::infra::llm::LlmRuntime;
use solar_focus_intelligence::prompts::{coaching_llm_prompt, summary_canned};
use solar_focus_intelligence::*;
use std::sync::Arc;

pub struct LlmCoach {
    runtime: Arc<LlmRuntime>,
}

impl LlmCoach {
    pub fn new(runtime: Arc<LlmRuntime>) -> Self {
        Self { runtime }
    }
}

impl Coach for LlmCoach {
    fn coaching_message(
        &self,
        trigger: CoachingTrigger,
        ctx: &FocusContext,
    ) -> AiFuture<String> {
        let runtime = self.runtime.clone();
        let prompt = coaching_llm_prompt(trigger, ctx);
        Box::pin(async move {
            runtime
                .generate(prompt, 50)
                .await
                .map(|s| s.trim().to_string())
                .map_err(|e| AiError::Inference(e.to_string()))
        })
    }
    fn is_ready(&self) -> bool {
        true
    }
}

pub struct LlmSummarizer {
    runtime: Arc<LlmRuntime>,
}

impl LlmSummarizer {
    pub fn new(runtime: Arc<LlmRuntime>) -> Self {
        Self { runtime }
    }
}

impl Summarizer for LlmSummarizer {
    fn daily_summary(&self, ctx: &DaySummaryContext) -> AiFuture<String> {
        // Phase 3 ships a *constrained* summary: feed real numbers via canned
        // template + ask the LLM to add 1 reflective sentence. This keeps
        // the LLM from hallucinating stats.
        let runtime = self.runtime.clone();
        let stats_line = summary_canned(ctx);
        let language = ctx.language;
        let prompt = match language {
            Language::Es => format!(
                "<|im_start|>system\nEres un coach de productividad. Responde en español, máximo 1 frase de cierre, sin emojis.<|im_end|>\n<|im_start|>user\nEstadísticas de hoy:\n{}\n\nEscribe una frase corta de cierre.<|im_end|>\n<|im_start|>assistant\n",
                stats_line
            ),
            Language::En => format!(
                "<|im_start|>system\nYou are a productivity coach. Reply in English, 1 short closing sentence, no emojis.<|im_end|>\n<|im_start|>user\nToday's stats:\n{}\n\nWrite a short closing sentence.<|im_end|>\n<|im_start|>assistant\n",
                stats_line
            ),
        };
        Box::pin(async move {
            let llm_text = runtime
                .generate(prompt, 60)
                .await
                .map(|s| s.trim().to_string())
                .map_err(|e| AiError::Inference(e.to_string()))?;
            Ok(format!("{}\n{}", stats_line, llm_text))
        })
    }
    fn is_ready(&self) -> bool {
        true
    }
}
