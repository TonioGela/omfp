//! Window-local UI state (UI-010, UI-005): selection, drafts, expanded cards, list grouping and filter, panel
//! placement, theme, and what this window is waiting for. Never sent to the daemon.

use std::collections::{BTreeMap, BTreeSet};

use omfp_protocol::ThreadSummary;

use crate::input::OutcomeEntry;
use crate::intent::{Intent, IntentOutcome};
use crate::panels::PanelRegistry;
use crate::vm::composer::{Draft, restore_draft, restore_failed_prompt};
use crate::vm::interaction::AskForm;
use crate::vm::new_thread::NewThreadForm;
use crate::vm::queue::PendingQueueOp;
use crate::vm::threads::{Filter, Grouping};

/// `(thread id, interaction id)`.
pub type InteractionKey = (String, String);

/// Colour scheme: the system's (`prefers-color-scheme`) or a manual override (UI-091).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|theme| theme.token() == token)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

/// The header's title and tags while being edited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeaderEdit {
    pub title: String,
    /// Comma-separated.
    pub tags: String,
}

/// Everything only this window knows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiState {
    pub selected: Option<String>,
    /// Composer drafts by thread id, kept across thread switches and reconnects (UI-064).
    pub drafts: BTreeMap<String, Draft>,
    /// Expanded/collapsed overrides by card key; cards without one use their default (UI-041, UI-052).
    pub expanded: BTreeMap<String, bool>,
    pub grouping: Grouping,
    pub filter: Filter,
    pub panels: PanelRegistry,
    /// Active tab of the side column; the first side panel when unset.
    pub side_tab: Option<String>,
    /// Active main-area panel; the thread when unset.
    pub main_tab: Option<String>,
    pub theme: Theme,
    /// Queue actions waiting for the daemon (UI-062).
    pub pending_queue: Vec<PendingQueueOp>,
    /// Interactions answered from this window.
    pub answered_here: BTreeSet<InteractionKey>,
    /// Closed-interaction rows the user dismissed.
    pub dismissed: BTreeSet<InteractionKey>,
    /// When each interaction was first shown (epoch ms), for countdowns (UI-073).
    pub shown_at: BTreeMap<InteractionKey, i64>,
    /// `ask` forms being filled.
    pub ask_forms: BTreeMap<InteractionKey, AskForm>,
    /// `updated_at` of each thread when this window last showed it (unread badges, UI-020).
    pub seen: BTreeMap<String, i64>,
    /// The new-thread dialog, when open.
    pub new_thread: Option<NewThreadForm>,
    /// Header editor of the selected thread, when open.
    pub header_edit: Option<HeaderEdit>,
    /// Errors to show until dismissed.
    pub errors: Vec<String>,
    /// The steer modifier is held (the send button shows "Steer now", UI-061).
    pub steer_modifier: bool,
    /// Sequence number of the last applied outcome; `None` before the first one.
    pub last_outcome: Option<u64>,
}

impl UiState {
    /// Selects `thread_id`, marking it seen at `updated_at`, and returns the intent that opens it.
    pub fn select(&mut self, thread_id: &str, updated_at: Option<i64>) -> Intent {
        self.selected = Some(thread_id.to_owned());
        self.main_tab = None;
        self.header_edit = None;
        if let Some(updated_at) = updated_at {
            self.seen.insert(thread_id.to_owned(), updated_at);
        }
        Intent::SelectThread {
            thread_id: thread_id.to_owned(),
        }
    }

    /// What the index changes in `seen`: threads observed for the first time get their current `updated_at` as
    /// baseline, and the selected thread is always seen. Empty when nothing changes, so callers can skip the write.
    #[must_use]
    pub fn seen_updates(&self, threads: &[ThreadSummary]) -> Vec<(String, i64)> {
        threads
            .iter()
            .filter(|thread| match self.seen.get(&thread.id) {
                None => true,
                Some(seen) => {
                    self.selected.as_deref() == Some(thread.id.as_str())
                        && *seen < thread.updated_at
                }
            })
            .map(|thread| (thread.id.clone(), thread.updated_at))
            .collect()
    }

    /// Whether the outcome numbered `seq` has not been applied yet.
    #[must_use]
    pub fn is_fresh_outcome(&self, seq: u64) -> bool {
        self.last_outcome.is_none_or(|last| seq > last)
    }

    /// Applies the outcomes not applied yet, in order, and returns the intents they lead to.
    pub fn apply_outcomes(&mut self, outcomes: &[OutcomeEntry]) -> Vec<Intent> {
        let mut follow_ups = Vec::new();
        for entry in outcomes {
            if !self.is_fresh_outcome(entry.seq) {
                continue;
            }
            self.last_outcome = Some(entry.seq);
            follow_ups.extend(self.apply_outcome(&entry.outcome));
        }
        follow_ups
    }

    /// Applies one outcome; a created thread is selected and opened.
    pub fn apply_outcome(&mut self, outcome: &IntentOutcome) -> Option<Intent> {
        match outcome {
            IntentOutcome::ThreadCreated { thread_id } => {
                self.new_thread = None;
                return Some(self.select(thread_id, None));
            }
            IntentOutcome::Aborted {
                thread_id,
                steering,
                follow_up,
            } => {
                let draft = self.drafts.entry(thread_id.clone()).or_default();
                draft.text = restore_draft(&draft.text, steering, follow_up);
            }
            IntentOutcome::Queue {
                thread_id,
                op,
                message,
                applied,
            } => {
                self.pending_queue.retain(|pending| {
                    !(pending.thread_id == *thread_id
                        && pending.op == *op
                        && pending.message == *message)
                });
                if !applied {
                    self.errors
                        .push(format!("\u{201c}{message}\u{201d} is no longer queued"));
                }
            }
            IntentOutcome::Failed { intent, message } => {
                match intent {
                    Intent::Queue {
                        thread_id,
                        op,
                        message,
                        ..
                    } => self.pending_queue.retain(|pending| {
                        !(pending.thread_id == *thread_id
                            && pending.op == *op
                            && pending.message == *message)
                    }),
                    Intent::Respond {
                        thread_id,
                        interaction_id,
                        ..
                    } => {
                        self.answered_here
                            .remove(&(thread_id.clone(), interaction_id.clone()));
                    }
                    Intent::Prompt {
                        thread_id,
                        text,
                        images,
                        ..
                    } => {
                        let draft = self.drafts.entry(thread_id.clone()).or_default();
                        *draft = restore_failed_prompt(draft, text, images);
                    }
                    _ => {}
                }
                self.errors.push(message.clone());
            }
        }
        None
    }

    /// Whether the card `key` is expanded, given its default.
    #[must_use]
    pub fn is_expanded(&self, key: &str, default: bool) -> bool {
        self.expanded.get(key).copied().unwrap_or(default)
    }

    /// Flips the card `key`.
    pub fn toggle_expanded(&mut self, key: &str, default: bool) {
        let expanded = self.is_expanded(key, default);
        self.expanded.insert(key.to_owned(), !expanded);
    }

    /// The draft of `thread_id`.
    #[must_use]
    pub fn draft(&self, thread_id: &str) -> Draft {
        self.drafts.get(thread_id).cloned().unwrap_or_default()
    }

    /// The side panel shown: the chosen tab when still in the side column, else the first side panel.
    #[must_use]
    pub fn active_side_panel(&self) -> Option<String> {
        let side = self.panels.in_placement(crate::panels::Placement::Side);
        self.side_tab
            .as_ref()
            .filter(|id| side.iter().any(|panel| panel.id == **id))
            .cloned()
            .or_else(|| side.first().map(|panel| panel.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use omfp_core::wire::QueuedMessageQueue;

    use super::*;
    use crate::intent::QueueOp;
    use crate::test_support::thread;
    use crate::vm::composer::image_attachment;

    fn entry(seq: u64, outcome: IntentOutcome) -> OutcomeEntry {
        OutcomeEntry { seq, outcome }
    }

    // spec: UI-063
    #[test]
    fn an_abort_outcome_restores_the_queued_texts_into_that_threads_draft() {
        let mut ui = UiState::default();
        ui.drafts.insert(
            "t".to_owned(),
            Draft {
                text: "typing".to_owned(),
                images: Vec::new(),
            },
        );
        let outcomes = vec![entry(
            1,
            IntentOutcome::Aborted {
                thread_id: "t".to_owned(),
                steering: Vec::new(),
                follow_up: vec!["later please".to_owned()],
            },
        )];
        assert_eq!(ui.apply_outcomes(&outcomes), Vec::new());
        assert_eq!(ui.draft("t").text, "later please\n\ntyping");
        // Already applied outcomes are skipped.
        ui.apply_outcomes(&outcomes);
        assert_eq!(ui.draft("t").text, "later please\n\ntyping");
        assert_eq!(ui.draft("other"), Draft::default());
    }

    // spec: UI-062
    #[test]
    fn a_queue_action_stays_pending_until_the_daemon_answers() {
        let mut ui = UiState::default();
        let pending = PendingQueueOp {
            thread_id: "t".to_owned(),
            op: QueueOp::Promote,
            message: "m".to_owned(),
        };
        ui.pending_queue.push(pending.clone());
        ui.apply_outcome(&IntentOutcome::Queue {
            thread_id: "t".to_owned(),
            op: QueueOp::Remove,
            message: "m".to_owned(),
            applied: true,
        });
        assert_eq!(ui.pending_queue, vec![pending]);
        ui.apply_outcome(&IntentOutcome::Queue {
            thread_id: "t".to_owned(),
            op: QueueOp::Promote,
            message: "m".to_owned(),
            applied: false,
        });
        assert_eq!(ui.pending_queue, Vec::new());
        assert_eq!(
            ui.errors,
            vec!["\u{201c}m\u{201d} is no longer queued".to_owned()]
        );
        ui.pending_queue.push(PendingQueueOp {
            thread_id: "t".to_owned(),
            op: QueueOp::Remove,
            message: "x".to_owned(),
        });
        ui.apply_outcome(&IntentOutcome::Failed {
            intent: Intent::Queue {
                thread_id: "t".to_owned(),
                op: QueueOp::Remove,
                message: "x".to_owned(),
                queue: QueuedMessageQueue::FollowUp,
            },
            message: "thread stopped".to_owned(),
        });
        assert_eq!(ui.pending_queue, Vec::new());
    }

    // spec: UI-023
    #[test]
    fn a_created_thread_is_selected_and_opened() {
        let mut ui = UiState {
            new_thread: Some(NewThreadForm::default()),
            ..UiState::default()
        };
        let follow_ups = ui.apply_outcomes(&[entry(
            4,
            IntentOutcome::ThreadCreated {
                thread_id: "t9".to_owned(),
            },
        )]);
        assert_eq!(
            follow_ups,
            vec![Intent::SelectThread {
                thread_id: "t9".to_owned()
            }]
        );
        assert_eq!(
            (
                ui.selected.as_deref(),
                ui.new_thread.is_none(),
                ui.last_outcome
            ),
            (Some("t9"), true, Some(4))
        );
    }

    #[test]
    fn an_outcome_numbered_zero_is_applied_once() {
        let mut ui = UiState::default();
        let outcomes = vec![entry(
            0,
            IntentOutcome::Failed {
                intent: Intent::SelectThread {
                    thread_id: "t".to_owned(),
                },
                message: "boom".to_owned(),
            },
        )];
        assert!(ui.is_fresh_outcome(0));
        ui.apply_outcomes(&outcomes);
        ui.apply_outcomes(&outcomes);
        assert_eq!(
            (ui.errors, ui.last_outcome),
            (vec!["boom".to_owned()], Some(0))
        );
    }

    // spec: UI-064
    #[test]
    fn a_failed_prompt_goes_back_into_that_threads_draft_before_what_was_typed_since() {
        let failed_image = image_attachment("image/png", &[1]).expect("image");
        let new_image = image_attachment("image/png", &[2]).expect("image");
        let mut ui = UiState::default();
        ui.drafts.insert(
            "t".to_owned(),
            Draft {
                text: "typed since".to_owned(),
                images: vec![new_image.clone()],
            },
        );
        ui.apply_outcome(&IntentOutcome::Failed {
            intent: Intent::Prompt {
                thread_id: "t".to_owned(),
                text: "fix the bug".to_owned(),
                images: vec![failed_image.clone()],
                behavior: None,
            },
            message: "thread stopped".to_owned(),
        });
        assert_eq!(
            ui.draft("t"),
            Draft {
                text: "fix the bug\n\ntyped since".to_owned(),
                images: vec![failed_image.clone(), new_image],
            }
        );
        ui.apply_outcome(&IntentOutcome::Failed {
            intent: Intent::Prompt {
                thread_id: "u".to_owned(),
                text: "hello".to_owned(),
                images: vec![failed_image.clone()],
                behavior: None,
            },
            message: "thread stopped".to_owned(),
        });
        assert_eq!(
            ui.draft("u"),
            Draft {
                text: "hello".to_owned(),
                images: vec![failed_image],
            }
        );
        assert_eq!(ui.errors.len(), 2);
    }

    // spec: UI-020
    #[test]
    fn the_index_sets_unread_baselines_and_the_selected_thread_stays_seen() {
        let mut ui = UiState::default();
        let mut first = thread("t1", "one", &[]);
        let mut second = thread("t2", "two", &[]);
        first.updated_at = 5;
        second.updated_at = 7;
        let updates = ui.seen_updates(&[first.clone(), second.clone()]);
        assert_eq!(updates, vec![("t1".to_owned(), 5), ("t2".to_owned(), 7)]);
        ui.seen.extend(updates);
        assert_eq!(
            ui.seen_updates(&[first.clone(), second.clone()]),
            Vec::new()
        );
        ui.select("t1", Some(5));
        first.updated_at = 9;
        second.updated_at = 9;
        assert_eq!(
            ui.seen_updates(&[first, second]),
            vec![("t1".to_owned(), 9)]
        );
    }

    // spec: UI-072
    #[test]
    fn a_failed_answer_is_no_longer_counted_as_answered_here() {
        let mut ui = UiState::default();
        ui.answered_here.insert(("t".to_owned(), "i".to_owned()));
        ui.apply_outcome(&IntentOutcome::Failed {
            intent: Intent::Respond {
                thread_id: "t".to_owned(),
                interaction_id: "i".to_owned(),
                response: omfp_protocol::InteractionResponse::Cancelled,
            },
            message: "already resolved".to_owned(),
        });
        assert_eq!(ui.answered_here, BTreeSet::new());
        assert_eq!(ui.errors, vec!["already resolved".to_owned()]);
    }
}
