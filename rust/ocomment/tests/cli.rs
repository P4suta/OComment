use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ocomment")
}

fn run(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(binary())
        .current_dir(directory)
        .env("PATH", "/usr/bin:/bin")
        .args(arguments)
        .output()
        .unwrap()
}

fn git(directory: &Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[cfg(unix)]
fn git_with_path(directory: &Path, arguments: &[&str], path: &std::ffi::OsStr) -> Vec<u8> {
    let output = Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn repository() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "-q"]);
    let hooks = directory.path().join(".git/no-hooks");
    fs::create_dir(&hooks).unwrap();
    git(
        directory.path(),
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    git(
        directory.path(),
        &["config", "user.email", "test@example.test"],
    );
    git(directory.path(), &["config", "user.name", "OComment Test"]);
    git(directory.path(), &["config", "commit.gpgsign", "false"]);
    directory
}

#[test]
fn check_diff_and_fix_follow_the_exit_contract() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.rs");
    fs::write(&path, b"fn main() { // remove\r\n}\r\n").unwrap();

    let checked = run(directory.path(), &["check", "sample.rs"]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&checked.stdout).contains("removable"));

    let diff = run(directory.path(), &["diff", "sample.rs"]);
    assert_eq!(diff.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&diff.stdout).contains("--- a/sample.rs"));

    let fixed = run(directory.path(), &["fix", "sample.rs"]);
    assert_eq!(
        fixed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    assert_eq!(fs::read(&path).unwrap(), b"fn main() { \r\n}\r\n");
    assert_eq!(
        run(directory.path(), &["check", "sample.rs"]).status.code(),
        Some(0)
    );
}

#[test]
fn invalid_input_returns_two_and_fix_is_non_destructive() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.c");
    let original = b"int x; /* unterminated";
    fs::write(&path, original).unwrap();
    let output = run(directory.path(), &["fix", "broken.c"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn force_invalid_applies_known_spans_but_keeps_error_exit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.c");
    fs::write(&path, b"int x; // remove\n\"unterminated").unwrap();
    let output = run(directory.path(), &["fix", "broken.c", "--force-invalid"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), b"int x; \n\"unterminated");
}

#[test]
fn json_and_jsonl_are_machine_readable() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("x.py"), b"x = 1 # remove\n").unwrap();
    let json = run(directory.path(), &["scan", "x.py", "--format", "json"]);
    assert_eq!(json.status.code(), Some(0));
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["files"][0]["language"], "python");
    let jsonl = run(directory.path(), &["scan", "x.py", "--format", "jsonl"]);
    assert_eq!(
        jsonl
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        1
    );
}

#[test]
fn strict_configuration_suggests_unknown_keys() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\npolciy = \"safe\"\n",
    )
    .unwrap();
    let output = run(directory.path(), &["config", "show"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("unknown field"));
    assert!(error.contains("policy"));
}

#[test]
fn declarative_profile_is_used_by_extension() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        br#"version = 1

[profiles.demo]
extensions = ["demo"]

[[profiles.demo.line_comments]]
start = ";;"

[[profiles.demo.strings]]
start = "\""
end = "\""
escape = "\\"
"#,
    )
    .unwrap();
    let path = directory.path().join("x.demo");
    fs::write(&path, b"\";; string\" ;; remove\n").unwrap();
    let output = run(directory.path(), &["fix", "x.demo"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(path).unwrap(), b"\";; string\" \n");
}

#[test]
fn init_lefthook_preserves_partial_stage_contract() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["init", "lefthook", "--fix"]);
    assert_eq!(output.status.code(), Some(0));
    let generated = fs::read_to_string(directory.path().join("lefthook.yml")).unwrap();
    assert!(generated.contains("ocomment fix --staged"));
    assert!(!generated.contains("stage_fixed"));
}

#[test]
fn staged_fix_does_not_stage_unrelated_working_tree_changes() {
    let directory = repository();
    let path = directory.path().join("partial stage.rs");
    fs::write(&path, b"let a = 1; // existing\nlet b = 2;\n").unwrap();
    git(directory.path(), &["add", "partial stage.rs"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );

    fs::write(&path, b"let a = 1; // existing\nlet b = 2; // staged\n").unwrap();
    git(directory.path(), &["add", "partial stage.rs"]);
    fs::write(
        &path,
        b"let a = 1; // existing\nlet b = 2; // staged\nlet c = 3; // unstaged\n",
    )
    .unwrap();

    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let staged = git(directory.path(), &["show", ":partial stage.rs"]);
    assert_eq!(staged, b"let a = 1; // existing\nlet b = 2; \n");
    assert_eq!(
        fs::read(&path).unwrap(),
        b"let a = 1; // existing\nlet b = 2; \nlet c = 3; // unstaged\n"
    );
    let unstaged =
        String::from_utf8(git(directory.path(), &["diff", "--", "partial stage.rs"])).unwrap();
    assert!(unstaged.contains("// unstaged"));
    let cached = String::from_utf8(git(
        directory.path(),
        &["diff", "--cached", "--", "partial stage.rs"],
    ))
    .unwrap();
    assert!(!cached.contains("// unstaged"));
}

#[test]
fn staged_fix_stops_on_existing_block_comment_interior() {
    let directory = repository();
    let path = directory.path().join("block.c");
    fs::write(&path, b"/* existing\nbase\n*/\nint x;\n").unwrap();
    git(directory.path(), &["add", "block.c"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );
    let staged_source = b"/* existing\nbase\nadded\n*/\nint x;\n";
    fs::write(&path, staged_source).unwrap();
    git(directory.path(), &["add", "block.c"]);
    let before_index = git(directory.path(), &["show", ":block.c"]);

    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stdout).contains("staged-existing-block-comment"));
    assert_eq!(git(directory.path(), &["show", ":block.c"]), before_index);
    assert_eq!(fs::read(path).unwrap(), staged_source);
}

#[test]
fn staged_new_rename_delete_and_unusual_paths_are_handled_from_index_blobs() {
    let directory = repository();
    fs::write(directory.path().join("renamed.rs"), b"let base = 1;\n").unwrap();
    fs::write(directory.path().join("deleted.rs"), b"let deleted = 1;\n").unwrap();
    git(directory.path(), &["add", "."]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );

    git(directory.path(), &["mv", "renamed.rs", "renamed target.rs"]);
    fs::write(
        directory.path().join("renamed target.rs"),
        b"let base = 1;\nlet renamed = 2; // staged rename\n",
    )
    .unwrap();
    git(directory.path(), &["add", "renamed target.rs"]);
    fs::remove_file(directory.path().join("deleted.rs")).unwrap();
    git(directory.path(), &["add", "deleted.rs"]);
    let unusual = "odd\n名前.rs";
    fs::write(
        directory.path().join(unusual),
        b"let new = 1; // new file\n",
    )
    .unwrap();
    git(directory.path(), &["add", unusual]);

    let output = run(directory.path(), &["fix", "--staged", "--index-only"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        git(directory.path(), &["show", ":renamed target.rs"]),
        b"let base = 1;\nlet renamed = 2; \n"
    );
    assert_eq!(
        git(directory.path(), &["show", &format!(":{unusual}")]),
        b"let new = 1; \n"
    );
    assert_eq!(
        fs::read(directory.path().join("renamed target.rs")).unwrap(),
        b"let base = 1;\nlet renamed = 2; // staged rename\n"
    );
    assert_eq!(
        fs::read(directory.path().join(unusual)).unwrap(),
        b"let new = 1; // new file\n"
    );
}

#[cfg(unix)]
#[test]
fn staged_non_utf8_paths_remain_os_native() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let directory = repository();
    let name = std::ffi::OsString::from_vec(b"non-\xff.rs".to_vec());
    let path = directory.path().join(&name);
    fs::write(&path, b"let value = 1; // remove\n").unwrap();
    git_with_path(directory.path(), &["add", "--"], &name);

    let output = run(directory.path(), &["fix", "--staged", "--index-only"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut specification = std::ffi::OsString::from(":");
    specification.push(&name);
    assert_eq!(
        git_with_path(
            directory.path(),
            &["cat-file", "blob"],
            specification.as_os_str()
        ),
        b"let value = 1; \n"
    );
    assert_eq!(name.as_bytes(), b"non-\xff.rs");
}

#[test]
fn ambiguous_staged_mapping_changes_nothing_and_suggests_index_only() {
    let directory = repository();
    let path = directory.path().join("ambiguous.rs");
    fs::write(&path, b"let base = 1;\n").unwrap();
    git(directory.path(), &["add", "ambiguous.rs"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "base"],
    );

    let staged = b"let base = 1;\nlet staged = 2; // remove\n";
    fs::write(&path, staged).unwrap();
    git(directory.path(), &["add", "ambiguous.rs"]);
    let working = [staged.as_slice(), staged.as_slice()].concat();
    fs::write(&path, &working).unwrap();
    let before_index = git(directory.path(), &["show", ":ambiguous.rs"]);

    let output = run(directory.path(), &["fix", "--staged"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--index-only"));
    assert_eq!(
        git(directory.path(), &["show", ":ambiguous.rs"]),
        before_index
    );
    assert_eq!(fs::read(&path).unwrap(), working);

    let index_only = run(directory.path(), &["fix", "--staged", "--index-only"]);
    assert_eq!(index_only.status.code(), Some(0));
    assert_eq!(
        git(directory.path(), &["show", ":ambiguous.rs"]),
        b"let base = 1;\nlet staged = 2; \n"
    );
    assert_eq!(fs::read(path).unwrap(), working);
}

#[test]
fn normal_walk_respects_gitignore() {
    let directory = repository();
    fs::write(directory.path().join(".gitignore"), b"ignored.rs\n").unwrap();
    fs::write(directory.path().join("ignored.rs"), b"// ignored\n").unwrap();
    fs::write(directory.path().join("seen.rs"), b"// seen\n").unwrap();
    let output = run(directory.path(), &[]);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("seen.rs"));
    assert!(!text.contains("ignored.rs"));
}

#[test]
fn explicit_io_failure_returns_two_and_blocks_the_whole_fix() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("good.rs");
    let original = b"let x = 1; // remove\n";
    fs::write(&path, original).unwrap();
    let output = run(directory.path(), &["fix", "good.rs", "missing.rs"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(String::from_utf8_lossy(&output.stdout).contains("path does not exist"));
}

#[test]
fn no_argument_scan_uses_repository_root_from_a_subdirectory() {
    let directory = repository();
    fs::write(directory.path().join("root.rs"), b"// root comment\n").unwrap();
    let nested = directory.path().join("nested/deeper");
    fs::create_dir_all(&nested).unwrap();
    let output = run(&nested, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("root.rs"));
}

#[test]
fn explicit_directory_bypasses_hidden_and_size_limits() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[files]\nmax_size = 1\n",
    )
    .unwrap();
    fs::write(
        directory.path().join(".hidden.rs"),
        b"// hidden and larger than one byte\n",
    )
    .unwrap();
    let output = run(directory.path(), &["check", "."]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains(".hidden.rs"));
}

#[cfg(unix)]
#[test]
fn symlink_following_is_explicitly_configurable() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("source.txt"),
        b"let x = 1; // remove\n",
    )
    .unwrap();
    symlink("source.txt", directory.path().join("link.rs")).unwrap();

    let skipped = run(directory.path(), &["check", "link.rs"]);
    assert_eq!(skipped.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&skipped.stdout).contains("symbolic link"));

    fs::write(
        directory.path().join(".ocomment.toml"),
        b"version = 1\n[files]\nfollow_symlinks = true\n",
    )
    .unwrap();
    let followed = run(directory.path(), &["check", "link.rs"]);
    assert_eq!(followed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&followed.stdout).contains("removable"));
}
