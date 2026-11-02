//! Prepared approvals bind a settings confirmation to backend-verified executable bytes.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use crate::lifecycle::{app_digest, Lifecycle};
use crate::models::InstalledApp;
use crate::persistence::{self, Repository};
use crate::store::Library;

const MAX_PREPARED_REVIEWS: usize = 64;

#[derive(Clone)]
pub(crate) struct PreparedReview {
    app: InstalledApp,
    digest: String,
}

impl PreparedReview {
    pub(crate) fn digest_for(&self, app: &InstalledApp) -> Result<String, String> {
        if self.app.id != app.id
            || self.app.file_path != app.file_path
            || self.app.kind != app.kind
            || self.app.reviewed_digest != app.reviewed_digest
            || self.app.launch_config != app.launch_config
        {
            return Err("App approval changed. Close settings and review this app again.".into());
        }
        Ok(self.digest.clone())
    }
}

impl Lifecycle {
    pub(crate) fn prepare_review(&self, id: &str) -> Result<PreparedReview, String> {
        let app = persistence::with_repository(
            &self.library_path,
            |repository: &mut Repository<Library>| {
                crate::lifecycle::recovery::recover(repository)?;
                repository
                    .state
                    .get(id)
                    .cloned()
                    .ok_or("app not found".into())
            },
        )?;
        // Reading the executable must not hold the shared settings/library lock.
        let digest = app_digest(std::path::Path::new(&app.file_path), app.kind)?;
        Ok(PreparedReview { app, digest })
    }
}

#[derive(Default)]
pub(crate) struct PreparedReviews {
    sequence: AtomicU64,
    reviews: Mutex<BTreeMap<u64, PreparedReview>>,
}

impl PreparedReviews {
    pub(crate) fn insert(&self, review: PreparedReview) -> Result<String, String> {
        let token = self.sequence.fetch_add(1, Ordering::Relaxed);
        let mut reviews = self.reviews.lock().map_err(|error| error.to_string())?;
        reviews.insert(token, review);
        while reviews.len() > MAX_PREPARED_REVIEWS {
            reviews.pop_first();
        }
        Ok(token.to_string())
    }

    pub(crate) fn get(&self, token: &str) -> Result<PreparedReview, String> {
        let token = token.parse::<u64>().map_err(|_| "Invalid app approval")?;
        self.reviews
            .lock()
            .map_err(|error| error.to_string())?
            .get(&token)
            .cloned()
            .ok_or_else(|| {
                "App verification expired. Close settings and review this app again.".into()
            })
    }

    pub(crate) fn remove(&self, token: &str) -> Result<(), String> {
        let token = token.parse::<u64>().map_err(|_| "Invalid app approval")?;
        self.reviews
            .lock()
            .map_err(|error| error.to_string())?
            .remove(&token);
        Ok(())
    }
}

pub(crate) static PREPARED_REVIEWS: LazyLock<PreparedReviews> =
    LazyLock::new(PreparedReviews::default);
