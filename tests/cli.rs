//! End-to-end tests of the binary: exit codes, check/fix behavior, config
//! and gitignore handling.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("passdown-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        TempTree { root }
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.join(rel)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_passdown"))
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap()
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn clean_tree_exits_zero() {
    let tree = TempTree::new("clean");
    tree.write("doc.md", "Already formatted.\n");
    assert_eq!(code(&tree.run(&["check", "."])), 0);
    assert_eq!(code(&tree.run(&["fix", "."])), 0);
}

#[test]
fn check_without_paths_uses_the_current_directory() {
    let tree = TempTree::new("default-check");
    tree.write("doc.md", "messy   text\n");

    let check = tree.run(&["check"]);

    assert_eq!(code(&check), 1);
    assert!(stdout(&check).contains("would reformat: ./doc.md"));
}

#[test]
fn fix_without_paths_uses_the_current_directory() {
    let tree = TempTree::new("default-fix");
    tree.write("doc.md", "messy   text\n");

    let fix = tree.run(&["fix"]);

    assert_eq!(code(&fix), 0);
    assert_eq!(tree.read("doc.md"), "messy text\n");
}

#[test]
fn check_reports_and_fix_rewrites() {
    let tree = TempTree::new("messy");
    tree.write("doc.md", "Some “curly”   text\nwith bad breaks.\n");

    let check = tree.run(&["check", "."]);
    assert_eq!(code(&check), 1);
    let out = stdout(&check);
    assert!(out.contains("would reformat:"), "stdout was: {out}");
    assert!(out.contains("non-ASCII punctuation"), "stdout was: {out}");

    assert_eq!(code(&tree.run(&["fix", "."])), 0);
    assert_eq!(
        tree.read("doc.md"),
        "Some \"curly\" text with bad breaks.\n"
    );
    assert_eq!(code(&tree.run(&["check", "."])), 0);
}

#[test]
fn untyped_fence_is_unfixable() {
    let tree = TempTree::new("untyped");
    tree.write("doc.md", "~~~~\ncode\n~~~~\n");

    let check = tree.run(&["check", "."]);
    assert_eq!(code(&check), 1);
    assert!(stderr(&check).contains("code block has no language"));

    let fix = tree.run(&["fix", "."]);
    assert_eq!(code(&fix), 1);
    assert!(stderr(&fix).contains("code block has no language"));
    assert_eq!(tree.read("doc.md"), "~~~~\ncode\n~~~~\n");
}

#[test]
fn missing_path_is_operational_error() {
    let tree = TempTree::new("nopath");
    let missing = tree.run(&["check", "nope.md"]);
    assert_eq!(code(&missing), 2);
    assert_eq!(
        stderr(&missing),
        "passdown: no such file or directory: nope.md\n"
    );
    let usage = Command::new(env!("CARGO_BIN_EXE_passdown"))
        .output()
        .unwrap();
    assert_eq!(usage.status.code().unwrap(), 2);
}

#[test]
fn discovery_failure_does_not_stop_valid_files() {
    let tree = TempTree::new("mixed-discovery");
    tree.write("doc.md", "messy   text\n");

    let check = tree.run(&["check", "missing.md", "doc.md"]);

    assert_eq!(code(&check), 2);
    assert!(stdout(&check).contains("would reformat: doc.md"));
    assert_eq!(
        stderr(&check),
        "passdown: no such file or directory: missing.md\n"
    );
}

#[test]
fn discovery_errors_are_sorted_independently_of_argument_order() {
    let tree = TempTree::new("discovery-order");

    let check = tree.run(&["check", "z-missing.md", "a-missing.md"]);

    assert_eq!(code(&check), 2);
    assert_eq!(
        stderr(&check),
        concat!(
            "passdown: no such file or directory: a-missing.md\n",
            "passdown: no such file or directory: z-missing.md\n"
        )
    );
}

#[test]
fn invalid_config_is_operational_error() {
    let tree = TempTree::new("badconfig");
    tree.write("passdown.toml", "line_width = 100\n");
    tree.write("doc.md", "hi\n");
    assert_eq!(code(&tree.run(&["check", "."])), 2);
}

#[test]
fn invalid_ignore_glob_fails_before_discovery() {
    let tree = TempTree::new("badglob");
    tree.write("passdown.toml", "ignore = [\"[\"]\n");
    tree.write("doc.md", "bad   spacing\n");

    let check = tree.run(&["check", "."]);

    assert_eq!(code(&check), 2);
    assert!(stderr(&check).contains("invalid ignore glob"));
    assert!(stdout(&check).is_empty());
}

#[test]
fn gitignore_and_config_ignore_are_honored() {
    let tree = TempTree::new("ignores");
    tree.write(".gitignore", "generated.md\n");
    tree.write("generated.md", "bad    spacing\neverywhere\n");
    tree.write("vendor/third.md", "bad    spacing\n");
    tree.write("passdown.toml", "ignore = [\"vendor/\"]\n");
    assert_eq!(code(&tree.run(&["check", "."])), 0);

    // Turning gitignore concatenation off surfaces the generated file.
    tree.write(
        "passdown.toml",
        "ignore = [\"vendor/\"]\nuse_gitignore = false\n",
    );
    let check = tree.run(&["check", "."]);
    assert_eq!(code(&check), 1);
    assert!(stdout(&check).contains("generated.md"));
    assert!(!stdout(&check).contains("vendor"));
}

#[test]
fn fix_is_idempotent_on_disk() {
    let tree = TempTree::new("idempotent");
    tree.write(
        "doc.md",
        "Title\n=====\n\n* a\n* b\n\n1) one\n2) two\n\n> quoted   text\nlazy line\n",
    );
    assert_eq!(code(&tree.run(&["fix", "."])), 0);
    let first = tree.read("doc.md");
    assert_eq!(code(&tree.run(&["fix", "."])), 0);
    assert_eq!(tree.read("doc.md"), first);
    let path = Path::new("doc.md");
    assert!(path.is_relative());
}

#[test]
fn aliased_paths_are_reported_once() {
    let tree = TempTree::new("aliases");
    tree.write("doc.md", "messy   text\n");

    let check = tree.run(&["check", "doc.md", "./doc.md"]);

    assert_eq!(code(&check), 1);
    assert_eq!(stdout(&check).matches("would reformat:").count(), 1);
}

#[test]
fn hidden_markdown_is_checked_but_vcs_metadata_is_not() {
    let tree = TempTree::new("hidden");
    tree.write(".draft.md", "draft   text\n");
    tree.write(".github/doc.md", "github   text\n");
    tree.write(".git/internal.md", "git   text\n");
    tree.write(".hg/internal.md", "hg   text\n");
    tree.write(".svn/internal.md", "svn   text\n");
    tree.write(".jj/internal.md", "jj   text\n");

    let check = tree.run(&["check", "."]);
    let out = stdout(&check);

    assert_eq!(code(&check), 1);
    assert!(out.contains(".draft.md"));
    assert!(out.contains(".github/doc.md"));
    assert!(!out.contains(".git/internal.md"));
    assert!(!out.contains(".hg/internal.md"));
    assert!(!out.contains(".svn/internal.md"));
    assert!(!out.contains(".jj/internal.md"));
}

#[test]
fn hard_link_is_refused_without_stopping_other_files() {
    let tree = TempTree::new("hardlink");
    tree.write("linked.md", "linked   text\n");
    std::fs::hard_link(tree.root.join("linked.md"), tree.root.join("alias.md")).unwrap();
    tree.write("ordinary.md", "ordinary   text\n");

    let fix = tree.run(&["fix", "linked.md", "ordinary.md"]);

    assert_eq!(code(&fix), 2);
    assert!(stderr(&fix).contains("refusing to rewrite hard-linked file linked.md"));
    assert_eq!(tree.read("linked.md"), "linked   text\n");
    assert_eq!(tree.read("alias.md"), "linked   text\n");
    assert_eq!(tree.read("ordinary.md"), "ordinary text\n");
}

#[test]
fn clean_hard_link_is_a_successful_noop() {
    let tree = TempTree::new("clean-hardlink");
    tree.write("linked.md", "Already formatted.\n");
    std::fs::hard_link(tree.root.join("linked.md"), tree.root.join("alias.md")).unwrap();

    let fix = tree.run(&["fix", "linked.md"]);

    assert_eq!(code(&fix), 0);
    assert_eq!(tree.read("linked.md"), "Already formatted.\n");
    assert_eq!(tree.read("alias.md"), "Already formatted.\n");
}

#[cfg(any(unix, windows))]
#[test]
fn fix_follows_and_preserves_a_file_symlink() {
    let tree = TempTree::new("symlink");
    tree.write("target.md", "linked   text\n");
    let link = tree.root.join("link.md");
    if let Err(err) = symlink_file(Path::new("target.md"), &link) {
        #[cfg(windows)]
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            return;
        }
        panic!("failed to create symlink: {err}");
    }

    let fix = tree.run(&["fix", "link.md", "target.md"]);

    assert_eq!(code(&fix), 0, "stderr was: {}", stderr(&fix));
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(tree.read("target.md"), "linked text\n");
}

#[cfg(unix)]
fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
