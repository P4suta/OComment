use std::{ffi::OsStr, process::Command, sync::OnceLock};

pub fn isolated(program: impl AsRef<OsStr>) -> Command {
    static GIT_CONFIG: OnceLock<tempfile::NamedTempFile> = OnceLock::new();
    let config = GIT_CONFIG
        .get_or_init(|| tempfile::NamedTempFile::new().expect("a private Git configuration"));
    let mut command = Command::new(program);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command.env("GIT_CONFIG_GLOBAL", config.path());
    command.env("GIT_CONFIG_NOSYSTEM", "1");
    command
}
