//! The composer (UI-060..064): send mode while busy, `/` command completion, key bindings, image attachments, and
//! drafts restored after an abort.

use omfp_core::ThreadState;
use omfp_core::wire::{AvailableSlashCommand, ImageContent, LitImage};
use omfp_protocol::PromptBehavior;

/// Most suggestions shown at once.
const MAX_SUGGESTIONS: usize = 8;

/// A composer draft: text and attached images (UI-064).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Draft {
    pub text: String,
    pub images: Vec<ImageContent>,
}

impl Draft {
    /// Nothing to send.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty() && self.images.is_empty()
    }
}

/// The agent is working: messages sent now are queued (UI-061).
#[must_use]
pub fn is_busy(state: &ThreadState) -> bool {
    !state.settled
}

/// What the send action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    /// The thread is idle: a new turn.
    Prompt,
    /// Busy, default: queued until the turn ends.
    FollowUp,
    /// Busy, with the modifier: delivered into the running turn.
    Steer,
}

impl SendMode {
    /// Busy threads queue as follow-up unless the steer modifier (Alt, Ctrl or ⌘ with Enter) is held.
    #[must_use]
    pub fn of(busy: bool, steer_modifier: bool) -> Self {
        match (busy, steer_modifier) {
            (false, _) => Self::Prompt,
            (true, false) => Self::FollowUp,
            (true, true) => Self::Steer,
        }
    }

    #[must_use]
    pub fn behavior(self) -> Option<PromptBehavior> {
        match self {
            Self::Prompt => None,
            Self::FollowUp => Some(PromptBehavior::FollowUp),
            Self::Steer => Some(PromptBehavior::Steer),
        }
    }

    /// Label of the send button: the choice is always visible (UI-061).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Prompt => "Send",
            Self::FollowUp => "Queue follow-up",
            Self::Steer => "Steer now",
        }
    }

    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::FollowUp => "follow-up",
            Self::Steer => "steer",
        }
    }

    /// The other busy mode, offered as a second button.
    #[must_use]
    pub fn alternative(self) -> Option<Self> {
        match self {
            Self::Prompt => None,
            Self::FollowUp => Some(Self::Steer),
            Self::Steer => Some(Self::FollowUp),
        }
    }
}

/// A key the composer reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerKey {
    Enter,
    Escape,
    Tab,
    Other,
}

/// Modifier keys held with a key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // One flag per physical modifier.
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
}

impl Modifiers {
    /// The modifier that turns a busy send into a steer.
    #[must_use]
    pub fn steer(self) -> bool {
        self.alt || self.ctrl || self.meta
    }
}

/// What a key press in the composer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Let the textarea handle it (typing, Shift+Enter newline).
    None,
    Send(SendMode),
    /// Esc while busy: `thread.abort` (UI-063).
    Abort,
    /// Tab with suggestions: complete the first one.
    Complete,
}

/// The composer's key bindings.
#[must_use]
pub fn key_action(
    key: ComposerKey,
    modifiers: Modifiers,
    busy: bool,
    has_suggestions: bool,
) -> KeyAction {
    match key {
        ComposerKey::Enter if modifiers.shift => KeyAction::None,
        ComposerKey::Enter => KeyAction::Send(SendMode::of(busy, modifiers.steer())),
        ComposerKey::Escape if busy => KeyAction::Abort,
        ComposerKey::Tab if has_suggestions && !modifiers.shift => KeyAction::Complete,
        ComposerKey::Escape | ComposerKey::Tab | ComposerKey::Other => KeyAction::None,
    }
}

/// One `/` completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub name: String,
    pub description: Option<String>,
    /// The usage hint of the command's argument.
    pub hint: Option<String>,
}

/// Commands matching the draft while it is a bare `/prefix` (no space yet): names first, then aliases, in the order
/// of `thread.commands` (kept fresh by `available_commands_update`, UI-060).
pub fn slash_suggestions(draft: &str, commands: &[AvailableSlashCommand]) -> Vec<Suggestion> {
    let Some(prefix) = draft.strip_prefix('/') else {
        return Vec::new();
    };
    if prefix.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    let by_name = commands
        .iter()
        .filter(|command| command.name.starts_with(prefix));
    let by_alias = commands.iter().filter(|command| {
        !command.name.starts_with(prefix)
            && command
                .aliases
                .iter()
                .flatten()
                .any(|alias| alias.starts_with(prefix))
    });
    by_name
        .chain(by_alias)
        .take(MAX_SUGGESTIONS)
        .map(|command| Suggestion {
            name: command.name.clone(),
            description: command.description.clone(),
            hint: command.input.as_ref().and_then(|input| input.hint.clone()),
        })
        .collect()
}

/// The draft after picking `suggestion`.
#[must_use]
pub fn complete(suggestion: &Suggestion) -> String {
    format!("/{} ", suggestion.name)
}

/// The draft after an abort: the restored steering and follow-up texts, oldest first, then what was being typed, one
/// blank line apart (UI-063).
pub fn restore_draft(draft: &str, steering: &[String], follow_up: &[String]) -> String {
    steering
        .iter()
        .chain(follow_up)
        .map(String::as_str)
        .chain(std::iter::once(draft))
        .map(str::trim_end)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The draft after a prompt failed to send: the failed text, then what was typed since, one blank line apart; the
/// failed images, then the images attached since. Nothing the user wrote is lost (UI-064).
#[must_use]
pub fn restore_failed_prompt(draft: &Draft, text: &str, images: &[ImageContent]) -> Draft {
    Draft {
        text: restore_draft(&draft.text, &[text.to_owned()], &[]),
        images: images.iter().chain(&draft.images).cloned().collect(),
    }
}

/// An image attachment from pasted or dropped bytes; `None` for anything but `image/*` (UI-060).
#[must_use]
pub fn image_attachment(mime: &str, bytes: &[u8]) -> Option<ImageContent> {
    mime.starts_with("image/").then(|| ImageContent {
        r#type: Some(LitImage),
        data: Some(base64(bytes)),
        mime_type: Some(mime.to_owned()),
        ..ImageContent::default()
    })
}

/// Standard base64 with padding.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = chunk.iter().enumerate().fold(0u32, |acc, (index, byte)| {
            acc | u32::from(*byte) << (16 - 8 * index)
        });
        for position in 0..4 {
            if position <= chunk.len() {
                let sextet = (triple >> (18 - 6 * position)) & 0x3f;
                out.push(char::from(ALPHABET[sextet as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::{fixture, fixture_until};

    fn commands() -> Vec<AvailableSlashCommand> {
        serde_json::from_value(json!([
            {"name": "compact", "source": "builtin", "description": "Compact the context"},
            {"name": "model", "source": "builtin", "aliases": ["m"], "input": {"hint": "<model>"}},
            {"name": "mcp", "source": "builtin"},
            {"name": "copy", "source": "builtin", "aliases": ["cp"]}
        ]))
        .expect("commands")
    }

    fn names(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions
            .iter()
            .map(|suggestion| suggestion.name.as_str())
            .collect()
    }

    // spec: UI-061
    #[test]
    fn busy_threads_queue_follow_ups_unless_the_modifier_steers() {
        let busy = fixture_until("follow_up_then_abort", |event| {
            event
                .pointer("/frame/type")
                .and_then(serde_json::Value::as_str)
                == Some("queue_update")
        });
        assert!(is_busy(&busy));
        assert!(!is_busy(&fixture("text_reply")));
        let plain = Modifiers::default();
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        assert_eq!(
            key_action(ComposerKey::Enter, plain, true, false),
            KeyAction::Send(SendMode::FollowUp)
        );
        assert_eq!(
            key_action(ComposerKey::Enter, alt, true, false),
            KeyAction::Send(SendMode::Steer)
        );
        assert_eq!(
            key_action(ComposerKey::Enter, alt, false, false),
            KeyAction::Send(SendMode::Prompt)
        );
        assert_eq!(
            SendMode::FollowUp.behavior(),
            Some(PromptBehavior::FollowUp)
        );
        assert_eq!(SendMode::Steer.behavior(), Some(PromptBehavior::Steer));
        assert_eq!(SendMode::Prompt.behavior(), None);
        assert_eq!(SendMode::FollowUp.alternative(), Some(SendMode::Steer));
    }

    // spec: UI-060
    #[test]
    fn shift_enter_is_a_newline_and_tab_completes_only_with_suggestions() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            key_action(ComposerKey::Enter, shift, false, false),
            KeyAction::None
        );
        assert_eq!(
            key_action(ComposerKey::Tab, Modifiers::default(), false, true),
            KeyAction::Complete
        );
        assert_eq!(
            key_action(ComposerKey::Tab, Modifiers::default(), false, false),
            KeyAction::None
        );
    }

    // spec: UI-063
    #[test]
    fn escape_aborts_only_while_busy() {
        assert_eq!(
            key_action(ComposerKey::Escape, Modifiers::default(), true, false),
            KeyAction::Abort
        );
        assert_eq!(
            key_action(ComposerKey::Escape, Modifiers::default(), false, false),
            KeyAction::None
        );
    }

    // spec: UI-060
    #[test]
    fn slash_completion_matches_names_then_aliases_until_a_space() {
        let commands = commands();
        assert_eq!(
            names(&slash_suggestions("/c", &commands)),
            vec!["compact", "copy"]
        );
        assert_eq!(
            names(&slash_suggestions("/m", &commands)),
            vec!["model", "mcp"]
        );
        assert_eq!(names(&slash_suggestions("/cp", &commands)), vec!["copy"]);
        assert_eq!(
            names(&slash_suggestions("/model gpt", &commands)),
            Vec::<&str>::new()
        );
        assert_eq!(
            names(&slash_suggestions("hello /m", &commands)),
            Vec::<&str>::new()
        );
        let model = &slash_suggestions("/mo", &commands)[0];
        assert_eq!(
            (model.hint.as_deref(), complete(model).as_str()),
            (Some("<model>"), "/model ")
        );
    }

    // spec: UI-060
    #[test]
    fn suggestions_come_from_the_threads_available_commands() {
        let state = fixture("text_reply");
        assert_ne!(state.commands, Vec::new());
        let first = &state.commands[0].name;
        let suggestions = slash_suggestions(&format!("/{first}"), &state.commands);
        assert_eq!(
            suggestions.first().map(|suggestion| &suggestion.name),
            Some(first)
        );
    }

    // spec: UI-063
    #[test]
    fn an_abort_puts_the_queued_texts_back_before_the_draft() {
        let restored = restore_draft(
            "still typing",
            &["steer me".to_owned()],
            &["later please".to_owned()],
        );
        assert_eq!(restored, "steer me\n\nlater please\n\nstill typing");
        assert_eq!(
            restore_draft("", &[], &["later please".to_owned()]),
            "later please"
        );
    }

    // spec: UI-060
    #[test]
    fn only_images_become_attachments_encoded_as_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        let image = image_attachment("image/png", &[0x89, b'P', b'N', b'G']).expect("image");
        assert_eq!(
            (image.mime_type.as_deref(), image.data.as_deref()),
            (Some("image/png"), Some("iVBORw=="))
        );
        assert_eq!(image_attachment("text/plain", b"hi"), None);
    }
}
