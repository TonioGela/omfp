//! Validation of `ask` dialog answers (DMN-054), mirroring omp's own check (rpc.md, "Inbound response"): any other
//! shape fails omp's `ask` call, so the daemon rejects it before forwarding.

use crate::wire::{AskAnswer, AskUiRequest};

/// Checks `answers` against `request`: one answer per question in request order with the question's id; selected
/// options are exact, distinct labels of that question; a single-select question takes at most one option and not both
/// an option and non-blank custom input. The error describes the first violation.
pub fn validate_ask_answers(request: &AskUiRequest, answers: &[AskAnswer]) -> Result<(), String> {
    if answers.len() != request.questions.len() {
        return Err(format!(
            "expected {} answers in question order, got {}",
            request.questions.len(),
            answers.len()
        ));
    }
    for (index, (question, answer)) in request.questions.iter().zip(answers).enumerate() {
        if answer.id != question.id {
            return Err(format!(
                "answer {index} must have id {:?}, got {:?}",
                question.id, answer.id
            ));
        }
        for (position, label) in answer.selected_options.iter().enumerate() {
            if !question.options.iter().any(|option| option.label == *label) {
                return Err(format!(
                    "answer {:?} selected unknown option {label:?}",
                    question.id
                ));
            }
            if answer.selected_options[..position].contains(label) {
                return Err(format!("answer {:?} selected {label:?} twice", question.id));
            }
        }
        let custom = answer
            .custom_input
            .as_deref()
            .is_some_and(|input| !input.trim().is_empty());
        let selected = answer.selected_options.len();
        if !question.multi && (selected > 1 || (selected == 1 && custom)) {
            return Err(format!(
                "answer {:?} is single-select but carries more than one answer",
                question.id
            ));
        }
    }
    Ok(())
}
