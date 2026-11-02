//! Local imports detect format before any managed-storage operation.
use super::*;
use crate::persistence::Repository;

impl Lifecycle {
    pub fn import_app(&self, source: &Path, move_to_managed: bool) -> Result<InstalledApp, String> {
        let source = candidate_path(source)?;
        let kind = app_file::detect(&source)?;
        if kind == AppKind::Binary && move_to_managed {
            return Err("Executables are registered at their current location and cannot be moved to managed storage".into());
        }
        if move_to_managed && !is_managed_path(&self.managed_dir, &source)? {
            return self.integrate(&source, true);
        }
        persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                recovery::recover(repository)?;
                let file = open_app(&source, kind)?;
                let identity = app_discovery::read_identity_for_kind(&source, kind)?;
                if kind == AppKind::AppImage && self_appimage::is_self_appimage(&source, &identity)
                {
                    return Err(self_appimage::IMPORT_WARNING.into());
                }
                let id = path_id(&source)?;
                if let Some(existing) = repository.state.values().find(|app| {
                    app.source_path.as_deref() == Some(id.as_str()) && app.file_path != id
                }) {
                    if existing.kind != kind {
                        return Err("App format differs from the existing entry; remove the old entry before adding it again".into());
                    }
                    return Ok(existing.clone());
                }
                let existing = repository
                    .state
                    .get(&id)
                    .or_else(|| repository.state.values().find(|app| app.file_path == id));
                let app = InstalledApp {
                    integrated: kind == AppKind::AppImage
                        && (existing.is_some_and(|app| app.integrated)
                            || is_managed_path(&self.managed_dir, &source)?),
                    ..discovered_app(&source, &file, existing, identity, kind)?
                };
                app.validate_kind()?;
                repository.state.insert(app.id.clone(), app.clone());
                repository.commit().map_err(|error| error.message)?;
                Ok(app)
            },
        )
    }
}
