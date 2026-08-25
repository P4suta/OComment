
use builtin;
use str;

set edit:completion:arg-completer[ocomment] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'ocomment'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'ocomment'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
            cand check 'Report removable comments (default command)'
            cand fix 'Remove comments in place through an atomic, rollback-backed transaction'
            cand diff 'Print a unified diff of the changes fix would make'
            cand scan 'List every comment with its kind, disposition and byte span'
            cand strip 'Read source on stdin and write the stripped result to stdout'
            cand lsp 'Run the LSP 3.18 server over stdio'
            cand init 'Write a starter .ocomment.toml or Lefthook configuration'
            cand config 'Show, locate, explain, or export the resolved configuration'
            cand languages 'List built-in languages, extensions, and dialects'
            cand plugin 'Manage sandboxed WASM scanner plugins'
            cand completions 'Generate shell completions'
            cand doctor 'Diagnose the environment (config, git, plugins, tools)'
            cand man 'Render the roff manual page to stdout'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'ocomment;check'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;fix'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --dry-run 'Print the patch `fix` would apply and write nothing'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;diff'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;scan'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;strip'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;lsp'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;init'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --fix 'For the Lefthook hook, run `fix` instead of `check`'
            cand --force 'Replace the file if it already exists'
            cand --stdout 'Print the template to standard output and write no file'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;config'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;languages'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand add 'Install a plugin and pin its digest in .ocomment.lock'
            cand remove 'Uninstall a plugin and drop its lock entry'
            cand list 'List the installed plugins and their pinned digests'
            cand update 'Re-fetch plugins and refresh their pinned digests'
            cand verify 'Check installed plugins against their pinned digests'
            cand new 'Scaffold a new plugin crate from the scanner WIT world'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'ocomment;plugin;add'= {
            cand --name 'Name to register the plugin under (default: the file stem)'
            cand --sha256 'Expected SHA-256 digest of the component, verified before install'
            cand --identity 'Publisher identity recorded alongside the pinned digest'
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin;remove'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin;list'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin;update'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin;verify'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin;new'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;plugin;help'= {
            cand add 'Install a plugin and pin its digest in .ocomment.lock'
            cand remove 'Uninstall a plugin and drop its lock entry'
            cand list 'List the installed plugins and their pinned digests'
            cand update 'Re-fetch plugins and refresh their pinned digests'
            cand verify 'Check installed plugins against their pinned digests'
            cand new 'Scaffold a new plugin crate from the scanner WIT world'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'ocomment;plugin;help;add'= {
        }
        &'ocomment;plugin;help;remove'= {
        }
        &'ocomment;plugin;help;list'= {
        }
        &'ocomment;plugin;help;update'= {
        }
        &'ocomment;plugin;help;verify'= {
        }
        &'ocomment;plugin;help;new'= {
        }
        &'ocomment;plugin;help;help'= {
        }
        &'ocomment;completions'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;doctor'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;man'= {
            cand --config 'Read this configuration file instead of discovering `.ocomment.toml`'
            cand --policy 'Which classes of comment the run is allowed to remove'
            cand --layout 'How the bytes left behind by a removed comment are laid out'
            cand --language 'Force this language instead of detecting it from path and contents'
            cand --dialect 'Force this dialect of the selected language'
            cand --keep-kind 'Comma-separated comment kinds to protect on top of the policy'
            cand --remove-kind 'Comma-separated comment kinds to remove regardless of the policy'
            cand --format 'Output encoding'
            cand --color 'When to colour terminal output'
            cand --hyperlinks 'When to emit terminal hyperlinks for reported paths'
            cand --progress 'When to draw the live scanning counter on standard error'
            cand --force-invalid 'Apply the edits that are still provably safe when the source fails to scan'
            cand --force-protected 'Remove protected comments such as shebang and encoding preambles'
            cand --no-preview 'Omit the one-line comment text from human `check` and `scan` lines'
            cand -q 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand --quiet 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written'
            cand -v 'Trace what is scanned and summarize every comment kind and skipped file'
            cand --verbose 'Trace what is scanned and summarize every comment kind and skipped file'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'ocomment;help'= {
            cand check 'Report removable comments (default command)'
            cand fix 'Remove comments in place through an atomic, rollback-backed transaction'
            cand diff 'Print a unified diff of the changes fix would make'
            cand scan 'List every comment with its kind, disposition and byte span'
            cand strip 'Read source on stdin and write the stripped result to stdout'
            cand lsp 'Run the LSP 3.18 server over stdio'
            cand init 'Write a starter .ocomment.toml or Lefthook configuration'
            cand config 'Show, locate, explain, or export the resolved configuration'
            cand languages 'List built-in languages, extensions, and dialects'
            cand plugin 'Manage sandboxed WASM scanner plugins'
            cand completions 'Generate shell completions'
            cand doctor 'Diagnose the environment (config, git, plugins, tools)'
            cand man 'Render the roff manual page to stdout'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'ocomment;help;check'= {
        }
        &'ocomment;help;fix'= {
        }
        &'ocomment;help;diff'= {
        }
        &'ocomment;help;scan'= {
        }
        &'ocomment;help;strip'= {
        }
        &'ocomment;help;lsp'= {
        }
        &'ocomment;help;init'= {
        }
        &'ocomment;help;config'= {
        }
        &'ocomment;help;languages'= {
        }
        &'ocomment;help;plugin'= {
            cand add 'Install a plugin and pin its digest in .ocomment.lock'
            cand remove 'Uninstall a plugin and drop its lock entry'
            cand list 'List the installed plugins and their pinned digests'
            cand update 'Re-fetch plugins and refresh their pinned digests'
            cand verify 'Check installed plugins against their pinned digests'
            cand new 'Scaffold a new plugin crate from the scanner WIT world'
        }
        &'ocomment;help;plugin;add'= {
        }
        &'ocomment;help;plugin;remove'= {
        }
        &'ocomment;help;plugin;list'= {
        }
        &'ocomment;help;plugin;update'= {
        }
        &'ocomment;help;plugin;verify'= {
        }
        &'ocomment;help;plugin;new'= {
        }
        &'ocomment;help;completions'= {
        }
        &'ocomment;help;doctor'= {
        }
        &'ocomment;help;man'= {
        }
        &'ocomment;help;help'= {
        }
    ]
    $completions[$command]
}
