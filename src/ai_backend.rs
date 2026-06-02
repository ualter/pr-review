use anyhow::Result;

use crate::{
    artifacts::{ai_run_debug_mode, run_ai_tool_streaming},
    cli::{AiRuntime, AiTool},
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
    fn run_review(&self, prompt: &str, emit: &mut dyn FnMut(AiEvent)) -> Result<String>;
}

pub fn backend_for_tool(runtime: &AiRuntime) -> Box<dyn AiBackend + '_> {
    match runtime.tool {
        AiTool::Copilot => Box::new(CliAiBackend::new(runtime)),
        #[cfg(feature = "copilot-sdk")]
        AiTool::CopilotSdk => Box::new(CopilotSdkBackend::new(runtime.model.clone())),
        AiTool::Codex => Box::new(CliAiBackend::new(runtime)),
    }
}

pub struct CliAiBackend<'a> {
    runtime: &'a AiRuntime,
}

impl<'a> CliAiBackend<'a> {
    pub fn new(runtime: &'a AiRuntime) -> Self {
        Self { runtime }
    }
}

impl AiBackend for CliAiBackend<'_> {
    fn run_review(&self, prompt: &str, emit: &mut dyn FnMut(AiEvent)) -> Result<String> {
        emit(AiEvent::Started);
        emit(AiEvent::Status(format!(
            "{} is starting...",
            self.runtime.display_name()
        )));

        if DEBUG {
            emit(AiEvent::Status(format!("AI: {}", self.runtime.display_name())));
            emit(AiEvent::Status(format!("Model: {}", self.runtime.model)));
            emit(AiEvent::Status(format!("Prompt bytes: {}", prompt.len())));
            emit(AiEvent::Status(format!(
                "Execution mode: {}",
                ai_run_debug_mode(self.runtime, prompt)
            )));
        }

        match run_ai_tool_streaming(self.runtime, prompt, |chunk| {
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

#[cfg(feature = "copilot-sdk")]
pub struct CopilotSdkBackend {
    model: String,
}

#[cfg(feature = "copilot-sdk")]
impl CopilotSdkBackend {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

#[cfg(feature = "copilot-sdk")]
impl AiBackend for CopilotSdkBackend {
    fn run_review(&self, prompt: &str, emit: &mut dyn FnMut(AiEvent)) -> Result<String> {
        use std::{env, time::Duration};

        use anyhow::Context;
        use github_copilot_sdk::{
            generated::{
                session_events::{
                    AssistantIntentData, AssistantMessageData, AssistantMessageDeltaData,
                    AssistantReasoningData, AssistantReasoningDeltaData,
                    AssistantStreamingDeltaData, AssistantTurnStartData, ModelCallFailureData,
                    SessionErrorData, SessionInfoData, SessionWarningData,
                    ToolExecutionCompleteData, ToolExecutionProgressData, ToolExecutionStartData,
                },
                SessionEventType,
            },
            subscription::RecvError,
            Client, ClientOptions, SessionConfig,
        };
        use tokio::runtime::Builder;

        const SDK_WAIT_TIMEOUT_SECS: u64 = 60 * 20;

        emit(AiEvent::Started);
        emit(AiEvent::Status(
            "Copilot SDK session is starting...".to_string(),
        ));

        if DEBUG {
            emit(AiEvent::Status("AI: Copilot SDK".to_string()));
            emit(AiEvent::Status(format!("Model: {}", self.model)));
            emit(AiEvent::Status(format!("Prompt bytes: {}", prompt.len())));
            emit(AiEvent::Status(
                "Execution mode: copilot sdk streaming via session API".to_string(),
            ));
        }

        let prompt_owned = prompt.to_string();
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create Tokio runtime for Copilot SDK backend")?;

        // This block is necessary because the Copilot SDK client and session must be created and used within
        // the context of a Tokio runtime (async), but our main function is synchronous.
        // By creating a runtime here and blocking on the async review logic, we can keep the
        // AiBackend trait synchronous while still leveraging the async capabilities of the Copilot SDK.
        runtime.block_on(async {
            let mut client_options = ClientOptions::default();
            if let Ok(cwd) = env::current_dir() {
                client_options.cwd = cwd;
            }

            let client = Client::start(client_options)
                .await
                .context("Failed to start Copilot SDK client")?;

            let session_config = {
                let mut config = SessionConfig::default()
                    .with_client_name("pr-review")
                    .with_streaming(true);
                config.enable_config_discovery = Some(false);
                config.request_user_input = Some(false);
                config.request_permission = Some(false);
                config.request_exit_plan_mode = Some(false);
                config.request_auto_mode_switch = Some(false);
                config.request_elicitation = Some(false);
                config.include_sub_agent_streaming_events = Some(false);
                config.model = Some(self.model.clone());
                if let Ok(cwd) = env::current_dir() {
                    config.working_directory = Some(cwd);
                }
                config
            };

            let session = match client.create_session(session_config).await {
                Ok(session) => session,
                Err(err) => {
                    let _ = client.stop().await;
                    return Err(err).context("Failed to create Copilot SDK session");
                }
            };

            let mut events = session.subscribe();
            let mut wait_future = Box::pin(session.send_and_wait(
                github_copilot_sdk::types::MessageOptions::new(prompt_owned)
                    .with_wait_timeout(Duration::from_secs(SDK_WAIT_TIMEOUT_SECS)),
            ));

            let mut streamed_text = String::new();
            let mut emitted_any_delta = false;
            let mut announced_streaming = false;
            let mut announced_reasoning = false;
            let mut current_tool_name: Option<String> = None;
            let final_event = loop {
                tokio::select! {
                    result = &mut wait_future => {
                        match result {
                            Ok(event) => break event,
                            Err(err) => {
                                let _ = session.disconnect().await;
                                let _ = client.stop().await;
                                return Err(err).context("Copilot SDK review failed");
                            }
                        }
                    }
                    event = events.recv() => {
                        match event {
                            Ok(event) => match event.parsed_type() {
                                SessionEventType::AssistantMessageDelta => {
                                    if let Some(data) = event.typed_data::<AssistantMessageDeltaData>() {
                                        emitted_any_delta = true;
                                        streamed_text.push_str(&data.delta_content);
                                        emit(AiEvent::TextDelta(data.delta_content));
                                    }
                                }
                                SessionEventType::AssistantMessage => {
                                    if let Some(data) = event.typed_data::<AssistantMessageData>()
                                        && !emitted_any_delta
                                        && !data.content.is_empty()
                                    {
                                        streamed_text.push_str(&data.content);
                                        emit(AiEvent::TextDelta(data.content));
                                    }
                                }
                                SessionEventType::AssistantIntent => {
                                    if let Some(data) = event.typed_data::<AssistantIntentData>()
                                        && DEBUG
                                    {
                                        emit(AiEvent::Status(format!(
                                            "assistant.intent: {}",
                                            data.intent
                                        )));
                                    }
                                }
                                SessionEventType::AssistantTurnStart => {
                                    if let Some(data) = event.typed_data::<AssistantTurnStartData>() {
                                        if DEBUG {
                                            emit(AiEvent::Status(format!(
                                                "assistant.turn_start: {}",
                                                data.turn_id
                                            )));
                                        } else {
                                            emit(AiEvent::Status("Thinking...".to_string()));
                                        }
                                    } else {
                                        if DEBUG {
                                            emit(AiEvent::Status(
                                                "assistant.turn_start".to_string(),
                                            ));
                                        } else {
                                            emit(AiEvent::Status("Thinking...".to_string()));
                                        }
                                    }
                                }
                                SessionEventType::AssistantReasoning => {
                                    if DEBUG {
                                        if let Some(data) = event.typed_data::<AssistantReasoningData>() {
                                            let preview = summarize_sdk_status(&data.content, 120);
                                            if !preview.is_empty() {
                                                emit(AiEvent::Status(format!(
                                                    "assistant.reasoning: {}",
                                                    preview
                                                )));
                                            }
                                        }
                                    } else {
                                        announced_reasoning = true;
                                    }
                                }
                                SessionEventType::AssistantReasoningDelta
                                    if !announced_reasoning =>
                                {
                                    if DEBUG
                                        && let Some(data) =
                                            event.typed_data::<AssistantReasoningDeltaData>()
                                    {
                                        let preview =
                                            summarize_sdk_status(&data.delta_content, 120);
                                        if !preview.is_empty() {
                                            emit(AiEvent::Status(format!(
                                                "assistant.reasoning_delta: {}",
                                                preview
                                            )));
                                        }
                                    }
                                    announced_reasoning = true;
                                }
                                SessionEventType::AssistantStreamingDelta
                                    if !announced_streaming =>
                                {
                                    if DEBUG {
                                        if let Some(data) =
                                            event.typed_data::<AssistantStreamingDeltaData>()
                                        {
                                            emit(AiEvent::Status(format!(
                                                "assistant.streaming_delta: {:.0} bytes",
                                                data.total_response_size_bytes
                                            )));
                                        } else {
                                            emit(AiEvent::Status(
                                                "assistant.streaming_delta".to_string(),
                                            ));
                                        }
                                    } else {
                                        emit(AiEvent::Status(
                                            "Receiving response...".to_string(),
                                        ));
                                    }
                                    announced_streaming = true;
                                }
                                SessionEventType::SessionInfo => {
                                    if let Some(data) = event.typed_data::<SessionInfoData>()
                                        && DEBUG
                                    {
                                        let message = data
                                            .tip
                                            .map(|tip| format!("{} Tip: {}", data.message, tip))
                                            .unwrap_or(data.message);
                                        emit(AiEvent::Status(format!(
                                            "session.info: {}",
                                            message
                                        )));
                                    }
                                }
                                SessionEventType::SessionWarning => {
                                    if let Some(data) = event.typed_data::<SessionWarningData>()
                                        && DEBUG
                                    {
                                        emit(AiEvent::Status(format!(
                                            "session.warning: {}",
                                            data.message
                                        )));
                                    }
                                }
                                SessionEventType::SessionError => {
                                    if let Some(data) = event.typed_data::<SessionErrorData>() {
                                        if DEBUG {
                                            if event.is_transient_error() {
                                                emit(AiEvent::Status(format!(
                                                    "session.error(transient): {}",
                                                    data.message
                                                )));
                                            } else {
                                                emit(AiEvent::Status(format!(
                                                    "session.error: {}",
                                                    data.message
                                                )));
                                            }
                                        } else {
                                            emit(AiEvent::Status(
                                                "The SDK session reported an error.".to_string(),
                                            ));
                                        }
                                    }
                                }
                                SessionEventType::ToolExecutionStart => {
                                    if let Some(data) = event.typed_data::<ToolExecutionStartData>() {
                                        current_tool_name = Some(data.tool_name.clone());
                                        if DEBUG {
                                            emit(AiEvent::Status(format!(
                                                "tool.execution_start: {}",
                                                data.tool_name
                                            )));
                                        } else {
                                            emit(AiEvent::Status(format!(
                                                "Running tool: {}",
                                                data.tool_name
                                            )));
                                        }
                                    }
                                }
                                SessionEventType::ToolExecutionProgress => {
                                    if let Some(data) = event.typed_data::<ToolExecutionProgressData>() {
                                        if DEBUG {
                                            emit(AiEvent::Status(format!(
                                                "tool.execution_progress: {}",
                                                data.progress_message
                                            )));
                                        } else if let Some(tool_name) = current_tool_name.as_deref() {
                                            emit(AiEvent::Status(format!(
                                                "Running tool: {}",
                                                tool_name
                                            )));
                                        }
                                    }
                                }
                                SessionEventType::ToolExecutionComplete => {
                                    if let Some(data) = event.typed_data::<ToolExecutionCompleteData>() {
                                        let message = if data.success {
                                            let label = current_tool_name
                                                .take()
                                                .unwrap_or_else(|| data.tool_call_id.clone());
                                            format!("Finished tool: {}", label)
                                        } else {
                                            let error = data.error.map(|e| e.message).unwrap_or_else(|| "unknown tool failure".to_string());
                                            format!("Tool failed: {}", error)
                                        };
                                        if DEBUG {
                                            emit(AiEvent::Status(format!(
                                                "tool.execution_complete: {}",
                                                message
                                            )));
                                        } else {
                                            emit(AiEvent::Status(message));
                                        }
                                    }
                                }
                                SessionEventType::ModelCallFailure => {
                                    if let Some(data) = event.typed_data::<ModelCallFailureData>() {
                                        let message = data
                                            .error_message
                                            .unwrap_or_else(|| "model call failure".to_string());
                                        if DEBUG {
                                            emit(AiEvent::Status(format!(
                                                "model.call_failure: {}",
                                                message
                                            )));
                                        } else {
                                            emit(AiEvent::Status(
                                                "The model call failed.".to_string(),
                                            ));
                                        }
                                    }
                                }
                                SessionEventType::SessionCompactionStart => {
                                    if DEBUG {
                                        emit(AiEvent::Status(
                                            "session.compaction_start".to_string(),
                                        ));
                                    } else {
                                        emit(AiEvent::Status(
                                            "Compacting context...".to_string(),
                                        ));
                                    }
                                }
                                SessionEventType::SessionCompactionComplete => {
                                    if DEBUG {
                                        emit(AiEvent::Status(
                                            "session.compaction_complete".to_string(),
                                        ));
                                    } else {
                                        emit(AiEvent::Status(
                                            "Context compaction finished.".to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            },
                            Err(RecvError::Lagged(lagged)) => {
                                if DEBUG {
                                    emit(AiEvent::Status(format!(
                                        "subscription.lagged: skipped {} events",
                                        lagged.skipped()
                                    )));
                                }
                            }
                            Err(RecvError::Closed) => {
                                if DEBUG {
                                    emit(AiEvent::Status(
                                        "subscription.closed before session idle".to_string(),
                                    ));
                                }
                                match wait_future.as_mut().await {
                                    Ok(event) => break event,
                                    Err(err) => {
                                        let _ = session.disconnect().await;
                                        let _ = client.stop().await;
                                        return Err(err).context("Copilot SDK review failed after the event stream closed");
                                    }
                                }
                            }
                            Err(_) => {
                                if DEBUG {
                                    emit(AiEvent::Status(
                                        "subscription.recv_error".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                }
            };

            let result: Result<String> = if let Some(event) = final_event {
                if let Some(data) = event.typed_data::<AssistantMessageData>() {
                    Ok(data.content)
                } else if !streamed_text.is_empty() {
                    Ok(streamed_text)
                } else {
                    Ok(event
                        .data
                        .get("content")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string())
                }
            } else {
                Ok(streamed_text)
            };

            let disconnect_result = session.disconnect().await;
            let stop_result = client.stop().await;

            if let Err(err) = disconnect_result {
                emit(AiEvent::Status(format!(
                    "Copilot SDK session cleanup warning: {err}"
                )));
            }
            if let Err(err) = stop_result {
                emit(AiEvent::Status(format!(
                    "Copilot SDK client shutdown warning: {err}"
                )));
            }

            let output = result?;
            emit(AiEvent::Finished);
            Ok(output)
        })
    }
}

#[cfg(feature = "copilot-sdk")]
fn summarize_sdk_status(message: &str, max_chars: usize) -> String {
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= max_chars {
        compact
    } else {
        format!("{}...", &compact[..max_chars])
    }
}
