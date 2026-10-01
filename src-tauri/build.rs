use std::path::PathBuf;

const COMMANDS: &[&str] = &[
    "library_list",
    "library_scan",
    "library_integrate",
    "library_launch",
    "library_remove",
    "catalog_list",
    "catalog_get",
    "updates_check",
    "updates_check_all",
    "settings_get",
    "settings_set",
];

fn main() {
    let catalog_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../catalog/apps");
    println!("cargo:rerun-if-changed={}", catalog_dir.display());
    let mut files: Vec<_> = std::fs::read_dir(catalog_dir)
        .expect("catalog directory missing")
        .map(|entry| entry.expect("cannot read catalog entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    files.sort();
    let mut source = String::from("const BUNDLED_CATALOG: &[(&str, &str)] = &[\n");
    for path in files {
        source.push_str(&format!(
            "({:?}, include_str!({:?})),\n",
            path.file_name().unwrap().to_str().unwrap(),
            path
        ));
    }
    source.push_str("];\n");
    let destination = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("catalog.rs");
    std::fs::write(destination, source).expect("cannot embed catalog");
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
