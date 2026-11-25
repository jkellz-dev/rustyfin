//! Library management: the rules for making, renaming, re-foldering and
//! deleting libraries, and the activity log entries each change leaves.
//!
//! A library a linked server made belongs to that link: it is set in
//! `rustyfin.toml` (`[[link]]`) and the next sync would make it again, so
//! it is never renamed, deleted or given folders here.

use rustyfin_models::data::{ActivityLogStore, Data, Library, LibraryStore};
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

/// Library management over a data layer.
pub struct Libraries<'a, D> {
    pub(crate) data: &'a D,
}

impl<D: Data> Libraries<'_, D> {
    /// Make a library after checking its name, kind and folders.
    pub async fn create(&self, new: NewLibrary) -> ServiceResult<Library> {
        let name = require_new_name(&new.name)?.to_owned();
        let kind = new.kind.to_ascii_lowercase();
        if !SUPPORTED_KINDS.contains(&kind.as_str()) {
            return Err(ServiceError::Invalid(format!(
                "collectionType must be one of: {}",
                SUPPORTED_KINDS.join(", ")
            )));
        }
        let mut paths: Vec<String> = Vec::new();
        for path in new.paths.iter().map(|p| p.trim()) {
            if !path.is_empty() && !paths.iter().any(|p| p == path) {
                paths.push(path.to_owned());
            }
        }
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
        refuse_if_linked(&before)?;
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
            refuse_if_linked(&lib)?;
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
        refuse_if_linked(&lib)?;
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
        refuse_if_linked(&lib)?;
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
