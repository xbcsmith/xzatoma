/// Skills command handlers.
///
/// This module implements the Phase 5 CLI backend for skills catalog and trust
/// operations, including path visibility reporting and deterministic trust
/// store management.
use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::skills::trust::{
    filter_visible_skill_records, load_trust_store, load_trusted_paths, resolve_trust_store_path,
};
use crate::skills::{discover_skills, SkillCatalog};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// List valid loaded visible skills.
///
/// This command discovers skills, applies trust visibility rules, and prints
/// only valid visible skills.
///
/// # Arguments
///
/// * `config` - Global configuration
///
/// # Errors
///
/// Returns an error if discovery or trust loading fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::skills::list_skills;
/// use xzatoma::config::Config;
///
/// let config = Config::default();
/// let _ = list_skills(config);
/// ```
pub fn list_skills(config: Config) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let trusted_paths = load_trusted_paths(&config.skills, &working_dir)?;
    let visible_catalog = build_visible_skill_catalog(&config, &working_dir, &trusted_paths)?;

    if visible_catalog.is_empty() {
        println!("No valid visible skills found.");
        return Ok(());
    }

    for name in visible_catalog.names() {
        if let Some(record) = visible_catalog.get(name) {
            println!(
                "{}\t{}\t{}",
                record.metadata.name,
                record.metadata.description,
                record.skill_file.display()
            );
        }
    }

    Ok(())
}

/// Validate configured skill roots and print discovery diagnostics.
///
/// This command prints valid visible skills plus invalid and shadowed
/// diagnostics so users can inspect skill discovery issues.
///
/// # Arguments
///
/// * `config` - Global configuration
///
/// # Errors
///
/// Returns an error if discovery or trust loading fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::skills::validate_skills;
/// use xzatoma::config::Config;
///
/// let config = Config::default();
/// let _ = validate_skills(config);
/// ```
pub fn validate_skills(config: Config) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let discovery = discover_skills(&config.skills, &working_dir)?;
    let trusted_paths = load_trusted_paths(&config.skills, &working_dir)?;
    let visible_catalog = build_visible_skill_catalog(&config, &working_dir, &trusted_paths)?;

    if visible_catalog.is_empty() {
        println!("No valid visible skills found.");
    } else {
        println!("Valid visible skills:");
        for name in visible_catalog.names() {
            if let Some(record) = visible_catalog.get(name) {
                println!(
                    "- {}\n  description: {}\n  location: {}\n  scope: {}",
                    record.metadata.name,
                    record.metadata.description,
                    record.skill_file.display(),
                    record.source_scope.as_str()
                );
            }
        }
    }

    if !discovery.invalid_diagnostics.is_empty() {
        println!("\nInvalid skill diagnostics:");
        for diagnostic in &discovery.invalid_diagnostics {
            println!(
                "- [{}] {} ({})",
                diagnostic.code(),
                diagnostic.message,
                diagnostic.skill_file_path.display()
            );
        }
    }

    if !discovery.shadowed_diagnostics.is_empty() {
        println!("\nShadowed skill diagnostics:");
        for diagnostic in &discovery.shadowed_diagnostics {
            let shadowed_by = diagnostic
                .overshadowed_by
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());

            println!(
                "- [{}] {} ({}) shadowed_by={}",
                diagnostic.code(),
                diagnostic.message,
                diagnostic.skill_file_path.display(),
                shadowed_by
            );
        }
    }

    Ok(())
}

/// Show metadata for one valid visible skill.
///
/// # Arguments
///
/// * `config` - Global configuration
/// * `name` - Skill name to display
///
/// # Errors
///
/// Returns an error if the skill is not visible or discovery fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::skills::show_skill;
/// use xzatoma::config::Config;
///
/// let config = Config::default();
/// let _ = show_skill(config, "example_skill");
/// ```
pub fn show_skill(config: Config, name: &str) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let trusted_paths = load_trusted_paths(&config.skills, &working_dir)?;
    let visible_catalog = build_visible_skill_catalog(&config, &working_dir, &trusted_paths)?;

    let record = visible_catalog.get(name).ok_or_else(|| {
        XzatomaError::Config(format!(
            "Skill '{}' is missing, invalid, or not visible in the current trust context",
            name
        ))
    })?;

    println!("name: {}", record.metadata.name);
    println!("description: {}", record.metadata.description);
    println!("scope: {}", record.source_scope.as_str());
    println!("skill_dir: {}", record.skill_dir.display());
    println!("skill_file: {}", record.skill_file.display());

    if let Some(license) = &record.metadata.license {
        println!("license: {}", license);
    }

    if let Some(compatibility) = &record.metadata.compatibility {
        println!("compatibility: {}", compatibility);
    }

    if !record.metadata.allowed_tools.is_empty() {
        println!(
            "allowed_tools: {}",
            record.metadata.allowed_tools.join(", ")
        );
    }

    if !record.metadata.metadata.is_empty() {
        println!("metadata:");
        for (key, value) in &record.metadata.metadata {
            println!("  {}: {}", key, value);
        }
    }

    Ok(())
}

/// Show effective discovery paths and trust state.
///
/// This command prints the effective discovery roots together with their trust
/// status so you can understand which roots are eligible for visible-skill
/// disclosure.
///
/// # Arguments
///
/// * `config` - Global configuration
///
/// # Errors
///
/// Returns an error if path resolution or trust loading fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::skills::show_paths;
/// use xzatoma::config::Config;
///
/// let config = Config::default();
/// let _ = show_paths(config);
/// ```
pub fn show_paths(config: Config) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let trusted_paths = load_trusted_paths(&config.skills, &working_dir)?;
    let trust_store_path = resolve_trust_store_path(config.skills.trust_store_path.as_deref())?;
    let project_client_specific = working_dir.join(".xzatoma").join("skills");
    let project_shared_convention = working_dir.join(".agents").join("skills");
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"));
    let user_client_specific = home.join(".xzatoma").join("skills");
    let user_shared_convention = home.join(".agents").join("skills");

    println!("working_dir: {}", working_dir.display());
    println!("trust_store: {}", trust_store_path.display());
    println!(
        "project_trust_required: {}",
        config.skills.project_trust_required
    );
    println!(
        "allow_custom_paths_without_trust: {}",
        config.skills.allow_custom_paths_without_trust
    );

    println!("\nconfigured discovery roots:");

    if config.skills.project_enabled {
        println!(
            "- project_client_specific: {} [trust={}]",
            project_client_specific.display(),
            root_trust_status(
                &project_client_specific,
                &working_dir,
                &trusted_paths,
                config.skills.project_trust_required
            )
        );
        println!(
            "- project_shared_convention: {} [trust={}]",
            project_shared_convention.display(),
            root_trust_status(
                &project_shared_convention,
                &working_dir,
                &trusted_paths,
                config.skills.project_trust_required
            )
        );
    }

    if config.skills.user_enabled {
        println!(
            "- user_client_specific: {} [trust=not_required]",
            user_client_specific.display()
        );
        println!(
            "- user_shared_convention: {} [trust=not_required]",
            user_shared_convention.display()
        );
    }

    for (index, path) in config.skills.additional_paths.iter().enumerate() {
        let configured_path = PathBuf::from(path);
        println!(
            "- custom_{}: {} [trust={}]",
            index,
            configured_path.display(),
            custom_root_trust_status(
                &configured_path,
                &trusted_paths,
                config.skills.allow_custom_paths_without_trust
            )
        );
    }

    println!("\ntrusted paths:");
    if trusted_paths.is_empty() {
        println!("- <none>");
    } else {
        for path in trusted_paths {
            println!("- {}", path.display());
        }
    }

    Ok(())
}

/// Handle trust-related subcommands.
///
/// This dispatcher routes `skills trust` operations to the persistent trust
/// store implementation.
///
/// # Arguments
///
/// * `command` - Trust subcommand to execute
/// * `config` - Global configuration
///
/// # Errors
///
/// Returns an error if trust store operations fail.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::cli::{SkillsTrustCommand, SkillsTrustPathArgs};
/// use xzatoma::commands::skills::handle_trust;
/// use xzatoma::config::Config;
///
/// let config = Config::default();
/// let command = SkillsTrustCommand::Show;
/// let _ = handle_trust(command, config);
///
/// let add_command = SkillsTrustCommand::Add(SkillsTrustPathArgs {
///     path: PathBuf::from("."),
/// });
/// let _ = handle_trust(add_command, Config::default());
/// ```
pub fn handle_trust(command: crate::cli::SkillsTrustCommand, config: Config) -> Result<()> {
    match command {
        crate::cli::SkillsTrustCommand::Show => show_trust(config),
        crate::cli::SkillsTrustCommand::Add(args) => trust_add(config, &args.path),
        crate::cli::SkillsTrustCommand::Remove(args) => trust_remove(config, &args.path),
    }
}

fn show_trust(config: Config) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let store = load_trust_store(&config.skills, &working_dir)?;
    let trusted = store.trusted_paths();

    println!("trust_store: {}", store.path().display());
    println!(
        "project_trust_required: {}",
        config.skills.project_trust_required
    );
    println!(
        "allow_custom_paths_without_trust: {}",
        config.skills.allow_custom_paths_without_trust
    );

    if trusted.is_empty() {
        println!("trusted_paths: <none>");
    } else {
        println!("trusted_paths:");
        for path in trusted {
            println!("- {}", path.display());
        }
    }

    Ok(())
}

fn trust_add(config: Config, path: &Path) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let mut store = load_trust_store(&config.skills, &working_dir)?;
    let canonical = store.add_path(path)?;

    println!("Trusted path added: {}", canonical.display());
    println!("trust_store: {}", store.path().display());

    Ok(())
}

fn trust_remove(config: Config, path: &Path) -> Result<()> {
    let working_dir = std::env::current_dir()?;
    let mut store = load_trust_store(&config.skills, &working_dir)?;
    let removed = store.remove_path(path)?;

    if removed {
        println!("Trusted path removed: {}", path.display());
    } else {
        println!("Path was not trusted: {}", path.display());
    }

    println!("trust_store: {}", store.path().display());

    Ok(())
}

fn root_trust_status(
    root: &Path,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
    trust_required: bool,
) -> &'static str {
    if !trust_required {
        return "not_required";
    }

    if trusted_paths
        .iter()
        .any(|trusted_path| working_dir.starts_with(trusted_path) || root.starts_with(trusted_path))
    {
        "trusted"
    } else {
        "required"
    }
}

fn custom_root_trust_status(
    root: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
    allow_without_trust: bool,
) -> &'static str {
    if allow_without_trust {
        return "not_required";
    }

    if trusted_paths
        .iter()
        .any(|trusted_path| root.starts_with(trusted_path))
    {
        "trusted"
    } else {
        "required"
    }
}

/// Build a valid visible skill catalog using persistent trust state.
///
/// # Arguments
///
/// * `config` - Global configuration
/// * `working_dir` - Current working directory
/// * `trusted_paths` - Trusted paths loaded from the trust store
///
/// # Returns
///
/// Returns a catalog containing only valid visible skills.
///
/// # Errors
///
/// Returns an error if discovery or catalog construction fails.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeSet;
/// use std::path::Path;
/// use xzatoma::commands::skills::build_visible_skill_catalog;
/// use xzatoma::config::Config;
///
/// let catalog = build_visible_skill_catalog(&Config::default(), Path::new("."), &BTreeSet::new())?;
/// let _ = catalog;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn build_visible_skill_catalog(
    config: &Config,
    working_dir: &Path,
    trusted_paths: &BTreeSet<PathBuf>,
) -> Result<SkillCatalog> {
    if !config.skills.enabled {
        return Ok(SkillCatalog::new());
    }

    let discovery = discover_skills(&config.skills, working_dir)?;
    let visible_records = filter_visible_skill_records(
        &discovery.catalog,
        &config.skills,
        working_dir,
        trusted_paths,
    );

    SkillCatalog::from_records(visible_records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SkillsConfig;
    use crate::skills::trust::SkillTrustStore;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_build_visible_skill_catalog_with_disabled_skills_returns_empty_catalog() {
        let mut config = Config::default();
        config.skills.enabled = false;

        let catalog =
            build_visible_skill_catalog(&config, Path::new("."), &BTreeSet::new()).unwrap();
        assert!(catalog.is_empty());
    }

    #[test]
    fn test_handle_trust_show_succeeds_with_empty_store() {
        let temp_dir = tempdir().unwrap();
        let mut config = Config::default();
        config.skills.trust_store_path = Some(
            temp_dir
                .path()
                .join("skills_trust.yaml")
                .to_string_lossy()
                .to_string(),
        );

        let result = handle_trust(crate::cli::SkillsTrustCommand::Show, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_visible_skill_catalog_no_visible_skills_returns_empty_catalog() {
        let temp_dir = tempdir().unwrap();
        let config = Config {
            skills: SkillsConfig {
                enabled: true,
                project_enabled: false,
                user_enabled: false,
                additional_paths: vec![temp_dir.path().join("missing").display().to_string()],
                ..SkillsConfig::default()
            },
            ..Config::default()
        };

        let catalog =
            build_visible_skill_catalog(&config, temp_dir.path(), &BTreeSet::new()).unwrap();
        assert!(catalog.is_empty());
    }

    #[test]
    fn test_trust_add_and_remove_round_trip() {
        let temp_dir = tempdir().unwrap();
        let trusted_dir = temp_dir.path().join("project");
        fs::create_dir_all(&trusted_dir).unwrap();

        let mut config = Config::default();
        let store_path = temp_dir.path().join("skills_trust.yaml");
        config.skills.trust_store_path = Some(store_path.to_string_lossy().to_string());

        trust_add(config.clone(), &trusted_dir).unwrap();

        let store = SkillTrustStore::load_or_create(store_path.clone()).unwrap();
        assert!(store.is_trusted_path(&trusted_dir));

        trust_remove(config, &trusted_dir).unwrap();

        let reloaded = SkillTrustStore::load_or_create(store_path).unwrap();
        assert!(!reloaded.is_trusted_path(&trusted_dir));
    }
}
