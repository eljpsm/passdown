use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use ignore::WalkBuilder;
use ignore::overrides::{Override, OverrideBuilder};
use serde::Deserialize;

pub const CONFIG_FILE_NAME: &str = "passdown.toml";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Files or directories to ignore, in gitignore glob syntax.
    pub ignore: Vec<String>,
    /// Whether .gitignore entries are concatenated into the ignore list.
    pub use_gitignore: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ignore: Vec::new(),
            use_gitignore: true,
        }
    }
}

/// The compiled form of `Config`: the gitignore toggle plus pre-built ignore
/// matchers, validated at load time. Kept as one unit on purpose. Do not
/// split the raw config and matcher back into independently passed values.
#[derive(Debug)]
pub struct DiscoveryPolicy {
    use_gitignore: bool,
    overrides: Option<Override>,
}

impl DiscoveryPolicy {
    pub fn new(root: PathBuf, config: Config) -> anyhow::Result<Self> {
        let overrides = if config.ignore.is_empty() {
            None
        } else {
            let mut builder = OverrideBuilder::new(&root);
            // Override semantics are whitelist by default, so invert each
            // configured glob to implement passdown's blacklist-only option.
            for glob in &config.ignore {
                builder.add(&format!("!{glob}")).with_context(|| {
                    format!("invalid ignore glob {glob:?} in {CONFIG_FILE_NAME}")
                })?;
            }
            Some(builder.build().context("failed to compile ignore globs")?)
        };
        Ok(DiscoveryPolicy {
            use_gitignore: config.use_gitignore,
            overrides,
        })
    }

    /// VCS metadata directories are refused even when named explicitly as
    /// roots. Only explicitly named files bypass discovery filtering.
    pub(crate) fn should_walk_root(&self, root: &Path) -> bool {
        !root.file_name().is_some_and(is_vcs_directory_name)
    }

    /// One walker over all directory roots: hidden files included, symlinks
    /// not followed, gitignore honored per config, and VCS metadata
    /// directories never descended into.
    pub(crate) fn walk_builder(&self, roots: &[PathBuf]) -> WalkBuilder {
        let mut builder = WalkBuilder::empty();
        for root in roots {
            builder.add(root);
        }
        builder
            .follow_links(false)
            .hidden(false)
            .ignore(false)
            .git_global(false)
            .git_ignore(self.use_gitignore)
            .git_exclude(self.use_gitignore)
            .require_git(false)
            .filter_entry(|entry| {
                !entry.file_type().is_some_and(|kind| kind.is_dir())
                    || !is_vcs_directory_name(entry.file_name())
            });
        if let Some(overrides) = &self.overrides {
            builder.overrides(overrides.clone());
        }
        builder
    }
}

fn is_vcs_directory_name(name: &OsStr) -> bool {
    matches!(name.to_str(), Some(".git" | ".hg" | ".svn" | ".jj"))
}

/// Walk upward from `start_dir`; the first `passdown.toml` found wins.
/// A missing config file means defaults.
pub fn load(start_dir: &Path) -> anyhow::Result<DiscoveryPolicy> {
    for dir in start_dir.ancestors() {
        let candidate = dir.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            let text = std::fs::read_to_string(&candidate)
                .with_context(|| format!("failed to read {}", candidate.display()))?;
            let config: Config = toml::from_str(&text)
                .with_context(|| format!("invalid config {}", candidate.display()))?;
            return DiscoveryPolicy::new(dir.to_path_buf(), config);
        }
    }
    DiscoveryPolicy::new(start_dir.to_path_buf(), Config::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("passdown-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_config_yields_defaults() {
        let dir = temp_dir("missing");
        let policy = load(&dir).unwrap();
        assert!(policy.use_gitignore);
        assert!(policy.overrides.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn walks_upward_to_find_config() {
        let root = temp_dir("walkup");
        std::fs::write(
            root.join(CONFIG_FILE_NAME),
            "ignore = [\"vendor/\"]\nuse_gitignore = false\n",
        )
        .unwrap();
        let nested = root.join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        let policy = load(&nested).unwrap();
        assert!(!policy.use_gitignore);
        assert!(
            policy
                .overrides
                .as_ref()
                .unwrap()
                .matched(root.join("vendor"), true)
                .is_ignore()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn nearest_config_wins() {
        let root = temp_dir("nearest");
        std::fs::write(root.join(CONFIG_FILE_NAME), "ignore = [\"outer\"]\n").unwrap();
        let inner = root.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join(CONFIG_FILE_NAME), "ignore = [\"inner\"]\n").unwrap();
        let policy = load(&inner).unwrap();
        let overrides = policy.overrides.as_ref().unwrap();
        assert!(overrides.matched(inner.join("inner"), false).is_ignore());
        assert!(!overrides.matched(root.join("outer"), false).is_ignore());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let root = temp_dir("unknown");
        std::fs::write(root.join(CONFIG_FILE_NAME), "line_width = 100\n").unwrap();
        assert!(load(&root).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn invalid_ignore_globs_are_rejected_during_load() {
        let root = temp_dir("invalid-glob");
        std::fs::write(root.join(CONFIG_FILE_NAME), "ignore = [\"[\"]\n").unwrap();
        let error = load(&root).unwrap_err();
        assert!(error.to_string().contains("invalid ignore glob"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
