# Security policy

## Supported versions

Security fixes are made on `main`. After releases begin, the latest release line
will also receive fixes where practical. Older snapshots are not supported.

## Reporting a vulnerability

Please use GitHub's
[private vulnerability reporting form](https://github.com/P4suta/OComment/security/advisories/new).
Do not open a public issue for a suspected vulnerability.

Include, when available:

- the affected command, library API, LSP operation, or plugin path;
- the OComment version or commit;
- operating system and relevant configuration;
- a minimal input or plugin artifact and reproduction steps;
- the expected impact and any known mitigations.

Redact repository contents, credentials, signing material, and personal data that
are not required to reproduce the issue. We aim to acknowledge reports within
five business days and will coordinate disclosure after a fix is available.

Security-sensitive areas include unsafe file replacement, Git index corruption,
scanner confusion that changes program meaning, path traversal, plugin sandbox
escape or resource-limit bypass, signature verification bypass, and LSP edits
outside the requested document.

The repository's advanced CodeQL workflow analyzes Rust, Python tooling, and
GitHub Actions with the `security-extended` query suite on pull requests, main
branch updates, and a weekly schedule.
