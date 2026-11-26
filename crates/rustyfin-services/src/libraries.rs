//! Library management: the rules for making, renaming, re-foldering and
//! deleting libraries, and the activity log entries each change leaves.
//!
//! A library a linked server made belongs to that link: it is set in
//! `rustyfin.toml` (`[[link]]`) and the next sync would make it again, so
//! it is never renamed, deleted or given folders here.
//!
//! Libraries are set in `rustyfin.toml` too (`[[library]]`; owner,
//! 2026-11-26: "completely config file driven"): [`Libraries::apply_config`]
//! makes or updates them at start. Once any library is set there, no
//! library is made, changed or deleted here; one left in the database but
//! not in the file is kept (with its watched history) until
//! `rustyfin prune` removes it ([`Libraries::prune`]).

use rustyfin_models::data::{
    ActivityLogStore, Data, Library, LibraryAccessStore, LibraryStore, LinkStore,
};
use uuid::Uuid;

use crate::error::{ServiceError, ServiceResult};

/// Library kinds the scanner can index.
pub const SUPPORTED_KINDS: [&str; 3] = ["movies", "tvshows", "music"];

/// A library name taken from a request: trimmed, and not blank.
pub fn require_name(raw: &str) -> ServiceResult<&str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(ServiceError::Invalid("name must not be blank".into()));
    }
    Ok(name)
}

/// A name a library may be given: not blank, and nothing that could leave
/// the libraries root were it used as a directory (Jellyfin's rule).
pub fn require_new_name(raw: &str) -> ServiceResult<&str> {
    let name = require_name(raw)?;
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ServiceError::Invalid(
            "name must not contain path separators".into(),
        ));
    }
    Ok(name)
}

/// A checked new library.
#[derive(Debug, Clone, Default)]
pub struct NewLibrary {
    pub name: String,
    /// One of [`SUPPORTED_KINDS`], in any case.
    pub kind: String,
    /// Trimmed, blanks and repeats dropped; each must be a folder.
    pub paths: Vec<String>,
    /// The client's library options, stored as given.
    pub options: serde_json::Value,
}

/// One `[[library]]` in `rustyfin.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ConfiguredLibrary {
    pub name: String,
    /// One of [`SUPPORTED_KINDS`].
    #[serde(rename = "type")]
    pub kind: String,
    /// Its folders.
    pub paths: Vec<String>,
}

/// What [`Libraries::apply_config`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfigReport {
    /// Libraries made.
    pub created: Vec<String>,
    /// Libraries whose name case, kind or folders changed.
    pub updated: Vec<String>,
    /// `[[library]]` entries that could not be applied, and why.
    pub refused: Vec<(String, String)>,
    /// Libraries in the database that are not in the file (links' aside).
    pub unconfigured: Vec<String>,
}

/// Library management over a data layer.
pub struct Libraries<'a, D> {
    pub(crate) data: &'a D,
}

impl<'a, D> Libraries<'a, D> {
    /// The rules over `data`, before the rest of the services exist (the
    /// config file's libraries are applied first thing at start).
    pub fn over(data: &'a D) -> Self {
        Libraries { data }
    }
}

impl<D: Data> Libraries<'_, D> {
    /// The libraries `user` may see.
    pub async fn accessible(&self, user: Uuid) -> ServiceResult<Vec<Library>> {
        let all = self.data.libraries().list_all().await?;
        let allowed = self
            .data
            .user_library_access()
            .get_accessible_library_ids(user)
            .await?;
        Ok(all
            .into_iter()
            .filter(|lib| allowed.as_ref().is_none_or(|ids| ids.contains(&lib.id)))
            .collect())
    }

    /// The libraries `user` is shown: those they may see, less those made
    /// for a linked server that is down; for a server linking this one
    /// (`for_linking`), less every library made for a link.
    pub async fn shown(&self, user: Uuid, for_linking: bool) -> ServiceResult<Vec<Library>> {
        let offline = self.data.links().offline_ids().await?;
        Ok(self
            .accessible(user)
            .await?
            .into_iter()
            .filter(|lib| lib.link_id.is_none_or(|l| !offline.contains(&l)))
            .filter(|lib| !for_linking || lib.link_id.is_none())
            .collect())
    }

    /// Make or update the libraries set in the config file, matched to the
    /// database's by name (in any case, so a library keeps its id and
    /// everything attached to it), and mark them as the file's. Folders
    /// that do not exist yet are kept: a share may be mounted later.
    pub async fn apply_config(&self, wanted: &[ConfiguredLibrary]) -> ServiceResult<ConfigReport> {
        let store = self.data.libraries();
        let mut report = ConfigReport::default();
        let mut ids = Vec::new();
        for entry in wanted {
            let name = match require_new_name(&entry.name) {
                Ok(n) => n.to_owned(),
                Err(e) => {
                    report.refused.push((entry.name.clone(), e.to_string()));
                    continue;
                }
            };
            let kind = entry.kind.trim().to_ascii_lowercase();
            if !SUPPORTED_KINDS.contains(&kind.as_str()) {
                report.refused.push((
                    name,
                    format!("type must be one of: {}", SUPPORTED_KINDS.join(", ")),
                ));
                continue;
            }
            let paths = clean_paths(&entry.paths);
            if paths.is_empty() {
                report
                    .refused
                    .push((name, "at least one path is required".into()));
                continue;
            }
            if ids
                .iter()
                .any(|(_, n): &(Uuid, String)| n.eq_ignore_ascii_case(&name))
            {
                report
                    .refused
                    .push((name, "set twice in rustyfin.toml".into()));
                continue;
            }
            let id = match store.find_by_name(&name).await? {
                Some(lib) if lib.link_id.is_some() => {
                    report
                        .refused
                        .push((name, "a linked server's library has this name".into()));
                    continue;
                }
                Some(lib) => {
                    let mut changed = false;
                    if lib.name != name {
                        store.rename(lib.id, &name).await?;
                        changed = true;
                    }
                    if lib.library_type != kind {
                        store.set_type(lib.id, &kind).await?;
                        changed = true;
                    }
                    if !same_folders(&lib.paths, &paths) {
                        store.set_paths(lib.id, &paths).await?;
                        changed = true;
                    }
                    if changed {
                        self.log("Library updated", &name, "LibraryUpdated").await;
                        report.updated.push(name.clone());
                    }
                    lib.id
                }
                None => {
                    let lib = store.create(&name, &kind, &paths).await?;
                    self.log("Library created", &name, "LibraryCreated").await;
                    report.created.push(name.clone());
                    lib.id
                }
            };
            ids.push((id, name));
        }
        let ids: Vec<Uuid> = ids.into_iter().map(|(id, _)| id).collect();
        store.mark_configured(&ids).await?;
        report.unconfigured = store
            .list_all()
            .await?
            .into_iter()
            .filter(|lib| lib.link_id.is_none() && !ids.contains(&lib.id))
            .map(|lib| lib.name)
            .collect();
        Ok(report)
    }

    /// The libraries the config file does not set (a linked server's
    /// aside), matched by name in any case as [`Self::apply_config`] does;
    /// deleted, with everything in them, when `apply`. Refused when the file
    /// sets no library at all: a missing or empty file would empty the
    /// server.
    pub async fn prune(
        &self,
        wanted: &[ConfiguredLibrary],
        apply: bool,
    ) -> ServiceResult<Vec<Library>> {
        if wanted.is_empty() {
            return Err(ServiceError::Invalid(
                "the config sets no library ([[library]]): nothing would be left".into(),
            ));
        }
        let store = self.data.libraries();
        let stray: Vec<Library> = store
            .list_all()
            .await?
            .into_iter()
            .filter(|lib| lib.link_id.is_none())
            .filter(|lib| {
                !wanted
                    .iter()
                    .any(|w| w.name.trim().eq_ignore_ascii_case(&lib.name))
            })
            .collect();
        if apply {
            for lib in &stray {
                store.delete(lib.id).await?;
                self.log("Library deleted", &lib.name, "LibraryDeleted")
                    .await;
            }
        }
        Ok(stray)
    }

    /// Libraries come from the config file once any is set there.
    async fn refuse_if_config_driven(&self) -> ServiceResult<()> {
        if self
            .data
            .libraries()
            .list_all()
            .await?
            .iter()
            .any(|lib| lib.configured)
        {
            return Err(ServiceError::Conflict(
                "libraries are set in rustyfin.toml ([[library]])".into(),
            ));
        }
        Ok(())
    }

    /// A library that may be changed here: not a link's, and none while the
    /// config file sets libraries.
    async fn refuse_change(&self, lib: &Library) -> ServiceResult<()> {
        refuse_if_linked(lib)?;
        if lib.configured {
            return Err(ServiceError::Conflict(format!(
                "'{}' is set in rustyfin.toml ([[library]]); change it there",
                lib.name
            )));
        }
        self.refuse_if_config_driven().await
    }

    /// Make a library after checking its name, kind and folders.
    pub async fn create(&self, new: NewLibrary) -> ServiceResult<Library> {
        self.refuse_if_config_driven().await?;
        let name = require_new_name(&new.name)?.to_owned();
        let kind = new.kind.to_ascii_lowercase();
        if !SUPPORTED_KINDS.contains(&kind.as_str()) {
            return Err(ServiceError::Invalid(format!(
                "collectionType must be one of: {}",
                SUPPORTED_KINDS.join(", ")
            )));
        }
        let paths = clean_paths(&new.paths);
        if paths.iter().any(|p| !std::path::Path::new(p).is_dir()) {
            return Err(ServiceError::NotFound);
        }
        let store = self.data.libraries();
        if store.find_by_name(&name).await?.is_some() {
            return Err(ServiceError::Conflict(format!(
                "a library named '{name}' already exists"
            )));
        }
        let lib = store
            .create_with_options(&name, &kind, &paths, &new.options)
            .await?;
        self.log("Library created", &name, "LibraryCreated").await;
        Ok(lib)
    }

    /// Make a library as given, without the checks of [`Self::create`]: the
    /// admin's own form, which checks its fields itself.
    pub async fn create_as_given(
        &self,
        name: &str,
        kind: &str,
        paths: &[String],
    ) -> ServiceResult<Library> {
        self.refuse_if_config_driven().await?;
        let lib = self.data.libraries().create(name, kind, paths).await?;
        self.log("Library created", name, "LibraryCreated").await;
        Ok(lib)
    }

    /// Rename a library and replace its folders. Returns the library and
    /// whether its folders changed (new folders mean a scan is due).
    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        paths: &[String],
    ) -> ServiceResult<(Library, bool)> {
        if paths.is_empty() {
            return Err(ServiceError::Invalid(
                "At least one path is required".into(),
            ));
        }
        let store = self.data.libraries();
        let before = store.find_by_id(id).await?.ok_or(ServiceError::NotFound)?;
        self.refuse_change(&before).await?;
        let lib = store
            .update(id, name, paths)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.log("Library updated", name, "LibraryUpdated").await;
        let moved = before.paths != lib.paths;
        Ok((lib, moved))
    }

    /// Delete the library with this name.
    pub async fn delete_named(&self, name: &str) -> ServiceResult<()> {
        let lib = self.named(require_name(name)?).await?;
        self.delete(lib.id, &lib.name).await
    }

    /// Delete the library with this id.
    pub async fn delete_by_id(&self, id: Uuid) -> ServiceResult<()> {
        self.delete(id, "").await
    }

    async fn delete(&self, id: Uuid, name: &str) -> ServiceResult<()> {
        let store = self.data.libraries();
        if let Some(lib) = store.find_by_id(id).await? {
            self.refuse_change(&lib).await?;
        }
        if !store.delete(id).await? {
            return Err(ServiceError::NotFound);
        }
        self.log("Library deleted", name, "LibraryDeleted").await;
        Ok(())
    }

    /// Rename the library called `name`.
    pub async fn rename(&self, name: &str, new_name: &str) -> ServiceResult<()> {
        let name = require_name(name)?;
        let new_name = require_new_name(new_name)?;
        let lib = self.named(name).await?;
        self.refuse_change(&lib).await?;
        let store = self.data.libraries();
        if let Some(other) = store.find_by_name(new_name).await?
            && other.id != lib.id
        {
            return Err(ServiceError::Conflict(format!(
                "a library named '{new_name}' already exists"
            )));
        }
        store
            .rename(lib.id, new_name)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.log("Library renamed", new_name, "LibraryUpdated")
            .await;
        Ok(())
    }

    /// Add a folder to the library called `name`.
    pub async fn add_path(&self, name: &str, path: &str) -> ServiceResult<()> {
        let name = require_name(name)?;
        let path = path.trim();
        if path.is_empty() {
            return Err(ServiceError::Invalid("path must not be blank".into()));
        }
        let lib = self.named(name).await?;
        self.refuse_change(&lib).await?;
        if !std::path::Path::new(path).is_dir() {
            return Err(ServiceError::NotFound);
        }
        if lib.paths.iter().any(|p| p == path) {
            return Err(ServiceError::Conflict(format!(
                "'{path}' is already part of '{}'",
                lib.name
            )));
        }
        let mut paths = lib.paths.clone();
        paths.push(path.to_owned());
        self.data
            .libraries()
            .set_paths(lib.id, &paths)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.log("Library path added", &lib.name, "LibraryUpdated")
            .await;
        Ok(())
    }

    /// Succeeds when `path` is one of the folders of the library `name`.
    pub async fn has_path(&self, name: &str, path: &str) -> ServiceResult<()> {
        let lib = self.named(require_name(name)?).await?;
        if lib.paths.iter().any(|p| p == path) {
            Ok(())
        } else {
            Err(ServiceError::NotFound)
        }
    }

    /// Remove a folder from the library called `name`; its last folder stays.
    pub async fn remove_path(&self, name: &str, path: &str) -> ServiceResult<()> {
        let lib = self.named(require_name(name)?).await?;
        if !lib.paths.iter().any(|p| p == path) {
            return Err(ServiceError::NotFound);
        }
        self.refuse_change(&lib).await?;
        if lib.paths.len() == 1 {
            return Err(ServiceError::Invalid(
                "a library needs at least one path".into(),
            ));
        }
        let remaining: Vec<String> = lib.paths.iter().filter(|p| *p != path).cloned().collect();
        self.data
            .libraries()
            .set_paths(lib.id, &remaining)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.log("Library path removed", &lib.name, "LibraryUpdated")
            .await;
        Ok(())
    }

    /// Replace a library's options.
    pub async fn set_options(&self, id: Uuid, options: &serde_json::Value) -> ServiceResult<()> {
        let before = self
            .data
            .libraries()
            .find_by_id(id)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.refuse_change(&before).await?;
        let lib = self
            .data
            .libraries()
            .set_options(id, options)
            .await?
            .ok_or(ServiceError::NotFound)?;
        self.log("Library options updated", &lib.name, "LibraryUpdated")
            .await;
        Ok(())
    }

    /// Set or clear a library's own scan schedule.
    pub async fn set_scan_schedule(&self, id: Uuid, schedule: Option<&str>) -> ServiceResult<()> {
        let store = self.data.libraries();
        let lib = store.find_by_id(id).await?.ok_or(ServiceError::NotFound)?;
        self.refuse_change(&lib).await?;
        store
            .update_scan_schedule(id, schedule)
            .await?
            .ok_or(ServiceError::NotFound)?;
        Ok(())
    }

    async fn named(&self, name: &str) -> ServiceResult<Library> {
        self.data
            .libraries()
            .find_by_name(name)
            .await?
            .ok_or(ServiceError::NotFound)
    }

    /// An activity log entry; a failure to write one never fails the change.
    async fn log(&self, message: &str, name: &str, kind: &str) {
        let _ = self
            .data
            .activity_log()
            .insert(message, Some(name), kind, "Informational", None, None)
            .await;
    }
}

/// Folders trimmed, blanks and repeats dropped, in the order given.
fn clean_paths(raw: &[String]) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for path in raw.iter().map(|p| p.trim()) {
        if !path.is_empty() && !paths.iter().any(|p| p == path) {
            paths.push(path.to_owned());
        }
    }
    paths
}

/// The same folders, in any order.
fn same_folders(a: &[String], b: &[String]) -> bool {
    let mut a: Vec<&String> = a.iter().collect();
    let mut b: Vec<&String> = b.iter().collect();
    a.sort();
    b.sort();
    a == b
}

/// A library a link made is the link's (see the module docs).
fn refuse_if_linked(lib: &Library) -> ServiceResult<()> {
    if lib.link_id.is_some() {
        return Err(ServiceError::Conflict(format!(
            "'{}' is filled by a linked server; change it in rustyfin.toml ([[link]])",
            lib.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_may_not_escape_the_libraries_root() {
        assert_eq!(require_name("  Films ").unwrap(), "Films");
        assert!(require_name("   ").is_err());
        for bad in [".", "..", "a/b", "a\\b"] {
            assert!(require_new_name(bad).is_err(), "{bad}");
        }
        assert_eq!(require_new_name(" Films ").unwrap(), "Films");
    }
}
