//! Serialized local library repository.
use std::collections::HashMap;

use chrono::Utc;

use crate::models::InstalledApp;
use crate::persistence;

pub type Library = HashMap<String, InstalledApp>;

pub fn load_all() -> Result<Library, String> {
    persistence::with_repository(
        &persistence::data_path("library.json")?,
        |repository: &mut persistence::Repository<Library>| {
            crate::lifecycle::recovery::recover(repository)?;
            Ok(repository.state.clone())
        },
    )
}

pub fn mutate_library<R>(
    operation: impl FnOnce(&mut Library) -> Result<R, String>,
) -> Result<R, String> {
    persistence::with_repository(
        &persistence::data_path("library.json")?,
        |repository: &mut persistence::Repository<Library>| {
            crate::lifecycle::recovery::recover(repository)?;
            let result = operation(&mut repository.state)?;
            repository.commit().map_err(|error| error.message)?;
            Ok(result)
        },
    )
}

pub fn touch_launch(id: &str) -> Result<InstalledApp, String> {
    mutate_library(|library| {
        let app = library.get_mut(id).ok_or("app not found")?;
        app.launch_count = app
            .launch_count
            .checked_add(1)
            .ok_or("launch count overflow")?;
        app.last_launched = Some(Utc::now());
        Ok(app.clone())
    })
}
