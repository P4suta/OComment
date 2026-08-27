# Changelog

All notable changes to the OComment VS Code extension are documented here. The
extension is versioned with the `ocomment` binary it launches, so its releases
follow the repository's tags.

## Unreleased

### Added

- First release. Launches `ocomment lsp` and attaches it to the thirty-five
  language identifiers OComment scans, including `objective-c`,
  `objective-cpp`, `cuda-cpp`, `javascriptreact`, `typescriptreact`, and
  `shellscript`.
- Removable comments as hints, with quick fixes, `source.fixAll.ocomment`, a
  per-document code lens, and pull diagnostics on change and on save; workspace
  diagnostics come from the server.
- `OComment: Remove comments in file`, `OComment: Remove comments in
  workspace`, `OComment: Restart server`, and `OComment: Show output`.
- A status bar entry counting the removable comments in the open files.
- `ocomment.enable`, `ocomment.path`, `ocomment.extraArgs`,
  `ocomment.languages`, and `ocomment.trace.server`. The server is restarted
  when any of them changes.
- `.ocomment.toml` and `.ocomment.lock` are watched, so a configuration change
  is picked up without a restart.
- A notification pointing at the install instructions when no `ocomment`
  executable can be found.
