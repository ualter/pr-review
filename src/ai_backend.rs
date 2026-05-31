use anyhow::Result;

use crate::{
    artifacts::{ai_run_debug_mode, run_ai_tool_streaming},
    cli::AiTool,
    debug::DEBUG,
};

#[derive(Debug, Clone)]
pub enum AiEvent {
    Started,
    Status(String),
    TextDelta(String),
    Finished,
    Failed(String),
}

pub trait AiBackend {
    fn run_review(
        &self,
        prompt: &str,
        emit: &mut dyn FnMut(AiEvent),
    ) -> Result<String>;
}

pub struct CliAiBackend<'a> {
    tool: &'a AiTool,
}

impl<'a> CliAiBackend<'a> {
    pub fn new(tool: &'a AiTool) -> Self {
        Self { tool }
    }
}

impl AiBackend for CliAiBackend<'_> {
    fn run_review(
        &self,
        prompt: &str,
        emit: &mut dyn FnMut(AiEvent),
    ) -> Result<String> {
        emit(AiEvent::Started);
        emit(AiEvent::Status(format!(
            "{} is starting...",
            self.tool.display_name()
        )));

        if DEBUG {
            emit(AiEvent::Status(format!("AI: {}", self.tool.display_name())));
            emit(AiEvent::Status(format!("Prompt bytes: {}", prompt.len())));
            emit(AiEvent::Status(format!(
                "Execution mode: {}",
                ai_run_debug_mode(self.tool, prompt)
            )));
        }

        match run_ai_tool_streaming(self.tool, prompt, |chunk| {
            emit(AiEvent::TextDelta(chunk.to_string()));
        }) {
            Ok(run_result) => {
                emit(AiEvent::Finished);
                Ok(run_result.output)
            }
            Err(err) => {
                emit(AiEvent::Failed(err.to_string()));
                Err(err)
            }
        }
    }
}
