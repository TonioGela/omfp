//! Pending interactions (UI-070..073): titles, countdowns, the `ask` form with its selection rules, and requests that
//! another client or omp closed.

use omfp_core::Resolution;
use omfp_core::validate_ask_answers;
use omfp_core::wire::{AskAnswer, AskUiRequest, ExtensionUiRequest};

use crate::input::ClosedInteraction;

/// The id of a dialog request (`select`, `confirm`, `input`, `editor`, `ask`).
#[must_use]
pub fn request_id(request: &ExtensionUiRequest) -> Option<&str> {
    match request {
        ExtensionUiRequest::Select(request) => Some(&request.id),
        ExtensionUiRequest::Confirm(request) => Some(&request.id),
        ExtensionUiRequest::Input(request) => Some(&request.id),
        ExtensionUiRequest::Editor(request) => Some(&request.id),
        ExtensionUiRequest::Ask(request) => Some(&request.id),
        _ => None,
    }
}

/// The request's title (the first question for `ask`).
#[must_use]
pub fn title(request: &ExtensionUiRequest) -> String {
    match request {
        ExtensionUiRequest::Select(request) => request.title.clone(),
        ExtensionUiRequest::Confirm(request) => request.title.clone(),
        ExtensionUiRequest::Input(request) => request.title.clone(),
        ExtensionUiRequest::Editor(request) => request.title.clone(),
        ExtensionUiRequest::Ask(request) => request
            .questions
            .first()
            .map(|question| {
                question
                    .header
                    .clone()
                    .unwrap_or_else(|| question.question.clone())
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Timeout announced by the request, in milliseconds (`editor` has none).
#[must_use]
pub fn timeout_ms(request: &ExtensionUiRequest) -> Option<i64> {
    match request {
        ExtensionUiRequest::Select(request) => request.timeout,
        ExtensionUiRequest::Confirm(request) => request.timeout,
        ExtensionUiRequest::Input(request) => request.timeout,
        ExtensionUiRequest::Ask(request) => request.timeout,
        _ => None,
    }
    .filter(|timeout| *timeout > 0)
}

/// Milliseconds left of `timeout_ms` for a request first shown at `shown_at_ms`, never negative (UI-073).
#[must_use]
pub fn remaining_ms(timeout_ms: i64, shown_at_ms: i64, now_ms: i64) -> i64 {
    (timeout_ms - (now_ms - shown_at_ms).max(0)).max(0)
}

/// `m:ss`, rounding up so the countdown shows `0:00` only when time is up.
#[must_use]
pub fn format_countdown(remaining_ms: i64) -> String {
    let seconds = (remaining_ms.max(0) + 999) / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// The answer being built for one `ask` question.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnswerDraft {
    pub selected: Vec<String>,
    /// Free text; offered for every question since omp never lists "Other" (UI-071).
    pub custom: String,
}

/// The `ask` form: one draft per question, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskForm {
    pub answers: Vec<AnswerDraft>,
}

impl AskForm {
    #[must_use]
    pub fn new(request: &AskUiRequest) -> Self {
        Self {
            answers: vec![AnswerDraft::default(); request.questions.len()],
        }
    }

    /// Picks `label` for question `index`: a single-select question takes it instead of any other choice or free
    /// text; a multi-select one toggles it.
    pub fn toggle(&mut self, request: &AskUiRequest, index: usize, label: &str) {
        let (Some(question), Some(draft)) =
            (request.questions.get(index), self.answers.get_mut(index))
        else {
            return;
        };
        if question.multi {
            match draft.selected.iter().position(|selected| selected == label) {
                Some(position) => {
                    draft.selected.remove(position);
                }
                None => draft.selected.push(label.to_owned()),
            }
        } else {
            draft.selected = vec![label.to_owned()];
            draft.custom.clear();
        }
    }

    /// Sets the free text of question `index`; on a single-select question non-blank text replaces the choice.
    pub fn set_custom(&mut self, request: &AskUiRequest, index: usize, text: &str) {
        let (Some(question), Some(draft)) =
            (request.questions.get(index), self.answers.get_mut(index))
        else {
            return;
        };
        text.clone_into(&mut draft.custom);
        if !question.multi && !text.trim().is_empty() {
            draft.selected.clear();
        }
    }

    /// The answers in question order, selected options in the request's option order.
    #[must_use]
    pub fn answers(&self, request: &AskUiRequest) -> Vec<AskAnswer> {
        request
            .questions
            .iter()
            .zip(&self.answers)
            .map(|(question, draft)| AskAnswer {
                id: question.id.clone(),
                selected_options: question
                    .options
                    .iter()
                    .filter(|option| draft.selected.contains(&option.label))
                    .map(|option| option.label.clone())
                    .collect(),
                custom_input: Some(draft.custom.trim().to_owned())
                    .filter(|custom| !custom.is_empty()),
            })
            .collect()
    }

    /// The answers when every question has one and omp's rules hold (DMN-054), else what is wrong.
    pub fn check(&self, request: &AskUiRequest) -> Result<Vec<AskAnswer>, String> {
        let answers = self.answers(request);
        if let Some(question) = request
            .questions
            .iter()
            .zip(&answers)
            .find(|(_, answer)| answer.selected_options.is_empty() && answer.custom_input.is_none())
            .map(|(question, _)| question)
        {
            return Err(format!("Answer \u{201c}{}\u{201d}", question.question));
        }
        validate_ask_answers(request, &answers)?;
        Ok(answers)
    }
}

/// A closed request shown in place of the dialog (UI-072).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedRow {
    pub id: String,
    pub title: String,
    pub text: String,
}

/// Rows for requests closed by someone else: another client (with its name) or omp. Requests answered from this window
/// (`answered_here`) or dismissed need no row.
pub fn closed_rows(
    closed: &[ClosedInteraction],
    answered_here: impl Fn(&str) -> bool,
    dismissed: impl Fn(&str) -> bool,
) -> Vec<ClosedRow> {
    closed
        .iter()
        .filter_map(|closed| {
            let id = request_id(&closed.request)?;
            if answered_here(id) || dismissed(id) {
                return None;
            }
            let text = match &closed.resolution {
                Some(Resolution::Client { client }) => format!("Answered elsewhere by {client}"),
                None => "Closed by omp (timed out or cancelled)".to_owned(),
            };
            Some(ClosedRow {
                id: id.to_owned(),
                title: title(&closed.request),
                text,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ask() -> AskUiRequest {
        serde_json::from_value(json!({
            "id": "ask-1",
            "timeout": 30000,
            "questions": [
                {"id": "color", "question": "Which color?", "options": [{"label": "red"}, {"label": "blue"}]},
                {"id": "tools", "question": "Which tools?", "multi": true,
                 "options": [{"label": "bash"}, {"label": "read"}, {"label": "edit"}]}
            ]
        }))
        .expect("ask request")
    }

    // spec: UI-071
    #[test]
    fn single_select_keeps_one_choice_and_free_text_replaces_it() {
        let request = ask();
        let mut form = AskForm::new(&request);
        form.toggle(&request, 0, "red");
        form.toggle(&request, 0, "blue");
        assert_eq!(form.answers[0].selected, vec!["blue".to_owned()]);
        form.set_custom(&request, 0, "green");
        assert_eq!(
            form.answers[0],
            AnswerDraft {
                selected: Vec::new(),
                custom: "green".to_owned()
            }
        );
        form.toggle(&request, 0, "red");
        assert_eq!(
            form.answers[0],
            AnswerDraft {
                selected: vec!["red".to_owned()],
                custom: String::new()
            }
        );
    }

    // spec: UI-071
    #[test]
    fn multi_select_toggles_and_combines_with_free_text_in_option_order() {
        let request = ask();
        let mut form = AskForm::new(&request);
        form.toggle(&request, 0, "red");
        form.toggle(&request, 1, "edit");
        form.toggle(&request, 1, "bash");
        form.toggle(&request, 1, "read");
        form.toggle(&request, 1, "read");
        form.set_custom(&request, 1, "  grep ");
        let answers = form.check(&request).expect("valid");
        assert_eq!(
            answers[1].selected_options,
            vec!["bash".to_owned(), "edit".to_owned()]
        );
        assert_eq!(answers[1].custom_input.as_deref(), Some("grep"));
    }

    // spec: UI-071
    #[test]
    fn every_question_needs_an_answer_and_omps_rules_are_enforced() {
        let request = ask();
        let mut form = AskForm::new(&request);
        form.toggle(&request, 1, "bash");
        assert_eq!(
            form.check(&request),
            Err("Answer \u{201c}Which color?\u{201d}".to_owned())
        );
        form.set_custom(&request, 0, "green");
        assert!(form.check(&request).is_ok());
        // A form edited into an invalid shape is refused by `validate_ask_answers`.
        form.answers[0] = AnswerDraft {
            selected: vec!["red".to_owned()],
            custom: "green".to_owned(),
        };
        let error = form
            .check(&request)
            .expect_err("single-select with two answers");
        assert!(error.contains("single-select"), "{error}");
    }

    // spec: UI-073
    #[test]
    fn countdown_counts_down_from_when_the_request_was_shown() {
        let request = ExtensionUiRequest::Ask(ask());
        let timeout = timeout_ms(&request).expect("timeout");
        assert_eq!(
            format_countdown(remaining_ms(timeout, 1_000, 1_000)),
            "0:30"
        );
        assert_eq!(
            format_countdown(remaining_ms(timeout, 1_000, 2_500)),
            "0:29"
        );
        assert_eq!(
            format_countdown(remaining_ms(timeout, 1_000, 40_000)),
            "0:00"
        );
        assert_eq!(format_countdown(remaining_ms(90_000, 0, 0)), "1:30");
        let editor =
            serde_json::from_value(json!({"method": "editor", "id": "e", "title": "Edit"}))
                .expect("editor");
        assert_eq!(timeout_ms(&editor), None);
    }

    // spec: UI-072
    #[test]
    fn requests_closed_elsewhere_say_who_answered_unless_answered_here() {
        let request = |id: &str| -> ExtensionUiRequest {
            serde_json::from_value(
                json!({"method": "confirm", "id": id, "title": "Proceed?", "message": "m"}),
            )
            .expect("confirm")
        };
        let closed = vec![
            ClosedInteraction {
                request: request("a"),
                resolution: Some(Resolution::Client {
                    client: "omfp-ui/web".to_owned(),
                }),
            },
            ClosedInteraction {
                request: request("b"),
                resolution: None,
            },
            ClosedInteraction {
                request: request("c"),
                resolution: Some(Resolution::Client {
                    client: "omfp-ui/web".to_owned(),
                }),
            },
        ];
        let rows = closed_rows(&closed, |id| id == "c", |_| false);
        let texts: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| (row.id.as_str(), row.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("a", "Answered elsewhere by omfp-ui/web"),
                ("b", "Closed by omp (timed out or cancelled)")
            ]
        );
        assert_eq!(rows[0].title, "Proceed?");
        assert_eq!(closed_rows(&closed, |_| false, |id| id != "b").len(), 1);
    }
}
