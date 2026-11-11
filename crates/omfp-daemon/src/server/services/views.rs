//! The `views/<workspaceId>` topics (DMN-082, PRT-040): each workspace's views manifest, keymap (DMN-121), widget
//! folders and widget state, refreshed on every rescan and poll round, plus the methods that change them (DMN-083,
//! DMN-086, DMN-087).

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use omfp_protocol::{
    CopyWidgetParams, ErrorBody, ErrorCode, SetViewsParams, SetWidgetStateParams, ViewsEvent,
    ViewsState, WidgetCopied, WorkspaceBinding,
};
use tracing::warn;

use super::{LIVE_BUFFER, Services, blocking, write_atomically};
use crate::event_log::EventLog;
use crate::server::convert::{bad_request, internal};
use crate::store::{Store, new_id};
use crate::widgets::{CopyError, Widgets, keymap, manifest, manifest_path};

/// A `views/<workspaceId>` topic (PRT-040).
pub(crate) type ViewsLog = EventLog<ViewsEvent, ViewsState>;

const VIEWS_CAPACITY: usize = 64;

/// Largest JSON value a widget may store (DMN-086).
pub(crate) const MAX_STORED_BYTES: usize = 256 * 1024;

#[allow(clippy::trivially_copy_pass_by_ref)] // `EventLog` takes `fn(&mut S, &E)`.
fn apply_views(state: &mut ViewsState, event: &ViewsEvent) {
    match event {
        ViewsEvent::ViewsManifest { manifest, error } => {
            state.manifest.clone_from(manifest);
            state.error.clone_from(error);
        }
        ViewsEvent::ViewsKeymap { keymap, error } => {
            state.keymap.clone_from(keymap);
            state.keymap_error.clone_from(error);
        }
        ViewsEvent::ViewsWidgets { widgets } => state.widgets.clone_from(widgets),
        ViewsEvent::WidgetStored {
            instance,
            key,
            value,
        } => {
            if value.is_null() {
                if let Some(keys) = state.stored.get_mut(instance) {
                    keys.remove(key);
                    if keys.is_empty() {
                        state.stored.remove(instance);
                    }
                }
            } else {
                state
                    .stored
                    .entry(instance.clone())
                    .or_default()
                    .insert(key.clone(), value.clone());
            }
        }
    }
}

/// The workspace's folder; `not_found` for an unknown workspace.
fn workspace_path(store: &Store, id: &str) -> Result<PathBuf, ErrorBody> {
    Ok(store.workspace(id)?.path)
}

/// The keymap the topic carries for `read` (DMN-121): the file's bindings (none without a file), or `last` with the
/// error of an invalid file.
fn workspace_keymap(
    read: Result<Option<Vec<WorkspaceBinding>>, String>,
    last: &[WorkspaceBinding],
) -> (Vec<WorkspaceBinding>, Option<String>) {
    match read {
        Ok(bindings) => (bindings.unwrap_or_default(), None),
        Err(error) => (last.to_vec(), Some(error)),
    }
}

fn copy_error(error: CopyError) -> ErrorBody {
    let (code, message) = match error {
        CopyError::BadRequest(message) => (ErrorCode::BadRequest, message),
        CopyError::NotFound(message) => (ErrorCode::NotFound, message),
        CopyError::Conflict(message) => (ErrorCode::Conflict, message),
        CopyError::Io(message) => (ErrorCode::Internal, message),
    };
    ErrorBody::new(code, message)
}

impl Services {
    /// The log of `views/<id>` with its state read now (DMN-082); `not_found` for an unknown workspace.
    pub async fn views_log(&self, id: &str) -> Result<Arc<ViewsLog>, ErrorBody> {
        let _serial = self.views_refresh.lock().await;
        if let Some(log) = self.views_log_entry(id) {
            self.refresh_views_locked(id, &log).await?;
            return Ok(log);
        }
        let (store, widgets, workspace_id) = (
            Arc::clone(&self.store),
            Arc::clone(&self.widgets),
            id.to_owned(),
        );
        let state = blocking(move || {
            let path = workspace_path(&store, &workspace_id)?;
            let (manifest, error) = Widgets::manifest(&path);
            let (keymap, keymap_error) = workspace_keymap(keymap::read(&keymap::path(&path)), &[]);
            Ok(ViewsState {
                manifest,
                error,
                widgets: widgets.widgets(&path),
                stored: store.widget_state(&workspace_id)?,
                keymap,
                keymap_error,
            })
        })
        .await?;
        let log = Arc::new(EventLog::new(
            new_id(),
            state,
            apply_views,
            VIEWS_CAPACITY,
            LIVE_BUFFER,
        ));
        self.views_logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned(), Arc::clone(&log));
        Ok(log)
    }

    fn views_log_entry(&self, id: &str) -> Option<Arc<ViewsLog>> {
        self.views_logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    /// The position of `views/<id>` when a window follows it (DMN-063).
    pub(crate) fn views_log_position(&self, id: &str) -> Option<crate::event_log::Position> {
        self.views_log_entry(id).map(|log| log.position())
    }

    /// Reads the workspace's manifest, keymap and widget folders again and publishes what changed on `views/<id>`, if
    /// the topic exists (DMN-082, DMN-121). A removed workspace's topic goes away.
    pub async fn refresh_views(&self, id: &str) {
        let _serial = self.views_refresh.lock().await;
        let Some(log) = self.views_log_entry(id) else {
            return;
        };
        match self.refresh_views_locked(id, &log).await {
            Ok(()) => {}
            Err(error) if error.code == ErrorCode::NotFound => {
                self.views_logs
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(id);
            }
            Err(error) => warn!(workspace = %id, error = %error.message, "cannot read the views"),
        }
    }

    /// The refresh itself; the caller holds `views_refresh`, so events of one topic never interleave.
    async fn refresh_views_locked(&self, id: &str, log: &ViewsLog) -> Result<(), ErrorBody> {
        let (store, widgets, workspace_id) = (
            Arc::clone(&self.store),
            Arc::clone(&self.widgets),
            id.to_owned(),
        );
        let (manifest, error, keymap_read, widgets) = blocking(move || {
            let path = workspace_path(&store, &workspace_id)?;
            let (manifest, error) = Widgets::manifest(&path);
            let keymap_read = keymap::read(&keymap::path(&path));
            Ok((manifest, error, keymap_read, widgets.widgets(&path)))
        })
        .await?;
        let (keymap, keymap_error) =
            log.with_state(|state| workspace_keymap(keymap_read, &state.keymap));
        let (manifest_changed, keymap_changed, widgets_changed) = log.with_state(|state| {
            (
                state.manifest != manifest || state.error != error,
                state.keymap != keymap || state.keymap_error != keymap_error,
                state.widgets != widgets,
            )
        });
        if manifest_changed {
            log.push(ViewsEvent::ViewsManifest { manifest, error });
        }
        if keymap_changed {
            log.push(ViewsEvent::ViewsKeymap {
                keymap,
                error: keymap_error,
            });
        }
        if widgets_changed {
            log.push(ViewsEvent::ViewsWidgets { widgets });
        }
        Ok(())
    }

    /// `views.set` (DMN-083): checks the manifest, writes `.omfp/views.json` with sorted keys, then publishes it.
    pub async fn set_views(&self, params: SetViewsParams) -> Result<(), ErrorBody> {
        manifest::check(&params.manifest).map_err(bad_request)?;
        let store = Arc::clone(&self.store);
        let id = params.workspace_id.clone();
        blocking(move || {
            let path = manifest_path(&workspace_path(&store, &id)?);
            write_atomically(&path, &manifest::to_text(&params.manifest))
                .map_err(|error| internal(format!("{}: {error}", path.display())))
        })
        .await?;
        self.refresh_views(&params.workspace_id).await;
        Ok(())
    }

    /// `widgets.copy` (DMN-087): copies a built-in or user widget into the workspace, then publishes its folders.
    pub async fn copy_widget(&self, params: CopyWidgetParams) -> Result<WidgetCopied, ErrorBody> {
        let (store, widgets) = (Arc::clone(&self.store), Arc::clone(&self.widgets));
        let id = params.workspace_id.clone();
        let name = blocking(move || {
            let path = workspace_path(&store, &id)?;
            widgets
                .copy(&path, &params.widget, params.name.as_deref())
                .map_err(copy_error)
        })
        .await?;
        self.refresh_views(&params.workspace_id).await;
        Ok(WidgetCopied { name })
    }

    /// `widgets.state_set` (DMN-086): stores one key of a widget instance (`null` deletes it) and publishes
    /// `widget_stored`.
    pub async fn set_widget_state(&self, params: SetWidgetStateParams) -> Result<(), ErrorBody> {
        let size = params.value.to_string().len();
        if size > MAX_STORED_BYTES {
            return Err(bad_request(format!(
                "a stored value is at most {MAX_STORED_BYTES} bytes of JSON; this one has {size}"
            )));
        }
        // Held while storing, so a topic created meanwhile reads the value or gets the event, never neither.
        let _serial = self.views_refresh.lock().await;
        let store = Arc::clone(&self.store);
        let SetWidgetStateParams {
            workspace_id,
            instance,
            key,
            value,
        } = params;
        let stored = (
            workspace_id.clone(),
            instance.clone(),
            key.clone(),
            value.clone(),
        );
        blocking(move || {
            let (workspace_id, instance, key, value) = stored;
            store.workspace(&workspace_id)?;
            store
                .set_widget_state(&workspace_id, &instance, &key, &value)
                .map_err(ErrorBody::from)
        })
        .await?;
        if let Some(log) = self.views_log_entry(&workspace_id) {
            log.push(ViewsEvent::WidgetStored {
                instance,
                key,
                value,
            });
        }
        Ok(())
    }
}
