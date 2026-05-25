#![allow(dead_code)] // Shared integration helpers; each test crate uses a subset.

use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use xzatoma::storage::SqliteStorage;

pub fn create_temp_storage() -> (SqliteStorage, TempDir) {
    let tmp = TempDir::new().expect("failed to create tempdir");
    let db_path = tmp.path().join("history.db");
    let storage =
        SqliteStorage::new_with_path(db_path).expect("failed to create sqlite storage with path");
    (storage, tmp)
}

pub fn temp_config_file(contents: &str) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("failed to create tempdir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(&config_path, contents).expect("failed to write config file");
    (temp_dir, config_path)
}

pub fn xzatoma_binary_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_xzatoma") {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("test executable path should be available");
    if path.ends_with("deps") {
        path.pop();
    } else {
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
    }
    path.push(format!("xzatoma{}", std::env::consts::EXE_SUFFIX));
    path
}

pub fn xzatoma_command() -> std::io::Result<Command> {
    Ok(Command::new(xzatoma_binary_path()))
}
