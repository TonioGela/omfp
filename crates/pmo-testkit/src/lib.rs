//! Test helpers shared by the workspace (docs/specs/06-testing.md).
//!
//! - [`TempRepo`]: a throwaway git repository with an initial commit.
//! - [`DenyProxy`]: an HTTP(S) proxy that refuses and records every connection, to catch stray network access.
//! - [`OmpEnv`]: an isolated omp home and agent directory wired to a [`fake_llm::FakeLlm`].
//! - [`OmpSession`]: a live `omp --mode rpc-ui` process with every raw stdout frame recorded.

mod deny_proxy;
mod omp;
mod repo;

pub use deny_proxy::DenyProxy;
pub use omp::{
    FAKE_MODEL, FAKE_PROVIDER, OmpEnv, OmpSession, normalize_frame, omp_version, require_pinned_omp,
};
pub use repo::TempRepo;

/// Hosts the pinned omp contacts on its own at startup for background model-catalog discovery (S-02): the shared
/// models.dev mirror and the model lists of keyless providers. No setting disables it, so contract tests run omp
/// behind a [`DenyProxy`] that refuses them, and fail on any host outside this list (TST-021). Review this list on
/// every omp upgrade.
pub const KNOWN_DISCOVERY_HOSTS: &[&str] = &[
    "catalog.stencil.so",
    "coding-intl.dashscope.aliyuncs.com",
    "hyper.charm.land",
    "api.commandcode.ai",
    "api.kilo.ai",
    "api.venice.ai",
    "zenmux.ai",
];
