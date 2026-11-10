//! The thread header (UI-030): title, tags, workspace chips, model and thinking selectors, context usage, session cost,
//! and process state.

use omfp_core::wire::{AgentMessage, ThinkingLevel};
use omfp_core::{ProcessState, ThreadState};
use omfp_protocol::{Repository, ThreadSummary, Workspace};

use super::threads::{ThreadStatus, WorkspaceChip, workspace_chips};
use crate::input::ThreadDetails;

/// One model choice: `value` is `provider/id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    pub value: String,
    pub label: String,
}

/// Everything the header shows.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderView {
    pub title: String,
    pub tags: Vec<String>,
    pub chips: Vec<WorkspaceChip>,
    /// Workspaces that can still be attached: `(id, label)`.
    pub attachable: Vec<(String, String)>,
    /// `provider/id` of the current model.
    pub model: Option<String>,
    /// The available models; the current one is included even when the list has not arrived.
    pub models: Vec<ModelOption>,
    pub thinking: Option<ThinkingLevel>,
    pub thinking_levels: Vec<ThinkingLevel>,
    /// `42% of 200k`.
    pub context: Option<String>,
    /// `$0.0123`.
    pub cost: Option<String>,
    pub status: ThreadStatus,
    /// Why the process failed.
    pub failure: Option<String>,
    /// The process runs (or starts): it can be stopped.
    pub can_stop: bool,
}

/// The header of `summary`'s thread.
pub fn header_view(
    summary: &ThreadSummary,
    state: Option<&ThreadState>,
    details: Option<&ThreadDetails>,
    workspaces: &[Workspace],
    repositories: &[Repository],
) -> HeaderView {
    let session = details.and_then(|details| details.session.as_ref());
    let current = session.and_then(|session| session.model.as_ref());
    let mut models: Vec<ModelOption> = details
        .map(|details| {
            details
                .models
                .iter()
                .map(|model| ModelOption {
                    value: format!("{}/{}", model.provider, model.id),
                    label: format!("{} ({})", model.name, model.provider),
                })
                .collect()
        })
        .unwrap_or_default();
    let model = current.map(|model| format!("{}/{}", model.provider, model.id));
    if let (Some(model), Some(current)) = (&model, current)
        && !models.iter().any(|option| option.value == *model)
    {
        models.insert(
            0,
            ModelOption {
                value: model.clone(),
                label: format!("{} ({})", current.name, current.provider),
            },
        );
    }
    let mut thinking_levels = details
        .map(|details| details.thinking_levels.clone())
        .unwrap_or_default();
    let thinking = session.and_then(|session| session.thinking_level);
    if let Some(level) = thinking
        && !thinking_levels.contains(&level)
    {
        thinking_levels.insert(0, level);
    }
    let attachable = workspaces
        .iter()
        .filter(|workspace| !summary.workspace_ids.contains(&workspace.id))
        .map(|workspace| {
            (
                workspace.id.clone(),
                super::threads::workspace_label(workspace, repositories),
            )
        })
        .collect();
    let cost = state
        .map(session_cost)
        .filter(|cost| *cost > 0.0)
        .map(format_cost);
    HeaderView {
        title: summary.title.clone(),
        tags: summary.tags.clone(),
        chips: workspace_chips(&summary.workspace_ids, workspaces, repositories),
        attachable,
        model,
        models,
        thinking,
        thinking_levels,
        context: session
            .and_then(|session| session.context_usage.as_ref())
            .map(|usage| {
                format!(
                    "{:.0}% of {}",
                    usage.percent,
                    format_tokens(usage.context_window)
                )
            }),
        cost,
        status: ThreadStatus::of(summary),
        failure: match &summary.process {
            ProcessState::Failed { message, .. } => Some(message.clone()),
            _ => None,
        },
        can_stop: matches!(
            summary.process,
            ProcessState::Running | ProcessState::Starting
        ),
    }
}

/// Total cost reported by the assistant messages of the transcript.
#[must_use]
pub fn session_cost(state: &ThreadState) -> f64 {
    state
        .transcript
        .iter()
        .filter_map(|entry| match &entry.message {
            AgentMessage::Assistant(message) => message.usage.as_ref()?.cost.as_ref()?.total,
            _ => None,
        })
        .sum()
}

/// `$0.0123` below a dollar, `$1.23` above.
#[must_use]
pub fn format_cost(cost: f64) -> String {
    if cost < 1.0 {
        format!("${cost:.4}")
    } else {
        format!("${cost:.2}")
    }
}

/// `200k`, `1.0M`, `950`.
#[must_use]
pub fn format_tokens(tokens: i64) -> String {
    match tokens {
        tokens if tokens >= 1_000_000 => {
            format!("{}.{}M", tokens / 1_000_000, tokens % 1_000_000 / 100_000)
        }
        tokens if tokens >= 1000 => format!("{}k", tokens / 1000),
        tokens => tokens.to_string(),
    }
}

/// Tags typed as a comma-separated list, trimmed, without empties or duplicates.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.split(',').map(str::trim).filter(|tag| !tag.is_empty()) {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.to_owned());
        }
    }
    tags
}

/// The model choice `provider/id` split in two (the id may contain slashes).
#[must_use]
pub fn split_model(value: &str) -> Option<(String, String)> {
    let (provider, id) = value.split_once('/')?;
    (!provider.is_empty() && !id.is_empty()).then(|| (provider.to_owned(), id.to_owned()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::{fixture, repository, thread, workspace};

    fn details() -> ThreadDetails {
        serde_json::from_value::<omfp_core::wire::SessionState>(json!({
            "sessionId": "s",
            "model": {
                "id": "opus", "name": "Opus", "api": "anthropic", "provider": "anthropic", "baseUrl": "",
                "reasoning": true, "input": ["text"], "contextWindow": 200_000, "maxTokens": 32000,
                "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
            },
            "thinkingLevel": "high",
            "contextUsage": {"tokens": 84000, "contextWindow": 200_000, "percent": 42.0}
        }))
        .map(|session| ThreadDetails {
            session: Some(session),
            models: Vec::new(),
            thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Low],
        })
        .expect("session state")
    }

    // spec: UI-030, UI-011
    #[test]
    fn header_shows_model_thinking_context_and_attachable_workspaces() {
        let repositories = vec![repository("r", "fe")];
        let workspaces = vec![
            workspace("w1", Some("r"), Some("main")),
            workspace("w2", Some("r"), Some("dev")),
        ];
        let summary = thread("t", "checkout", &["w1"]);
        let details = details();
        let view = header_view(&summary, None, Some(&details), &workspaces, &repositories);
        assert_eq!(view.model.as_deref(), Some("anthropic/opus"));
        assert_eq!(
            view.models,
            vec![ModelOption {
                value: "anthropic/opus".to_owned(),
                label: "Opus (anthropic)".to_owned()
            }]
        );
        assert_eq!(view.thinking, Some(ThinkingLevel::High));
        assert_eq!(
            view.thinking_levels,
            vec![ThinkingLevel::High, ThinkingLevel::Off, ThinkingLevel::Low]
        );
        assert_eq!(view.context.as_deref(), Some("42% of 200k"));
        assert_eq!(
            view.attachable,
            vec![("w2".to_owned(), "fe:dev".to_owned())]
        );
        assert!(view.chips[0].cwd);
        assert!(view.can_stop);
    }

    // spec: UI-030
    #[test]
    fn cost_sums_the_assistant_usage_of_the_transcript() {
        let mut state = fixture("text_reply");
        assert_eq!(session_cost(&state), 0.0);
        for entry in &mut state.transcript {
            if let AgentMessage::Assistant(message) = &mut entry.message {
                message
                    .usage
                    .get_or_insert_default()
                    .cost
                    .get_or_insert_default()
                    .total = Some(0.0125);
            }
        }
        state.transcript.push(state.transcript[1].clone());
        assert_eq!(format_cost(session_cost(&state)), "$0.0250");
        assert_eq!(format_cost(12.5), "$12.50");
    }

    // spec: UI-030
    #[test]
    fn tags_and_model_values_are_parsed_from_the_editors() {
        assert_eq!(
            parse_tags(" ci, release ,, ci"),
            vec!["ci".to_owned(), "release".to_owned()]
        );
        assert_eq!(
            split_model("openrouter/anthropic/opus"),
            Some(("openrouter".to_owned(), "anthropic/opus".to_owned()))
        );
        assert_eq!(split_model("opus"), None);
        assert_eq!(format_tokens(1_250_000), "1.2M");
    }
}
