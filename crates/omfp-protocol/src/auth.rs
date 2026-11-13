//! Params and results of logging omp in to a provider (PRT-129..PRT-133): `auth.providers`, `auth.login`,
//! `auth.login_cancel` and `auth.logout`. The flow of a login reaches a window as `login_*` window requests
//! ([`crate::WindowRequest`]).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `auth.providers` result (PRT-129): the providers omp can log in to with OAuth, in omp's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthProviders {
    pub providers: Vec<AuthProvider>,
}

/// One provider of `auth.providers` (PRT-129), as omp's `get_login_providers` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthProvider {
    /// omp's id, e.g. `anthropic`, `openai-codex`: what `auth.login` takes.
    pub id: String,
    /// What the user reads, e.g. `Anthropic (Claude Pro/Max)`.
    pub name: String,
    /// omp can run its login here.
    pub available: bool,
    /// omp holds a credential for it (a login, or a key from the environment or omp's config).
    pub authenticated: bool,
}

/// `auth.login` and `auth.logout` params: the provider's omp id (PRT-130, PRT-133).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRef {
    pub provider_id: String,
}

/// `auth.login` result (PRT-130): omp stored the provider's credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoggedIn {
    pub provider_id: String,
    /// The provider's name, as `auth.providers` gives it.
    pub name: String,
}

/// `auth.login_cancel` result (PRT-132).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginCancelled {
    /// The provider whose login was cancelled; `null` when none ran.
    pub provider_id: Option<String>,
}

/// `auth.logout` result (PRT-133).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoggedOut {
    pub provider_id: String,
    /// How many stored credentials (logins and stored keys) of the provider omp removed.
    pub removed: u32,
    /// What still authenticates the provider once they are gone, as omp names it (e.g. an environment variable);
    /// `null` when nothing does.
    pub remaining_source: Option<String>,
}
