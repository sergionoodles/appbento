//! Keep AppBento itself out of the library, including renamed running images.
use std::path::{Path, PathBuf};

use crate::app_identity::AppIdentity;

pub(super) const IMPORT_WARNING: &str = "AppBento cannot add itself to its library. You are already running AppBento; adding it would only create another copy of this app inside itself. Add other AppImages instead.";
const APP_NAME: &str = "AppBento";

pub(super) fn is_self_appimage(path: &Path, identity: &AppIdentity) -> bool {
    let running_path = std::env::var_os("APPIMAGE").map(PathBuf::from);
    matches_self(path, identity, running_path.as_deref())
}

fn matches_self(path: &Path, identity: &AppIdentity, running_path: Option<&Path>) -> bool {
    if identity.name.eq_ignore_ascii_case(APP_NAME) {
        return true;
    }

    running_path
        .and_then(|running| std::fs::canonicalize(running).ok())
        .zip(std::fs::canonicalize(path).ok())
        .is_some_and(|(running, candidate)| running == candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::tests::TemporaryDirectory;

    #[test]
    fn recognizes_embedded_identity_even_when_the_file_is_renamed() {
        let identity = AppIdentity {
            name: "AppBento".into(),
            version: None,
        };
        assert!(matches_self(Path::new("Renamed.AppImage"), &identity, None));
    }

    #[test]
    fn recognizes_the_running_path_when_metadata_is_unavailable() {
        let directory = TemporaryDirectory::new();
        let path = directory.0.join("Renamed.AppImage");
        std::fs::write(&path, b"appimage").unwrap();
        let identity = AppIdentity {
            name: "Renamed".into(),
            version: None,
        };
        assert!(matches_self(
            &path,
            &identity,
            Some(&directory.0.join("./Renamed.AppImage"))
        ));
    }

    #[test]
    fn keeps_other_apps_with_similar_names() {
        let identity = AppIdentity {
            name: "AppBento Helper".into(),
            version: None,
        };
        assert!(!matches_self(
            Path::new("AppBento-Helper.AppImage"),
            &identity,
            None
        ));
    }
}
