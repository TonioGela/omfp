//! Mounts and the component library (AGT-060..065): `views.mount`, `views.mount_update` and `views.unmount`
//! (PRT-080..082) edit the workspace's `views.json` with [`crate::widgets::mounts`] and publish it as `views.set` does;
//! `widgets.save` and `widgets.library` (PRT-083, PRT-084) work on the user library.

use std::sync::{Arc, PoisonError};

use omfp_protocol::{
    ErrorBody, ErrorCode, LibraryComponent, MountParams, MountPlace, MountUpdateParams, Mounted,
    SaveWidgetParams, UnmountParams, Unmounted, WidgetSaved,
};
use serde_json::Value;

use super::views::copy_error;
use super::{Services, blocking};
use crate::paths::replace_file;
use crate::server::convert::{bad_request, internal};
use crate::store::Store;
use crate::widgets::folders::{ENTRIES, MOUNT_MARKER, is_widget_name};
use crate::widgets::layout::Place;
use crate::widgets::library::{self, Source};
use crate::widgets::mounts::{self, Mount, MountError, Target};
use crate::widgets::{Widgets, manifest, manifest_path, workspace_widgets};

fn mount_error(error: MountError) -> ErrorBody {
    match error {
        MountError::Invalid(message) => bad_request(message),
        MountError::NotFound(message) => ErrorBody::new(ErrorCode::NotFound, message),
    }
}

/// `id` when it is a widget name (AGT-061).
fn mount_id(id: &str) -> Result<&str, ErrorBody> {
    let id = id.trim();
    if is_widget_name(id) {
        Ok(id)
    } else {
        Err(bad_request(format!(
            "invalid mount id {id:?}: ASCII letters, digits, `_`, `-` and `.` (not first)"
        )))
    }
}

/// A widget reference as the agent wrote it: a bare name means `./widgets/<name>` (AGT-061).
fn reference(widget: &str) -> Result<String, ErrorBody> {
    let widget = widget.trim();
    if widget.is_empty() {
        return Err(bad_request("`widget` is empty"));
    }
    Ok(
        if widget.starts_with("./widgets/")
            || widget.starts_with("user:")
            || widget.starts_with("omfp:")
        {
            widget.to_owned()
        } else {
            format!("./widgets/{widget}")
        },
    )
}

fn place(place: MountPlace) -> Place {
    match place {
        MountPlace::SideTab => Place::SideTab,
        MountPlace::MainTab => Place::MainTab,
        MountPlace::End => Place::End,
    }
}

/// The valid manifest of the workspace at `id` (`None` without a file) and its folder; an invalid file is a
/// `bad_request` naming its error (PRT-080).
fn current(store: &Store, id: &str) -> Result<(std::path::PathBuf, Option<Value>), ErrorBody> {
    let path = store.workspace(id)?.path;
    let (manifest, error) = Widgets::manifest(&path);
    if let Some(error) = error {
        return Err(bad_request(format!(
            ".omfp/views.json is invalid ({error}); fix it first (or ask the user to run `:view-reset`)"
        )));
    }
    Ok((path, manifest))
}

/// Whether the code of mount `id` may go to `.omfp/widgets/<id>/` of the workspace at `workspace` (DMN-183): not when
/// `id` names a built-in (the folder would replace `omfp:<id>` in the whole workspace), nor when the folder exists
/// without this mount's marker or has another entry file (which `index.tsx` would shadow or that would shadow it).
/// `Ok(true)` when the mount's own folder exists: its code is replaced. `update` says the conflict as
/// `views.mount_update`'s, which cannot choose another id.
fn claim_folder(
    workspace: &std::path::Path,
    id: &str,
    builtins: &[String],
    update: bool,
) -> Result<bool, ErrorBody> {
    let conflict = |why: String| {
        let message = if update {
            format!(
                "cannot update the code of mount {id}: {why}; mount the code again under another id"
            )
        } else {
            format!("cannot mount code as {id}: {why}; choose another id")
        };
        ErrorBody::new(ErrorCode::Conflict, message)
    };
    if builtins.iter().any(|name| name == id) {
        return Err(conflict(format!(
            "a folder .omfp/widgets/{id}/ would replace the built-in omfp:{id} in this workspace"
        )));
    }
    let folder = workspace_widgets(workspace).join(id);
    match std::fs::symlink_metadata(&folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(internal(format!("{}: {error}", folder.display()))),
        Ok(metadata) if !metadata.is_dir() => Err(conflict(format!(
            ".omfp/widgets/{id} exists and is not a folder"
        ))),
        Ok(_) if !folder.join(MOUNT_MARKER).is_file() => Err(conflict(format!(
            ".omfp/widgets/{id}/ holds a widget no mount wrote"
        ))),
        Ok(_) => match ENTRIES
            .into_iter()
            .find(|entry| *entry != "index.tsx" && folder.join(entry).exists())
        {
            Some(entry) => Err(conflict(format!(".omfp/widgets/{id}/ also has {entry}"))),
            None => Ok(true),
        },
    }
}

/// Writes `.omfp/widgets/<id>/index.tsx` of the workspace at `workspace`, with the mount's marker.
fn write_code(workspace: &std::path::Path, id: &str, code: &str) -> Result<(), ErrorBody> {
    let folder = workspace_widgets(workspace).join(id);
    let marker = folder.join(MOUNT_MARKER);
    replace_file(
        &marker,
        format!("Written by views.mount `{id}`: its code (index.tsx) is replaced by the next mount with code.\n")
            .as_bytes(),
        None,
    )
    .map_err(|error| internal(format!("{}: {error}", marker.display())))?;
    let file = folder.join("index.tsx");
    replace_file(&file, code.as_bytes(), None)
        .map_err(|error| internal(format!("{}: {error}", file.display())))
}

fn write_manifest(workspace: &std::path::Path, manifest: &Value) -> Result<(), ErrorBody> {
    let file = manifest_path(workspace);
    replace_file(&file, manifest::to_text(manifest).as_bytes(), None)
        .map_err(|error| internal(format!("{}: {error}", file.display())))
}

impl Services {
    /// `views.mount` (PRT-080, AGT-061).
    pub async fn mount(&self, params: MountParams) -> Result<Mounted, ErrorBody> {
        let id = mount_id(&params.id)?.to_owned();
        let reference = match (&params.widget, &params.code) {
            (Some(widget), None) => reference(widget)?,
            (None, Some(_)) => format!("./widgets/{id}"),
            _ => return Err(bad_request("give exactly one of `widget` and `code`")),
        };
        if params.panel && (params.view.is_some() || params.place.is_some()) {
            return Err(bad_request(
                "a panel floats over every view: `view` and `place` do not apply",
            ));
        }
        let _edit = self.manifest_edits.lock().await;
        let store = Arc::clone(&self.store);
        let builtins = self.widgets.builtin_names();
        let workspace_id = params.workspace_id.clone();
        let mounted = blocking(move || {
            let (path, current) = current(&store, &workspace_id)?;
            let code_replaced = match params.code {
                Some(_) => claim_folder(&path, &id, &builtins, false)?,
                None => false,
            };
            let view = params.view.as_deref().map(str::trim);
            let target = if params.panel {
                Target::Panel
            } else {
                Target::View {
                    view,
                    place: place(params.place.unwrap_or_default()),
                }
            };
            let placed = mounts::mount(
                current.as_ref(),
                &Mount {
                    id: &id,
                    reference: &reference,
                    props: params.props.unwrap_or_default(),
                    target,
                },
            )
            .map_err(mount_error)?;
            // The code first: a window importing the new reference finds it.
            if let Some(code) = &params.code {
                write_code(&path, &id, code)?;
            }
            write_manifest(&path, &placed.manifest)?;
            Ok(Mounted {
                id,
                reference: placed.reference,
                panel: placed.view.is_none(),
                view: placed.view,
                replaced: placed.replaced || code_replaced,
            })
        })
        .await?;
        self.refresh_views(&params.workspace_id).await;
        Ok(mounted)
    }

    /// `views.mount_update` (PRT-081, AGT-062).
    pub async fn mount_update(&self, params: MountUpdateParams) -> Result<Mounted, ErrorBody> {
        let id = mount_id(&params.id)?.to_owned();
        if params.props.is_none() && params.code.is_none() {
            return Err(bad_request("give `props`, `code` or both"));
        }
        let _edit = self.manifest_edits.lock().await;
        let store = Arc::clone(&self.store);
        let builtins = self.widgets.builtin_names();
        let workspace_id = params.workspace_id.clone();
        let mounted = blocking(move || {
            let (path, current) = current(&store, &workspace_id)?;
            let props = params.props.unwrap_or_default();
            let placed = mounts::update(current.as_ref(), &id, &props).map_err(mount_error)?;
            if let Some(code) = &params.code {
                let own = format!("./widgets/{id}");
                if placed.reference != own {
                    return Err(bad_request(format!(
                        "mount {id} shows {}, not inline code ({own}): mount it again with `code` instead",
                        placed.reference
                    )));
                }
                claim_folder(&path, &id, &builtins, true)?;
                write_code(&path, &id, code)?;
            }
            if current.as_ref() != Some(&placed.manifest) {
                write_manifest(&path, &placed.manifest)?;
            }
            Ok(Mounted {
                id,
                reference: placed.reference,
                panel: placed.view.is_none(),
                view: placed.view,
                replaced: true,
            })
        })
        .await?;
        self.refresh_views(&params.workspace_id).await;
        Ok(mounted)
    }

    /// `views.unmount` (PRT-082, AGT-063).
    pub async fn unmount(&self, params: UnmountParams) -> Result<Unmounted, ErrorBody> {
        let id = params.id.trim().to_owned();
        let _edit = self.manifest_edits.lock().await;
        let store = Arc::clone(&self.store);
        let workspace_id = params.workspace_id.clone();
        let unmounted = blocking(move || {
            let (path, current) = current(&store, &workspace_id)?;
            let removed = mounts::unmount(current.as_ref(), &id).map_err(mount_error)?;
            write_manifest(&path, &removed.manifest)?;
            Ok(Unmounted {
                views: removed.views,
                removed_views: removed.removed_views,
                panel: removed.panel,
            })
        })
        .await?;
        self.refresh_views(&params.workspace_id).await;
        Ok(unmounted)
    }

    /// `widgets.save` (PRT-083, AGT-064): then every followed views topic lists the component.
    pub async fn save_widget(&self, params: SaveWidgetParams) -> Result<WidgetSaved, ErrorBody> {
        let name = params.name.trim().to_owned();
        let (store, widgets) = (Arc::clone(&self.store), Arc::clone(&self.widgets));
        let saved = name.clone();
        blocking(move || {
            let library = widgets.user_library();
            match (&params.widget, &params.code) {
                (Some(widget), None) => {
                    let workspace_id = params
                        .workspace_id
                        .as_deref()
                        .ok_or_else(|| bad_request("`workspaceId` is required with `widget`"))?;
                    let folder_name = widget.trim();
                    let folder_name = folder_name.strip_prefix("./widgets/").unwrap_or(folder_name);
                    if !is_widget_name(folder_name) {
                        return Err(bad_request(format!(
                            "cannot save {widget:?}: expected a workspace widget, `./widgets/<name>` or a bare name"
                        )));
                    }
                    let folder = workspace_widgets(&store.workspace(workspace_id)?.path).join(folder_name);
                    library::save(library, &name, Source::Folder(&folder), params.replace)
                }
                (None, Some(code)) => library::save(library, &name, Source::Code(code), params.replace),
                _ => return Err(bad_request("give exactly one of `widget` and `code`")),
            }
            .map_err(copy_error)
        })
        .await?;
        let followed: Vec<String> = self
            .views_logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        for workspace_id in followed {
            self.refresh_views(&workspace_id).await;
        }
        Ok(WidgetSaved {
            reference: format!("user:{saved}"),
            name: saved,
        })
    }

    /// `widgets.library` (PRT-084, AGT-065).
    pub async fn widget_library(&self) -> Result<Vec<LibraryComponent>, ErrorBody> {
        let widgets = Arc::clone(&self.widgets);
        blocking(move || Ok(library::list(widgets.user_library()))).await
    }
}
