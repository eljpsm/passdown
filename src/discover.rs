use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::app::RunStatus;
use crate::config::DiscoveryPolicy;
use crate::file_io::{FileIdentity, canonicalize_and_inspect};

#[derive(Debug)]
pub struct DiscoveredFile {
    /// The path as given or found, used for reporting and sort order.
    pub(crate) display_path: PathBuf,
    /// The canonicalized path (symlinks resolved), used for reads and writes.
    pub(crate) target_path: PathBuf,
    /// Identity at discovery time; `atomic_replace` rechecks it at commit.
    pub(crate) identity: FileIdentity,
}

#[derive(Debug)]
pub struct Discovered {
    /// Deduplicated by physical identity and sorted by the retained display
    /// path for deterministic output.
    pub files: Vec<DiscoveredFile>,
    /// Operational errors encountered while expanding or inspecting paths.
    pub errors: Vec<DiscoveryError>,
}

/// An operational failure encountered during discovery.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryError {
    MissingPath {
        path: PathBuf,
    },
    Walk {
        root: PathBuf,
        path: Option<PathBuf>,
        message: String,
    },
    Inspect {
        path: PathBuf,
        message: String,
    },
}

impl DiscoveryError {
    /// Discovery errors are operational failures.
    pub const fn status(&self) -> RunStatus {
        RunStatus::Failure
    }

    /// Best available path for deterministic ordering and context.
    pub fn path(&self) -> &Path {
        match self {
            Self::MissingPath { path } | Self::Inspect { path, .. } => path,
            Self::Walk { root, path, .. } => path.as_deref().unwrap_or(root),
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// Expand file/directory arguments into the list of Markdown files to process.
/// Explicitly named files bypass ignore rules; directories are walked with
/// the validated discovery policy.
pub fn discover(paths: &[PathBuf], policy: &DiscoveryPolicy) -> Discovered {
    let mut candidates: BTreeSet<PathBuf> = BTreeSet::new();
    let mut directory_roots: BTreeSet<PathBuf> = BTreeSet::new();
    let mut errors = Vec::new();

    for path in paths {
        if path.is_file() {
            candidates.insert(path.clone());
        } else if path.is_dir() {
            directory_roots.insert(path.clone());
        } else {
            errors.push(DiscoveryError::MissingPath { path: path.clone() });
        }
    }

    let directory_roots: Vec<PathBuf> = directory_roots
        .into_iter()
        .filter(|root| policy.should_walk_root(root))
        .collect();
    if !directory_roots.is_empty() {
        for entry in policy.walk_builder(&directory_roots).build() {
            match entry {
                Ok(entry) => {
                    let is_file = entry.file_type().is_some_and(|kind| kind.is_file());
                    if is_file && is_markdown(entry.path()) {
                        candidates.insert(entry.into_path());
                    }
                }
                Err(err) => {
                    let path = ignore_error_path(&err).map(Path::to_path_buf);
                    errors.push(DiscoveryError::Walk {
                        root: error_root(path.as_deref(), &directory_roots).to_path_buf(),
                        path,
                        message: err.to_string(),
                    });
                }
            }
        }
    }

    let mut identities = HashSet::new();
    let mut files = Vec::new();
    for display_path in candidates {
        match canonicalize_and_inspect(&display_path) {
            Ok((target_path, inspected)) => {
                if identities.insert(inspected.identity) {
                    files.push(DiscoveredFile {
                        display_path,
                        target_path,
                        identity: inspected.identity,
                    });
                }
            }
            Err(err) => {
                errors.push(DiscoveryError::Inspect {
                    path: display_path,
                    message: err.to_string(),
                });
            }
        }
    }

    Discovered { files, errors }
}

/// Dig the most specific path out of `ignore`'s nested error structure, for
/// deterministic ordering of walk errors.
fn ignore_error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::Partial(errors) => errors.iter().find_map(ignore_error_path),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => {
            ignore_error_path(err)
        }
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::Loop { child, .. } => Some(child),
        ignore::Error::Io(_)
        | ignore::Error::Glob { .. }
        | ignore::Error::UnrecognizedFileType(_)
        | ignore::Error::InvalidDefinition => None,
    }
}

/// Attribute a walk error to the deepest root containing its path. Errors
/// with no usable path fall back to the first root.
fn error_root<'a>(error_path: Option<&Path>, roots: &'a [PathBuf]) -> &'a Path {
    error_path
        .and_then(|path| {
            roots
                .iter()
                .filter(|root| path.starts_with(root))
                .max_by_key(|root| root.components().count())
        })
        .unwrap_or(&roots[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, DiscoveryPolicy};

    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("passdown-discover-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            TempTree { root }
        }

        fn write(&self, rel: &str, contents: &str) {
            let path = self.root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn rel_files(discovered: &Discovered, root: &Path) -> Vec<String> {
        discovered
            .files
            .iter()
            .map(|file| {
                file.display_path
                    .as_path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    fn policy(root: &Path, config: Config) -> DiscoveryPolicy {
        DiscoveryPolicy::new(root.to_path_buf(), config).unwrap()
    }

    #[test]
    fn finds_markdown_sorted_and_deduped() {
        let tree = TempTree::new("sorted");
        tree.write("b.md", "");
        tree.write("a.md", "");
        tree.write("sub/c.markdown", "");
        tree.write("note.txt", "");
        let policy = policy(&tree.root, Config::default());
        let discovered = discover(&[tree.root.clone(), tree.root.clone()], &policy);
        assert!(discovered.errors.is_empty());
        assert_eq!(
            rel_files(&discovered, &tree.root),
            ["a.md", "b.md", "sub/c.markdown"]
        );
    }

    #[test]
    fn gitignore_respected_when_enabled() {
        let tree = TempTree::new("gitignore");
        tree.write(".gitignore", "ignored.md\n");
        tree.write("kept.md", "");
        tree.write("ignored.md", "");

        let on = policy(&tree.root, Config::default());
        let discovered = discover(std::slice::from_ref(&tree.root), &on);
        assert_eq!(rel_files(&discovered, &tree.root), ["kept.md"]);

        let off = policy(
            &tree.root,
            Config {
                use_gitignore: false,
                ..Config::default()
            },
        );
        let discovered = discover(std::slice::from_ref(&tree.root), &off);
        assert_eq!(
            rel_files(&discovered, &tree.root),
            ["ignored.md", "kept.md"]
        );
    }

    #[test]
    fn config_ignore_globs_apply() {
        let tree = TempTree::new("globs");
        tree.write("keep.md", "");
        tree.write("vendor/skip.md", "");
        let policy = policy(
            &tree.root,
            Config {
                ignore: vec!["vendor/".to_string()],
                use_gitignore: true,
            },
        );
        let discovered = discover(std::slice::from_ref(&tree.root), &policy);
        assert_eq!(rel_files(&discovered, &tree.root), ["keep.md"]);
    }

    #[test]
    fn explicit_files_bypass_ignore_rules() {
        let tree = TempTree::new("explicit");
        tree.write(".gitignore", "draft.md\n");
        tree.write("draft.md", "");
        let policy = policy(&tree.root, Config::default());
        let discovered = discover(&[tree.root.join("draft.md")], &policy);
        assert_eq!(rel_files(&discovered, &tree.root), ["draft.md"]);
    }

    #[test]
    fn multiple_roots_share_one_policy_and_retain_sorted_results() {
        let tree = TempTree::new("multiple-roots");
        tree.write("first/.gitignore", "ignored.md\n");
        tree.write("first/ignored.md", "");
        tree.write("first/kept.md", "");
        tree.write("first/configured.md", "");
        tree.write("second/ignored.md", "");
        tree.write("second/kept.md", "");
        tree.write("second/configured.md", "");
        let policy = policy(
            &tree.root,
            Config {
                ignore: vec!["**/configured.md".to_owned()],
                ..Config::default()
            },
        );

        let discovered = discover(
            &[
                tree.root.join("second"),
                tree.root.join("first"),
                tree.root.join("first"),
            ],
            &policy,
        );

        assert!(discovered.errors.is_empty());
        assert_eq!(
            rel_files(&discovered, &tree.root),
            ["first/kept.md", "second/ignored.md", "second/kept.md"]
        );
    }

    #[test]
    fn nested_roots_still_produce_one_file_outcome() {
        let tree = TempTree::new("nested-roots");
        tree.write("root/doc.md", "");
        tree.write("root/nested/doc.md", "");
        let policy = policy(&tree.root, Config::default());

        let discovered = discover(
            &[tree.root.join("root"), tree.root.join("root/nested")],
            &policy,
        );

        assert!(discovered.errors.is_empty());
        assert_eq!(
            rel_files(&discovered, &tree.root),
            ["root/doc.md", "root/nested/doc.md"]
        );
    }

    #[test]
    fn missing_path_is_an_error() {
        let tree = TempTree::new("missing");
        let policy = policy(&tree.root, Config::default());
        let missing = tree.root.join("nope.md");
        let discovered = discover(std::slice::from_ref(&missing), &policy);
        assert_eq!(
            discovered.errors,
            [DiscoveryError::MissingPath {
                path: missing.clone()
            }]
        );
        assert_eq!(discovered.errors[0].path(), missing);
        assert!(discovered.files.is_empty());
    }

    #[test]
    fn aliases_are_deduplicated_by_physical_identity() {
        let tree = TempTree::new("aliases");
        tree.write("doc.md", "messy   text\n");
        let direct = tree.root.join("doc.md");
        let dotted = tree.root.join("./doc.md");

        let policy = policy(&tree.root, Config::default());
        let discovered = discover(&[direct, dotted], &policy);

        assert!(discovered.errors.is_empty());
        assert_eq!(discovered.files.len(), 1);
    }

    #[test]
    fn hard_link_aliases_are_deduplicated() {
        let tree = TempTree::new("hardlinks");
        tree.write("first.md", "messy   text\n");
        std::fs::hard_link(tree.root.join("first.md"), tree.root.join("second.md")).unwrap();

        let policy = policy(&tree.root, Config::default());
        let discovered = discover(std::slice::from_ref(&tree.root), &policy);

        assert!(discovered.errors.is_empty());
        assert_eq!(discovered.files.len(), 1);
    }

    #[test]
    fn hidden_markdown_is_included_but_vcs_directories_are_not() {
        let tree = TempTree::new("hidden");
        tree.write(".draft.md", "");
        tree.write(".github/doc.md", "");
        tree.write(".git/internal.md", "");
        tree.write(".hg/internal.md", "");
        tree.write(".svn/internal.md", "");
        tree.write(".jj/internal.md", "");
        let policy = policy(
            &tree.root,
            Config {
                use_gitignore: false,
                ..Config::default()
            },
        );

        let discovered = discover(std::slice::from_ref(&tree.root), &policy);

        assert_eq!(
            rel_files(&discovered, &tree.root),
            [".draft.md", ".github/doc.md"]
        );
    }

    #[test]
    fn explicit_vcs_file_bypasses_policy_but_vcs_directory_does_not() {
        let tree = TempTree::new("explicit-vcs");
        tree.write(".git/internal.md", "");
        let policy = policy(&tree.root, Config::default());

        let file = discover(&[tree.root.join(".git/internal.md")], &policy);
        let directory = discover(&[tree.root.join(".git")], &policy);

        assert_eq!(rel_files(&file, &tree.root), [".git/internal.md"]);
        assert!(directory.files.is_empty());
    }

    #[test]
    fn walk_errors_use_the_longest_matching_root() {
        let roots = vec![
            PathBuf::from("a"),
            PathBuf::from("a/nested"),
            PathBuf::from("b"),
        ];

        assert_eq!(
            error_root(Some(Path::new("a/nested/doc.md")), &roots),
            Path::new("a/nested")
        );
        assert_eq!(error_root(None, &roots), Path::new("a"));
    }
}
