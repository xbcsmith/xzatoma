use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use xzatoma::storage::SqliteStorage;

#[allow(dead_code)]
pub fn create_temp_storage() -> (SqliteStorage, TempDir) {
    let tmp = TempDir::new().expect("failed to create tempdir");
    let db_path = tmp.path().join("history.db");
    let storage =
        SqliteStorage::new_with_path(db_path).expect("failed to create sqlite storage with path");
    (storage, tmp)
}

#[allow(dead_code)]
pub fn temp_config_file(contents: &str) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(&config_path, contents).expect("failed to write config file");
    (temp_dir, config_path)
}
