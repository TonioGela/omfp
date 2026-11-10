//! Version pinning checks (ENV-010, ENV-011): everything that names the omp release must agree.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, anyhow};
use regex::Regex;
use serde_json::Value;

/// Versions gathered from the repository files and the tools on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// `ref` of the `oh-my-pi` input in `flake.lock`, e.g. `v18.18.6`.
    pub flake_ref: String,
    /// Locked commit of the `oh-my-pi` input.
    pub flake_rev: String,
    /// Tag of the `omp-rpc` git source in `Cargo.lock`.
    pub cargo_tag: String,
    /// Commit of the `omp-rpc` git source in `Cargo.lock`.
    pub cargo_rev: String,
    /// `omfp_core::OMP_VERSION`.
    pub code_version: String,
    /// Version printed by `omp --version`.
    pub omp_version: String,
}

/// Every disagreement between the observed versions; empty when consistent.
#[must_use]
pub fn mismatches(observed: &Observed) -> Vec<String> {
    let mut problems = Vec::new();
    let release = Regex::new(r"^v\d+\.\d+\.\d+$").expect("valid regex");
    if !release.is_match(&observed.flake_ref) {
        problems.push(format!(
            "ENV-011: flake input oh-my-pi must point at a release tag, not `{}`",
            observed.flake_ref
        ));
    }
    let expected_tag = format!("v{}", observed.code_version);
    if observed.flake_ref != expected_tag {
        problems.push(format!(
            "ENV-010: flake.lock oh-my-pi ref `{}` != v(omfp_core::OMP_VERSION) `{expected_tag}`",
            observed.flake_ref
        ));
    }
    if observed.cargo_tag != expected_tag {
        problems.push(format!(
            "ENV-010: Cargo.lock omp-rpc tag `{}` != `{expected_tag}`",
            observed.cargo_tag
        ));
    }
    if observed.flake_rev != observed.cargo_rev {
        problems.push(format!(
            "ENV-010: flake.lock oh-my-pi rev `{}` != Cargo.lock omp-rpc rev `{}`",
            observed.flake_rev, observed.cargo_rev
        ));
    }
    if observed.omp_version != observed.code_version {
        problems.push(format!(
            "ENV-010: `omp --version` is `{}` but omfp_core::OMP_VERSION is `{}`",
            observed.omp_version, observed.code_version
        ));
    }
    problems
}

/// `(ref, rev)` of the `oh-my-pi` input in a `flake.lock`.
pub fn flake_omp_pin(flake_lock: &str) -> anyhow::Result<(String, String)> {
    let lock: Value = serde_json::from_str(flake_lock).context("parsing flake.lock")?;
    let node = &lock["nodes"]["oh-my-pi"];
    let reference = node["original"]["ref"]
        .as_str()
        .ok_or_else(|| anyhow!("flake.lock: oh-my-pi has no ref"))?;
    let rev = node["locked"]["rev"]
        .as_str()
        .ok_or_else(|| anyhow!("flake.lock: oh-my-pi has no locked rev"))?;
    Ok((reference.to_owned(), rev.to_owned()))
}

/// `(tag, rev)` of the `omp-rpc` git source in a `Cargo.lock`.
pub fn cargo_omp_rpc_pin(cargo_lock: &str) -> anyhow::Result<(String, String)> {
    let source =
        Regex::new(r##"source = "git\+[^"?]+\?tag=([^"#]+)#([0-9a-f]+)""##).expect("valid regex");
    let package = cargo_package(cargo_lock, "omp-rpc")?;
    let captures = source
        .captures(package)
        .ok_or_else(|| anyhow!("Cargo.lock: omp-rpc is not a tagged git source"))?;
    Ok((captures[1].to_owned(), captures[2].to_owned()))
}

fn cargo_package<'a>(cargo_lock: &'a str, name: &str) -> anyhow::Result<&'a str> {
    let header = format!("[[package]]\nname = \"{name}\"\n");
    let start = cargo_lock
        .find(&header)
        .ok_or_else(|| anyhow!("Cargo.lock: no package {name}"))?;
    let rest = &cargo_lock[start + header.len()..];
    Ok(&rest[..rest.find("\n\n").unwrap_or(rest.len())])
}

/// First `x.y.z` in a tool's `--version` output (`omp/18.18.6`).
#[must_use]
pub fn version_in(output: &str) -> Option<String> {
    Regex::new(r"\d+\.\d+\.\d+")
        .expect("valid regex")
        .find(output)
        .map(|found| found.as_str().to_owned())
}

fn tool_version(program: &str) -> anyhow::Result<String> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .with_context(|| format!("running `{program} --version`; is the dev shell active?"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    version_in(&stdout)
        .ok_or_else(|| anyhow!("no version in `{program} --version` output: {stdout:?}"))
}

/// Reads every version from the repository at `root` and the tools on `PATH`.
pub fn observe(root: &Path) -> anyhow::Result<Observed> {
    let flake_lock =
        std::fs::read_to_string(root.join("flake.lock")).context("reading flake.lock")?;
    let cargo_lock =
        std::fs::read_to_string(root.join("Cargo.lock")).context("reading Cargo.lock")?;
    let (flake_ref, flake_rev) = flake_omp_pin(&flake_lock)?;
    let (cargo_tag, cargo_rev) = cargo_omp_rpc_pin(&cargo_lock)?;
    Ok(Observed {
        flake_ref,
        flake_rev,
        cargo_tag,
        cargo_rev,
        code_version: omfp_core::OMP_VERSION.to_owned(),
        omp_version: tool_version("omp")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consistent() -> Observed {
        Observed {
            flake_ref: "v18.18.6".into(),
            flake_rev: "9058ab9".into(),
            cargo_tag: "v18.18.6".into(),
            cargo_rev: "9058ab9".into(),
            code_version: "18.18.6".into(),
            omp_version: "18.18.6".into(),
        }
    }

    // spec: ENV-010, ENV-011
    #[test]
    fn consistent_versions_have_no_mismatches() {
        assert_eq!(mismatches(&consistent()), Vec::<String>::new());
    }

    // spec: ENV-010
    #[test]
    fn every_omp_pin_disagreement_is_reported() {
        let mut observed = consistent();
        observed.cargo_tag = "v18.18.5".into();
        observed.cargo_rev = "aaaaaaa".into();
        observed.omp_version = "18.18.1".into();

        let problems = mismatches(&observed);

        assert_eq!(problems.len(), 3, "{problems:#?}");
        assert!(
            problems
                .iter()
                .all(|problem| problem.starts_with("ENV-010"))
        );
    }

    // spec: ENV-011
    #[test]
    fn a_branch_instead_of_a_release_tag_is_rejected() {
        let mut observed = consistent();
        observed.flake_ref = "main".into();

        let problems = mismatches(&observed);

        assert!(
            problems
                .iter()
                .any(|problem| problem.starts_with("ENV-011")),
            "{problems:#?}"
        );
    }

    #[test]
    fn parses_the_flake_lock_omp_pin() {
        let lock =
            r#"{"nodes":{"oh-my-pi":{"locked":{"rev":"abc123"},"original":{"ref":"v1.2.3"}}}}"#;
        assert_eq!(
            flake_omp_pin(lock).unwrap(),
            ("v1.2.3".to_owned(), "abc123".to_owned())
        );
        assert!(flake_omp_pin(r#"{"nodes":{}}"#).is_err());
    }

    #[test]
    fn parses_the_cargo_lock_git_source() {
        let lock = "\
[[package]]
name = \"omfp-core\"
version = \"0.1.0\"

[[package]]
name = \"omp-rpc\"
version = \"0.1.0\"
source = \"git+https://github.com/can1357/oh-my-pi?tag=v18.18.6#9058ab9d2296\"
";
        assert_eq!(
            cargo_omp_rpc_pin(lock).unwrap(),
            ("v18.18.6".to_owned(), "9058ab9d2296".to_owned())
        );
        assert!(cargo_omp_rpc_pin("[[package]]\nname = \"omfp-core\"\n").is_err());
    }

    #[test]
    fn finds_the_version_in_tool_output() {
        assert_eq!(version_in("omp/18.18.6\n").as_deref(), Some("18.18.6"));
        assert_eq!(
            version_in("omp 18.18.6 (9058ab9)").as_deref(),
            Some("18.18.6")
        );
        assert_eq!(version_in("no version here"), None);
    }
}
