//! Inline notice rows (UI-043): compaction, retries, omp notices, extension and frame errors as compact rows; frames
//! the reducer could not interpret as a generic row with expandable JSON.

use omfp_core::NoticeFrame;
use omfp_core::wire::NotifyType;
use serde_json::Value;

use super::pretty_json;

/// Severity of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One notice row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeRow {
    /// Stable kind token (`compaction`, `retry`, `notice`, `extension-error`, `frame-error`, `unknown`).
    pub kind: &'static str,
    pub level: Level,
    pub text: String,
    /// Pretty JSON of an unknown frame, shown on demand.
    pub json: Option<String>,
}

/// The row of a notice frame.
pub fn notice_row(frame: &NoticeFrame) -> NoticeRow {
    let row = |kind, level, text: String| NoticeRow {
        kind,
        level,
        text,
        json: None,
    };
    match frame {
        NoticeFrame::AutoCompactionStart(start) => row(
            "compaction",
            Level::Info,
            format!(
                "Compacting the context ({}, {})",
                start.reason.as_str(),
                start.action.as_str()
            ),
        ),
        NoticeFrame::AutoCompactionEnd(end) => {
            compaction_end(end.aborted, end.skipped, end.error_message.as_deref())
        }
        NoticeFrame::ManualCompactionStart(_) => row(
            "compaction",
            Level::Info,
            "Compacting the context".to_owned(),
        ),
        NoticeFrame::ManualCompactionEnd(end) => {
            compaction_end(end.aborted, end.skipped, end.error_message.as_deref())
        }
        NoticeFrame::AutoRetryStart(start) => row(
            "retry",
            Level::Warning,
            format!(
                "Retrying ({}/{}) in {:.1} s: {}",
                start.attempt,
                start.max_attempts,
                ms_to_seconds(start.delay_ms),
                start.error_message
            ),
        ),
        NoticeFrame::AutoRetryEnd(end) if end.success => row(
            "retry",
            Level::Info,
            format!("Retry succeeded (attempt {})", end.attempt),
        ),
        NoticeFrame::AutoRetryEnd(end) => row(
            "retry",
            Level::Error,
            format!(
                "Retry failed after {} attempts: {}",
                end.attempt,
                end.final_error.as_deref().unwrap_or("unknown error")
            ),
        ),
        NoticeFrame::Notice(notice) => {
            let level = match notice.level {
                NotifyType::Info => Level::Info,
                NotifyType::Warning => Level::Warning,
                NotifyType::Error => Level::Error,
            };
            let text = match &notice.source {
                Some(source) => format!("{source}: {}", notice.message),
                None => notice.message.clone(),
            };
            row("notice", level, text)
        }
        NoticeFrame::ExtensionError(error) => row(
            "extension-error",
            Level::Error,
            format!(
                "Extension {} failed on {}: {}",
                error.extension_path, error.event, error.error
            ),
        ),
        NoticeFrame::RpcFrameError(error) => row(
            "frame-error",
            Level::Error,
            match &error.original_type {
                Some(kind) => format!("omp rejected a {kind} frame: {}", error.error),
                None => format!("omp rejected a frame: {}", error.error),
            },
        ),
        NoticeFrame::Unknown(value) => NoticeRow {
            kind: "unknown",
            level: Level::Info,
            text: match value.get("type").and_then(Value::as_str) {
                Some(kind) => format!("Unrecognized {kind} frame"),
                None => "Unrecognized frame".to_owned(),
            },
            json: Some(pretty_json(value)),
        },
    }
}

fn compaction_end(aborted: bool, skipped: Option<bool>, error: Option<&str>) -> NoticeRow {
    let (level, text) = match (aborted, skipped, error) {
        (_, _, Some(error)) => (Level::Error, format!("Compaction failed: {error}")),
        (true, _, None) => (Level::Warning, "Compaction aborted".to_owned()),
        (false, Some(true), None) => (Level::Info, "Compaction skipped".to_owned()),
        (false, _, None) => (Level::Info, "Context compacted".to_owned()),
    };
    NoticeRow {
        kind: "compaction",
        level,
        text,
        json: None,
    }
}

fn ms_to_seconds(ms: i64) -> f64 {
    // Retry delays are far below 2^52 ms.
    #[allow(clippy::cast_precision_loss)]
    let ms = ms as f64;
    ms / 1000.0
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn frame(value: serde_json::Value) -> NoticeFrame {
        serde_json::from_value(value).expect("notice frame")
    }

    // spec: UI-043
    #[test]
    fn known_notices_are_compact_rows_with_their_severity() {
        let retry = notice_row(&frame(json!({
            "type": "auto_retry_start",
            "data": {"attempt": 2, "maxAttempts": 5, "delayMs": 1500, "errorMessage": "overloaded"}
        })));
        assert_eq!((retry.kind, retry.level), ("retry", Level::Warning));
        assert_eq!(retry.text, "Retrying (2/5) in 1.5 s: overloaded");
        assert_eq!(retry.json, None);
        let notice = notice_row(&frame(json!({
            "type": "notice", "data": {"level": "error", "message": "model switched", "source": "advisor"}
        })));
        assert_eq!(
            (notice.level, notice.text.as_str()),
            (Level::Error, "advisor: model switched")
        );
        let compaction = notice_row(&frame(json!({
            "type": "manual_compaction_end", "data": {"aborted": false, "errorMessage": "too small"}
        })));
        assert_eq!(
            (compaction.level, compaction.text.as_str()),
            (Level::Error, "Compaction failed: too small")
        );
    }

    // spec: UI-043
    #[test]
    fn unknown_frames_get_a_generic_row_with_their_json() {
        let row = notice_row(&NoticeFrame::Unknown(
            json!({"type": "irc_card", "from": "advisor"}),
        ));
        assert_eq!(
            (row.kind, row.text.as_str()),
            ("unknown", "Unrecognized irc_card frame")
        );
        assert_eq!(
            row.json.as_deref(),
            Some("{\n  \"from\": \"advisor\",\n  \"type\": \"irc_card\"\n}")
        );
    }
}
