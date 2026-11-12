//! One-time tickets (DMN-186, PRT-096; experimental, branch idea/ticket). A URL a program opens or prints for a human
//! (the tray's `Open in browser`, the lines `omfp daemon` and `omfp desktop` print to a terminal) carries
//! `#ticket=<ticket>` instead of the token: a browser started with a URL keeps it in `/proc/<pid>/cmdline`, which
//! every local user can read for the browser's whole life, and a ticket read there later is used or expired. The
//! page trades the ticket for the token in `system.hello`. Tickets live in the daemon's memory, so a restart forgets
//! them.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use subtle::ConstantTimeEq;

/// A ticket's life unless asked otherwise: long enough for a browser to start and load the page.
pub const DEFAULT_TTL: Duration = Duration::from_secs(60);
/// The longest life a ticket may get: a URL printed to a terminal waits for a click.
pub const MAX_TTL: Duration = Duration::from_secs(600);
/// Tickets remembered at most, outstanding, used or expired; the oldest is forgotten first.
const KEPT: usize = 256;

/// What a presented ticket turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redeemed {
    /// Issued, unused and in time: now used.
    Valid,
    /// Redeemed before: a second use is refused, and says so (someone else may have opened the URL first).
    Used,
    /// Its time ran out unused.
    Expired,
    /// Never issued by this daemon, or forgotten.
    Unknown,
}

impl Redeemed {
    /// Why `system.hello` refuses the ticket; `None` for a valid one.
    #[must_use]
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Self::Valid => None,
            Self::Used => Some("ticket already used"),
            Self::Expired => Some("ticket expired"),
            Self::Unknown => Some("invalid ticket"),
        }
    }
}

struct Issued {
    secret: String,
    expires: Instant,
    used: bool,
}

/// The daemon's tickets; cheap to clone, usable from any thread.
#[derive(Clone, Default)]
pub struct Tickets(Arc<Mutex<Vec<Issued>>>);

impl fmt::Debug for Tickets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Tickets(…)")
    }
}

impl Tickets {
    /// A new single-use ticket (256 random bits, hex encoded) valid for `ttl`, at most [`MAX_TTL`].
    pub fn issue(&self, ttl: Duration) -> std::io::Result<String> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        let secret = hex::encode(bytes);
        let now = Instant::now();
        let mut issued = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        // A ticket expired for longer than any ticket lives is only remembered to say `expired` rather than `invalid`.
        issued.retain(|ticket| now.saturating_duration_since(ticket.expires) < MAX_TTL);
        if issued.len() >= KEPT {
            issued.remove(0);
        }
        issued.push(Issued {
            secret: secret.clone(),
            expires: now + ttl.min(MAX_TTL),
            used: false,
        });
        Ok(secret)
    }

    /// Uses `presented` once; every remembered ticket is compared in constant time.
    #[must_use]
    pub fn redeem(&self, presented: &str) -> Redeemed {
        let now = Instant::now();
        let mut issued = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut found = None;
        for (index, ticket) in issued.iter().enumerate() {
            if bool::from(ticket.secret.as_bytes().ct_eq(presented.as_bytes())) {
                found = Some(index);
            }
        }
        let Some(ticket) = found.map(|index| &mut issued[index]) else {
            return Redeemed::Unknown;
        };
        if ticket.used {
            Redeemed::Used
        } else if now >= ticket.expires {
            Redeemed::Expired
        } else {
            ticket.used = true;
            Redeemed::Valid
        }
    }
}

/// Issues ticket URLs of one daemon's UI: what the tray holds to open the UI in a browser.
#[derive(Debug, Clone)]
pub struct TicketUrls {
    base: String,
    tickets: Tickets,
}

impl TicketUrls {
    /// URLs on `base` (the UI's URL, `http://<addr>/`) with tickets of `tickets`.
    #[must_use]
    pub fn new(base: String, tickets: Tickets) -> Self {
        Self { base, tickets }
    }

    /// `<base>#ticket=<a new ticket valid for ttl>`.
    pub fn issue(&self, ttl: Duration) -> std::io::Result<String> {
        Ok(format!("{}#ticket={}", self.base, self.tickets.issue(ttl)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-186
    #[test]
    fn a_ticket_is_256_random_bits_valid_once() {
        let tickets = Tickets::default();
        let ticket = tickets.issue(DEFAULT_TTL).unwrap();
        assert_eq!(ticket.len(), 64);
        assert!(ticket.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(tickets.issue(DEFAULT_TTL).unwrap(), ticket);

        assert_eq!(tickets.redeem(&ticket), Redeemed::Valid);
        assert_eq!(tickets.redeem(&ticket), Redeemed::Used);
        assert_eq!(Redeemed::Used.refusal(), Some("ticket already used"));
    }

    // spec: DMN-186
    #[test]
    fn an_expired_or_unknown_ticket_is_refused() {
        let tickets = Tickets::default();
        let expired = tickets.issue(Duration::ZERO).unwrap();

        assert_eq!(tickets.redeem(&expired), Redeemed::Expired);
        assert_eq!(tickets.redeem(&expired), Redeemed::Expired, "still unused");
        assert_eq!(tickets.redeem("not-a-ticket"), Redeemed::Unknown);
        assert_eq!(tickets.redeem(""), Redeemed::Unknown);
        assert_eq!(
            Tickets::default().redeem(&expired),
            Redeemed::Unknown,
            "another daemon's ticket"
        );
    }

    // spec: DMN-186
    #[test]
    fn only_the_latest_tickets_are_remembered() {
        let tickets = Tickets::default();
        let first = tickets.issue(DEFAULT_TTL).unwrap();
        let second = tickets.issue(DEFAULT_TTL).unwrap();
        for _ in 2..KEPT {
            tickets.issue(DEFAULT_TTL).unwrap();
        }
        let _ = tickets.issue(DEFAULT_TTL).unwrap();

        assert_eq!(tickets.redeem(&first), Redeemed::Unknown);
        assert_eq!(tickets.redeem(&second), Redeemed::Valid);
    }

    // spec: DMN-186
    #[test]
    fn ticket_urls_carry_a_fresh_ticket_in_the_fragment() {
        let tickets = Tickets::default();
        let urls = TicketUrls::new("http://127.0.0.1:7420/".into(), tickets.clone());
        let url = urls.issue(DEFAULT_TTL).unwrap();

        let ticket = url
            .strip_prefix("http://127.0.0.1:7420/#ticket=")
            .expect(&url);
        assert_eq!(tickets.redeem(ticket), Redeemed::Valid);
        assert_ne!(urls.issue(DEFAULT_TTL).unwrap(), url);
        assert_eq!(format!("{tickets:?}"), "Tickets(…)");
    }
}
