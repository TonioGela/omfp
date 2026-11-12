//! Answers to omp's interactive requests (DMN-050..054): `select`, `confirm`, `input`, `editor` and `ask` dialogs.

use omfp_core::wire::LitTrue;
use omfp_core::{Resolution, ThreadEvent, validate_ask_answers};
use omfp_protocol::InteractionResponse;
use omp_rpc::{
    AnswersUiResponse, CancelUiResponse, ConfirmUiResponse, ExtensionUiRequest,
    ExtensionUiResponse, ValueUiResponse,
};

use super::{Result, Supervisor, SupervisorError, lock};

impl Supervisor {
    /// Answers a pending interactive request: the first valid answer is forwarded and recorded, later ones get
    /// `already_resolved` (DMN-051); `ask` answers are validated first (DMN-054).
    pub async fn respond(
        &self,
        id: &str,
        interaction_id: &str,
        response: InteractionResponse,
        client: &str,
    ) -> Result<()> {
        let actor = self.actor(id).await?;
        let _answers = lock(&actor.answers);
        let request = actor.log.with_state(|state| {
            state
                .interactions
                .iter()
                .find(|request| dialog_id(request) == Some(interaction_id))
                .cloned()
        });
        let (Some(request), Some(process)) = (request, actor.process()) else {
            return Err(SupervisorError::AlreadyResolved(interaction_id.to_owned()));
        };
        let reply = ui_response(&request, interaction_id, response)
            .map_err(SupervisorError::InvalidResponse)?;
        // `respond` only queues the frame, so holding the answer lock across it is cheap.
        process.respond(reply)?;
        self.push(
            &actor,
            ThreadEvent::InteractionResolved {
                id: interaction_id.to_owned(),
                resolution: Resolution::Client {
                    client: client.to_owned(),
                },
            },
        );
        Ok(())
    }
}

/// The id of a dialog request; `None` for presentation-only methods.
pub(super) fn dialog_id(request: &ExtensionUiRequest) -> Option<&str> {
    match request {
        ExtensionUiRequest::Select(select) => Some(&select.id),
        ExtensionUiRequest::Confirm(confirm) => Some(&confirm.id),
        ExtensionUiRequest::Input(input) => Some(&input.id),
        ExtensionUiRequest::Editor(editor) => Some(&editor.id),
        ExtensionUiRequest::Ask(ask) => Some(&ask.id),
        _ => None,
    }
}

/// The omp answer for `request`, checked against its kind: `select` takes one of its options, `confirm` a boolean,
/// `ask` answers in omp's shape (DMN-054); any request can be cancelled.
fn ui_response(
    request: &ExtensionUiRequest,
    id: &str,
    response: InteractionResponse,
) -> std::result::Result<ExtensionUiResponse, String> {
    let id = id.to_owned();
    match (request, response) {
        (_, InteractionResponse::Cancelled) => {
            Ok(ExtensionUiResponse::CancelUiResponse(CancelUiResponse {
                id,
                cancelled: LitTrue,
                timed_out: None,
            }))
        }
        (ExtensionUiRequest::Select(select), InteractionResponse::Value(value)) => {
            if select.options.contains(&value) {
                Ok(ExtensionUiResponse::ValueUiResponse(ValueUiResponse {
                    id,
                    value,
                }))
            } else {
                Err(format!(
                    "{value:?} is not one of the options {:?}",
                    select.options
                ))
            }
        }
        (
            ExtensionUiRequest::Input(_) | ExtensionUiRequest::Editor(_),
            InteractionResponse::Value(value),
        ) => Ok(ExtensionUiResponse::ValueUiResponse(ValueUiResponse {
            id,
            value,
        })),
        (ExtensionUiRequest::Confirm(_), InteractionResponse::Confirmed(confirmed)) => {
            Ok(ExtensionUiResponse::ConfirmUiResponse(ConfirmUiResponse {
                id,
                confirmed,
            }))
        }
        (ExtensionUiRequest::Ask(ask), InteractionResponse::Answers(answers)) => {
            validate_ask_answers(ask, &answers)?;
            Ok(ExtensionUiResponse::AnswersUiResponse(AnswersUiResponse {
                id,
                answers,
            }))
        }
        (_, response) => Err(format!(
            "{} cannot answer this request",
            match response {
                InteractionResponse::Value(_) => "a value",
                InteractionResponse::Confirmed(_) => "a confirmation",
                InteractionResponse::Answers(_) => "ask answers",
                InteractionResponse::Cancelled => "a cancellation",
            }
        )),
    }
}
