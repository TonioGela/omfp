//! The Flatpak sandbox of the Linux desktop package (ENV-064). omp, git and the shells of jobs act on the user's
//! machine (repositories, build tools, shells), which the sandbox does not have: inside it every process the daemon
//! starts for them runs on the host through `flatpak-spawn --host` (the `org.freedesktop.Flatpak` development portal).
//! Outside a sandbox commands run as they are.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

/// The file Flatpak puts in every sandbox.
const INFO_FILE: &str = "/.flatpak-info";
/// Where the app's files are mounted inside the sandbox.
const APP_MOUNT: &str = "/app";

/// The sandbox the daemon runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    /// The host path of the app's files (`app-path` of `/.flatpak-info`), mounted at `/app` inside.
    app_path: Option<PathBuf>,
}

/// The sandbox of this process, read once from `/.flatpak-info`; `None` outside Flatpak.
static SANDBOX: LazyLock<Option<Sandbox>> = LazyLock::new(|| {
    std::fs::read_to_string(INFO_FILE)
        .ok()
        .map(|info| Sandbox::from_info(&info))
});

/// The sandbox of this process; `None` outside Flatpak.
pub fn sandbox() -> Option<&'static Sandbox> {
    SANDBOX.as_ref()
}

/// `command` as it must run for the user: on the host when the daemon runs in a Flatpak sandbox (see
/// [`Sandbox::on_host`]), else unchanged. Set the program, arguments, directory and environment first, the standard
/// streams after.
#[must_use]
pub fn on_host(command: Command) -> Command {
    match sandbox() {
        Some(sandbox) => sandbox.on_host(&command),
        None => command,
    }
}

impl Sandbox {
    /// The sandbox described by the `/.flatpak-info` text `info`.
    #[must_use]
    pub fn from_info(info: &str) -> Self {
        let mut section = "";
        let mut app_path = None;
        for line in info.lines().map(str::trim) {
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|line| line.strip_suffix(']'))
            {
                section = name;
            } else if section == "Instance"
                && let Some(path) = line.strip_prefix("app-path=")
                && !path.is_empty()
            {
                app_path = Some(PathBuf::from(path));
            }
        }
        Self { app_path }
    }

    /// The host path of `path`: a file of the app (under `/app`) lives in the app's deployment on the host; any other
    /// path is the same on both sides (the home directory is shared, `--filesystem=home`).
    #[must_use]
    pub fn host_path(&self, path: &Path) -> PathBuf {
        match (&self.app_path, path.strip_prefix(APP_MOUNT)) {
            (Some(app_path), Ok(rest)) => app_path.join(rest),
            _ => path.to_path_buf(),
        }
    }

    /// `flatpak-spawn --host --watch-bus [--directory=<dir>] [--env=K=V | --unset-env=K]… -- <program> <args>…`
    /// for `command`: its working directory and environment changes go to the host process, which otherwise gets the
    /// host session's environment; `--watch-bus` ends it when `flatpak-spawn` dies (a SIGKILL cannot be forwarded).
    /// A program that is a file of the app is named by its host path.
    #[must_use]
    pub fn on_host(&self, command: &Command) -> Command {
        let mut host = Command::new("flatpak-spawn");
        host.args(["--host", "--watch-bus"]);
        if let Some(dir) = command.get_current_dir() {
            host.arg(joined(&["--directory=".as_ref(), dir.as_os_str()]));
        }
        for (key, value) in command.get_envs() {
            host.arg(match value {
                Some(value) => joined(&["--env=".as_ref(), key, "=".as_ref(), value]),
                None => joined(&["--unset-env=".as_ref(), key]),
            });
        }
        let program = Path::new(command.get_program());
        let program = if program.is_absolute() {
            self.host_path(program).into_os_string()
        } else {
            program.as_os_str().to_owned()
        };
        host.arg("--").arg(program).args(command.get_args());
        host
    }
}

fn joined(parts: &[&OsStr]) -> OsString {
    parts.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = "\
[Application]
name=io.github.aciceri.omfp
runtime=runtime/org.gnome.Platform/x86_64/51

[Instance]
instance-id=123
app-path=/home/u/.local/share/flatpak/app/io.github.aciceri.omfp/x86_64/master/abc/files
branch=master
";

    fn args(command: &Command) -> Vec<String> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    // spec: ENV-064
    #[test]
    fn the_app_path_comes_from_the_instance_section() {
        let sandbox = Sandbox::from_info(INFO);
        assert_eq!(
            sandbox.host_path(Path::new("/app/lib/omfp/omp")),
            Path::new(
                "/home/u/.local/share/flatpak/app/io.github.aciceri.omfp/x86_64/master/abc/files/lib/omfp/omp"
            )
        );
        // Paths outside the app are the host's own.
        assert_eq!(
            sandbox.host_path(Path::new("/home/u/.omfp/w")),
            Path::new("/home/u/.omfp/w")
        );
        assert_eq!(
            sandbox.host_path(Path::new("/application")),
            Path::new("/application")
        );
        // A runtime without an app (or a key in another section) has no app path.
        let bare = Sandbox::from_info("[Runtime]\napp-path=/x\n[Instance]\nbranch=51\n");
        assert_eq!(bare, Sandbox { app_path: None });
        assert_eq!(
            bare.host_path(Path::new("/app/lib/omfp/omp")),
            Path::new("/app/lib/omfp/omp")
        );
    }

    // spec: ENV-064
    #[test]
    fn a_command_runs_on_the_host_with_its_directory_and_environment() {
        let sandbox = Sandbox::from_info(INFO);
        let mut command = Command::new("/app/lib/omfp/omp");
        command
            .args(["--mode", "rpc-ui", "--session-dir", "/home/u/s"])
            .current_dir("/home/u/.omfp/w")
            .env("OMP_X", "a=b")
            .env_remove("NO_COLOR");
        let host = sandbox.on_host(&command);
        let mut expected = vec![
            "flatpak-spawn",
            "--host",
            "--watch-bus",
            "--directory=/home/u/.omfp/w",
        ];
        // `get_envs` yields the changes sorted by name.
        expected.extend(["--unset-env=NO_COLOR", "--env=OMP_X=a=b", "--"]);
        expected.push(
            "/home/u/.local/share/flatpak/app/io.github.aciceri.omfp/x86_64/master/abc/files/lib/omfp/omp",
        );
        expected.extend(["--mode", "rpc-ui", "--session-dir", "/home/u/s"]);
        assert_eq!(args(&host), expected);
        // A program named without a path is looked up on the host's PATH.
        let mut git = Command::new("git");
        git.arg("status");
        assert_eq!(
            args(&sandbox.on_host(&git)),
            [
                "flatpak-spawn",
                "--host",
                "--watch-bus",
                "--",
                "git",
                "status"
            ]
        );
    }

    // spec: ENV-064
    #[test]
    fn outside_a_sandbox_commands_run_as_they_are() {
        // The tests never run in Flatpak.
        assert_eq!(sandbox(), None);
        let mut command = Command::new("sh");
        command.args(["-c", "true"]);
        assert_eq!(args(&on_host(command)), ["sh", "-c", "true"]);
    }
}
