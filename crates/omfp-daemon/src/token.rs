//! The owner's access token (DMN-003, PRT-002).

use std::fmt;
use std::io::Write;
use std::path::Path;

use subtle::ConstantTimeEq;
use tracing::warn;

use crate::paths::{create_private_dir, make_private_file, private_file_options};

/// Random secret every client presents in `system.hello`.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(…)")
    }
}

impl Token {
    /// 256 random bits, hex encoded.
    pub fn generate() -> std::io::Result<Self> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        Ok(Self(hex::encode(bytes)))
    }

    /// Reads the token at `path`, creating it (mode `0600`) on first run. A stored token that group or others may read
    /// is tightened to `0600` with a warning; one that cannot be (another user's file) is refused (DMN-003).
    pub fn load_or_create(path: &Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) if !text.trim().is_empty() => {
                if let Some(mode) = make_private_file(path)? {
                    warn!(
                        path = %path.display(),
                        mode = format_args!("{mode:o}"),
                        "the token file was readable by other users: its mode is now 0600; if others could read \
                         it, delete it and restart the daemon to get a new token"
                    );
                }
                return Ok(Self(text.trim().to_owned()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Some(parent) = path.parent() {
            create_private_dir(parent)?;
        }
        let token = Self::generate()?;
        let mut file = private_file_options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        make_private_file(path)?;
        file.write_all(token.0.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(token)
    }

    /// Constant-time comparison with a presented token (PRT-002).
    #[must_use]
    pub fn matches(&self, presented: &str) -> bool {
        self.0.as_bytes().ct_eq(presented.as_bytes()).into()
    }

    /// The secret itself, for printing a connection URL or handing to the in-process UI.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-003
    #[test]
    fn first_run_creates_a_256_bit_token_readable_only_by_the_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/token");

        let token = Token::load_or_create(&path).unwrap();

        assert_eq!(token.expose().len(), 64);
        assert!(token.expose().bytes().all(|byte| byte.is_ascii_hexdigit()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    // spec: DMN-003
    #[test]
    fn later_runs_reuse_the_stored_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");

        let first = Token::load_or_create(&path).unwrap();
        let second = Token::load_or_create(&path).unwrap();

        assert_eq!(first, second);
        assert_ne!(Token::generate().unwrap(), first);
    }

    // spec: DMN-003
    #[test]
    fn a_stored_token_others_may_read_is_tightened_to_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "abc\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert_eq!(Token::load_or_create(&path).unwrap().expose(), "abc");

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // spec: PRT-002
    #[test]
    fn matches_only_the_exact_token() {
        let token = Token("abc123".into());
        assert!(token.matches("abc123"));
        assert!(!token.matches("abc12"));
        assert!(!token.matches("abc1234"));
        assert!(!token.matches(""));
    }

    #[test]
    fn debug_output_never_shows_the_secret() {
        assert_eq!(format!("{:?}", Token("secret".into())), "Token(…)");
    }
}
