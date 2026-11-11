//! Views and widgets (02-daemon "Views and widgets (M4)", 08 AGT-010..025): the views manifest of a workspace, the
//! widget folders of the workspace and of the user library with their versions, serving widget files (TypeScript and
//! JSX transpiled with oxc), copying widgets, and the read-only `omfp://` host URIs agents learn widgets from.
//!
//! Everything here is blocking file work; the server runs it on blocking tasks.

pub mod compile;
pub mod folders;
pub mod manifest;

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use omfp_protocol::{WidgetInfo, WidgetSource};
use omp_rpc::{HostUri, HostUriRead, HostUriResultContentType};
use serde_json::Value;
use tracing::warn;

use crate::store::new_id;
use compile::{CompileError, Exports};
use folders::{files, is_widget_name};

/// The widget SDK and kit reference, served as [`DOCS_URL`] (AGT-025).
pub const DOCS: &str = include_str!("widgets/docs.md");
/// The JSON Schema of the views manifest, served as [`SCHEMA_URL`] (AGT-010, AGT-025).
pub const VIEWS_SCHEMA: &str = include_str!("widgets/views.schema.json");
/// Host URI of [`DOCS`] (DMN-062).
pub const DOCS_URL: &str = "omfp://docs/widgets";
/// Host URI of [`VIEWS_SCHEMA`] (DMN-062).
pub const SCHEMA_URL: &str = "omfp://schemas/views.json";
/// The host URI scheme of the daemon (DMN-062).
pub const SCHEME: &str = "omfp";

/// Where a workspace keeps its views and widgets.
const OMFP_DIR: &str = ".omfp";

/// `<workspace>/.omfp/views.json` (AGT-010).
#[must_use]
pub fn manifest_path(workspace: &Path) -> PathBuf {
    workspace.join(OMFP_DIR).join("views.json")
}

/// `<workspace>/.omfp/widgets/`, the workspace's widget folders (AGT-012).
#[must_use]
pub fn workspace_widgets(workspace: &Path) -> PathBuf {
    workspace.join(OMFP_DIR).join("widgets")
}

/// What an `omfp://` URL reads (DMN-062): the docs, the schema, or an error naming both.
pub fn read_uri(url: &str) -> Result<HostUriRead, String> {
    match url.trim_end_matches('/') {
        DOCS_URL => Ok(HostUriRead {
            content: DOCS.to_owned(),
            content_type: Some(HostUriResultContentType::TextMarkdown),
            immutable: Some(true),
            ..HostUriRead::default()
        }),
        SCHEMA_URL => Ok(HostUriRead {
            content: VIEWS_SCHEMA.to_owned(),
            content_type: Some(HostUriResultContentType::ApplicationJson),
            immutable: Some(true),
            ..HostUriRead::default()
        }),
        _ => Err(format!(
            "unknown omfp URL {url}: omfp serves {DOCS_URL} (the widget SDK and kit reference) and {SCHEMA_URL} (the \
             views manifest schema)"
        )),
    }
}

/// The host URI schemes of every thread (DMN-062): `omfp://`, read-only.
pub fn host_uris() -> Result<Vec<HostUri>, omp_rpc::Error> {
    let omfp = HostUri::new(SCHEME, |url, _context| read_uri(url).map_err(Into::into))?
        .description(format!(
            "omfp, the window the user works in: {DOCS_URL} documents widgets (SDK, kit, views); {SCHEMA_URL} is the \
             JSON Schema of .omfp/views.json"
        ))
        .immutable(true);
    Ok(vec![omfp])
}

/// A file served from a widget folder (DMN-084).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Served {
    /// JavaScript made by the daemon: a transpiled file or a stand-in module (DMN-085).
    Module(String),
    /// A file served as it is; `path` gives its content type.
    File { path: PathBuf, bytes: Vec<u8> },
}

/// Why `widgets.copy` failed (DMN-087).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CopyError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Io(String),
}

/// A compiled file, valid while its size and modification time stay.
#[derive(Debug)]
struct Compiled {
    size: u64,
    modified: Option<SystemTime>,
    result: Result<String, CompileError>,
}

/// The widget libraries the daemon serves: the user library, the built-ins of the web bundle, and the compile cache.
#[derive(Debug)]
pub struct Widgets {
    user_library: PathBuf,
    builtin: Option<PathBuf>,
    exports: Option<Exports>,
    compiled: Mutex<HashMap<PathBuf, Compiled>>,
}

impl Widgets {
    /// The user library is `<data-dir>/widgets/` (AGT-023); with a web bundle (`static_dir`), the built-ins are its
    /// `builtin-widgets/` and the names the bundle exports are its `widget-exports.json` (DMN-085, DMN-087).
    #[must_use]
    pub fn new(user_library: PathBuf, static_dir: Option<&Path>) -> Self {
        let exports = static_dir.and_then(|dir| {
            let path = dir.join("widget-exports.json");
            match std::fs::read_to_string(&path) {
                Ok(text) => compile::parse_exports(&text)
                    .inspect_err(
                        |error| warn!(path = %path.display(), %error, "invalid widget exports"),
                    )
                    .ok(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    warn!(path = %path.display(), %error, "cannot read the widget exports");
                    None
                }
            }
        });
        Self {
            user_library,
            builtin: static_dir.map(|dir| dir.join("builtin-widgets")),
            exports,
            compiled: Mutex::new(HashMap::new()),
        }
    }

    /// `<data-dir>/widgets/` (AGT-023).
    #[must_use]
    pub fn user_library(&self) -> &Path {
        &self.user_library
    }

    /// `<static-dir>/builtin-widgets/` when the daemon serves a bundle that has it (AGT-021).
    #[must_use]
    pub fn builtin_dir(&self) -> Option<&Path> {
        self.builtin.as_deref().filter(|dir| dir.is_dir())
    }

    /// The manifest of the workspace at `workspace` and why it is refused (AGT-010, AGT-011, DMN-082).
    #[must_use]
    pub fn manifest(workspace: &Path) -> (Option<Value>, Option<String>) {
        manifest::read(&manifest_path(workspace))
    }

    /// The workspace's widget folders, then the user library's, each by name (DMN-082).
    #[must_use]
    pub fn widgets(&self, workspace: &Path) -> Vec<WidgetInfo> {
        let mut widgets = folders::scan(&workspace_widgets(workspace), WidgetSource::Workspace);
        widgets.extend(folders::scan(&self.user_library, WidgetSource::User));
        widgets
    }

    /// The file at `path` of widget folder `name` in `library` (DMN-084, DMN-085); `None` (404) when the folder or
    /// the file does not exist or `path` leaves the folder.
    #[must_use]
    pub fn serve(&self, library: &Path, name: &str, path: &str) -> Option<Served> {
        if !is_widget_name(name) {
            return None;
        }
        let folder = std::fs::canonicalize(library.join(name)).ok()?;
        if !folder.is_dir() {
            return None;
        }
        let file = resolve(&folder, &relative_path(path)?)?;
        let is_entry = folders::entry(&folder)
            .and_then(|entry| std::fs::canonicalize(folder.join(entry)).ok())
            .is_some_and(|entry| entry == file);
        if is_entry {
            // The entry stands for the whole folder: the first file that does not compile replaces it.
            let failure = files(&folder)
                .into_iter()
                .filter(|(relative, _)| compile::is_compiled(relative))
                .find_map(|(relative, _)| self.compile(&folder, &relative).err());
            if let Some(error) = failure {
                return Some(Served::Module(compile::stand_in(&error)));
            }
        }
        let relative = file.strip_prefix(&folder).ok()?.to_owned();
        if compile::is_compiled(&relative) {
            return Some(Served::Module(
                self.compile(&folder, &relative)
                    .unwrap_or_else(|error| compile::stand_in(&error)),
            ));
        }
        let bytes = std::fs::read(&file).ok()?;
        Some(Served::File { path: file, bytes })
    }

    /// The compiled file `relative` of `folder`, from the cache while its size and modification time stay.
    fn compile(&self, folder: &Path, relative: &Path) -> Result<String, CompileError> {
        let path = folder.join(relative);
        let unreadable = |error: &std::io::Error| CompileError {
            file: compile::display(relative),
            line: None,
            column: None,
            message: format!("cannot read the file: {error}"),
        };
        let metadata = std::fs::metadata(&path).map_err(|error| unreadable(&error))?;
        let (size, modified) = (metadata.len(), metadata.modified().ok());
        {
            let cache = self.compiled.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(hit) = cache.get(&path)
                && hit.size == size
                && hit.modified == modified
            {
                return hit.result.clone();
            }
        }
        let source = std::fs::read_to_string(&path).map_err(|error| unreadable(&error))?;
        let result = compile::compile(relative, &source, self.exports.as_ref());
        self.compiled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                path,
                Compiled {
                    size,
                    modified,
                    result: result.clone(),
                },
            );
        result
    }

    /// Copies `omfp:<name>` (a built-in) or `user:<name>` into `<workspace>/.omfp/widgets/<new name>/`, the new name
    /// being `name` else the widget's own (DMN-087); returns the new name.
    pub fn copy(
        &self,
        workspace: &Path,
        widget: &str,
        name: Option<&str>,
    ) -> Result<String, CopyError> {
        let (library, source) = if let Some(source) = widget.strip_prefix("omfp:") {
            let Some(builtin) = self.builtin.as_deref() else {
                return Err(CopyError::NotFound(
                    "no built-in widgets: the daemon serves no web bundle".to_owned(),
                ));
            };
            (builtin, source)
        } else if let Some(source) = widget.strip_prefix("user:") {
            (self.user_library.as_path(), source)
        } else {
            return Err(CopyError::BadRequest(format!(
                "cannot copy {widget:?}: expected omfp:<name> or user:<name>"
            )));
        };
        let target = name.unwrap_or(source);
        for name in [source, target] {
            if !is_widget_name(name) {
                return Err(CopyError::BadRequest(format!(
                    "invalid widget name {name:?}: ASCII letters, digits, `_`, `-` and `.` (not first)"
                )));
            }
        }
        let from = library.join(source);
        if !from.is_dir() {
            return Err(CopyError::NotFound(format!("no widget {widget}")));
        }
        let widgets = workspace_widgets(workspace);
        let to = widgets.join(target);
        if std::fs::symlink_metadata(&to).is_ok() {
            return Err(CopyError::Conflict(format!(
                "the workspace already has a widget {target}"
            )));
        }
        // Copied under a hidden name (never listed nor served), then renamed: nobody sees half a widget.
        let staging = widgets.join(format!(".{target}.{}.tmp", new_id()));
        let io = |error: std::io::Error| CopyError::Io(format!("cannot copy {widget}: {error}"));
        let copied = std::fs::create_dir_all(&widgets)
            .and_then(|()| copy_dir(&from, &staging))
            .and_then(|()| std::fs::rename(&staging, &to));
        if let Err(error) = copied {
            let _ = std::fs::remove_dir_all(&staging);
            if std::fs::symlink_metadata(&to).is_ok() {
                return Err(CopyError::Conflict(format!(
                    "the workspace already has a widget {target}"
                )));
            }
            return Err(io(error));
        }
        Ok(target.to_owned())
    }
}

/// Copies the folder `from` to the new folder `to`: files (symlinks to files as their content) and real
/// subdirectories; symlinks to directories are left out.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            copy_dir(&source, &target)?;
        } else if std::fs::metadata(&source)?.is_file() {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

/// The request path as a relative path without `..`; `None` when it is empty or tries to leave the folder.
fn relative_path(path: &str) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!relative.as_os_str().is_empty()).then_some(relative)
}

/// The file `relative` names in `folder` (canonical `folder`): itself, else, without extension, the first of
/// `.tsx`, `.ts`, `.jsx`, `.js`; never outside the folder, even through a symlink (DMN-084).
fn resolve(folder: &Path, relative: &Path) -> Option<PathBuf> {
    let path = folder.join(relative);
    let candidates = if relative.extension().is_some() {
        vec![path]
    } else {
        std::iter::once(path.clone())
            .chain(["tsx", "ts", "jsx", "js"].into_iter().map(|extension| {
                let mut with = path.clone().into_os_string();
                with.push(".");
                with.push(extension);
                PathBuf::from(with)
            }))
            .collect()
    };
    candidates
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .find_map(|candidate| std::fs::canonicalize(candidate).ok())
        .filter(|file| file.starts_with(folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-062, AGT-025
    #[test]
    fn omfp_urls_read_the_docs_and_the_schema_and_name_both_otherwise() {
        let docs = read_uri("omfp://docs/widgets").unwrap();
        assert_eq!(docs.content, DOCS);
        assert_eq!(
            docs.content_type,
            Some(HostUriResultContentType::TextMarkdown)
        );
        let schema = read_uri("omfp://schemas/views.json").unwrap();
        assert_eq!(schema.content, VIEWS_SCHEMA);
        let parsed: Value = serde_json::from_str(&schema.content).unwrap();
        assert_eq!(parsed["$id"], SCHEMA_URL);

        let error = read_uri("omfp://docs/nothing").unwrap_err();
        assert!(error.contains("omfp://docs/nothing"), "{error}");
        assert!(
            error.contains(DOCS_URL) && error.contains(SCHEMA_URL),
            "{error}"
        );
    }

    // spec: DMN-062
    #[test]
    fn the_omfp_scheme_is_read_only() {
        let uris = host_uris().unwrap();
        assert_eq!(uris.len(), 1);
        assert_eq!(uris[0].scheme(), "omfp");
        assert!(!uris[0].writable());
        assert_eq!(uris[0].definition().immutable, Some(true));
    }

    #[test]
    fn request_paths_stay_relative() {
        assert_eq!(
            relative_path("lib/chart.ts"),
            Some(PathBuf::from("lib/chart.ts"))
        );
        assert_eq!(
            relative_path("./index.tsx"),
            Some(PathBuf::from("index.tsx"))
        );
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a/.."] {
            assert_eq!(relative_path(bad), None, "{bad:?}");
        }
    }
}
