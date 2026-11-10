//! Queue chips (UI-062): rendered only from the last `queue_update` (`ThreadState::queue`). Remove and promote mark a
//! chip as pending; the chip itself changes only when the next `queue_update` arrives.

use omfp_core::QueueState;
use omfp_core::wire::QueuedMessageQueue;

use crate::intent::QueueOp;

/// A queue action sent and not yet answered.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingQueueOp {
    pub thread_id: String,
    pub op: QueueOp,
    pub message: String,
}

/// One chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueChip {
    pub queue: QueuedMessageQueue,
    pub text: String,
    /// An action on this text waits for the daemon.
    pub pending: Option<QueueOp>,
    /// Follow-ups can be promoted to steering.
    pub can_promote: bool,
}

impl QueueChip {
    #[must_use]
    pub fn queue_token(&self) -> &'static str {
        match self.queue {
            QueuedMessageQueue::Steering => "steering",
            QueuedMessageQueue::FollowUp => "follow-up",
        }
    }
}

/// Steering chips, then follow-ups, each in queue order.
#[must_use]
pub fn queue_chips(
    queue: &QueueState,
    thread_id: &str,
    pending: &[PendingQueueOp],
) -> Vec<QueueChip> {
    let chip = |kind: QueuedMessageQueue, text: &String| QueueChip {
        queue: kind,
        text: text.clone(),
        pending: pending
            .iter()
            .find(|op| op.thread_id == thread_id && op.message == *text)
            .map(|op| op.op),
        can_promote: kind == QueuedMessageQueue::FollowUp,
    };
    queue
        .steering
        .iter()
        .map(|text| chip(QueuedMessageQueue::Steering, text))
        .chain(
            queue
                .follow_up
                .iter()
                .map(|text| chip(QueuedMessageQueue::FollowUp, text)),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, fixture_prefix};

    // spec: UI-062
    #[test]
    fn chips_follow_the_last_queue_update_and_show_pending_actions() {
        let queued = fixture_prefix("follow_up_then_abort", 12);
        let pending = vec![PendingQueueOp {
            thread_id: "t".to_owned(),
            op: QueueOp::Remove,
            message: "later please".to_owned(),
        }];
        let chips = queue_chips(&queued.queue, "t", &pending);
        assert_eq!(
            chips,
            vec![QueueChip {
                queue: QueuedMessageQueue::FollowUp,
                text: "later please".to_owned(),
                pending: Some(QueueOp::Remove),
                can_promote: true,
            }]
        );
        assert_eq!(
            queue_chips(&queued.queue, "other", &pending)[0].pending,
            None
        );
        let drained = fixture("follow_up_then_abort");
        assert_eq!(queue_chips(&drained.queue, "t", &pending), Vec::new());
    }
}
