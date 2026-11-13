//! A launch without a subcommand (ENV-063): the release `omfp.app` has no wrapper around its executable,
//! `Contents/MacOS/omfp`, which the Finder, the Dock and `open` run without arguments: there it means `omfp desktop`.

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Whether `exe` is the executable of a macOS app bundle, `<name>.app/Contents/MacOS/<exe>`.
#[must_use]
pub fn in_app_bundle(exe: &Path) -> bool {
    let mut dirs = exe.ancestors().skip(1).map(Path::file_name);
    dirs.next() == Some(Some(OsStr::new("MacOS")))
        && dirs.next() == Some(Some(OsStr::new("Contents")))
        && dirs
            .next()
            .flatten()
            .and_then(|name| Path::new(name).extension())
            .is_some_and(|extension| extension == "app")
}

/// The command line to parse: `omfp desktop` when an app bundle's executable runs without arguments (older macOS
/// versions pass a `-psn_…` process serial number, which is dropped), else `args` as given.
#[must_use]
pub fn args(args: Vec<OsString>, exe: Option<&Path>) -> Vec<OsString> {
    let bare = args
        .iter()
        .skip(1)
        .all(|arg| arg.to_string_lossy().starts_with("-psn_"));
    match args.first() {
        Some(program) if bare && exe.is_some_and(in_app_bundle) => {
            vec![program.clone(), OsString::from("desktop")]
        }
        _ => args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    // spec: ENV-063
    #[test]
    fn an_app_bundle_executable_is_recognised_by_its_place() {
        assert!(in_app_bundle(Path::new(
            "/Applications/omfp.app/Contents/MacOS/omfp"
        )));
        assert!(!in_app_bundle(Path::new("/usr/local/bin/omfp")));
        assert!(!in_app_bundle(Path::new("/opt/omfp/Contents/MacOS/omfp")));
        assert!(!in_app_bundle(Path::new("/x.app/MacOS/omfp")));
        assert!(!in_app_bundle(Path::new("omfp")));
    }

    // spec: ENV-063
    #[test]
    fn a_bare_launch_of_the_app_opens_the_desktop_window() {
        let app = Path::new("/Applications/omfp.app/Contents/MacOS/omfp");
        let desktop = strings(&["omfp", "desktop"]);
        assert_eq!(args(strings(&["omfp"]), Some(app)), desktop);
        assert_eq!(args(strings(&["omfp", "-psn_0_12345"]), Some(app)), desktop);
        // Arguments, or an executable outside an app bundle, are parsed as given.
        assert_eq!(
            args(strings(&["omfp", "--version"]), Some(app)),
            strings(&["omfp", "--version"])
        );
        assert_eq!(
            args(strings(&["omfp", "daemon"]), Some(app)),
            strings(&["omfp", "daemon"])
        );
        let bin = Path::new("/usr/local/bin/omfp");
        assert_eq!(args(strings(&["omfp"]), Some(bin)), strings(&["omfp"]));
        assert_eq!(args(strings(&["omfp"]), None), strings(&["omfp"]));
    }
}
