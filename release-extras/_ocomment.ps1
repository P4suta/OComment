
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'ocomment' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'ocomment'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'ocomment' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Report removable comments (default command)')
            [CompletionResult]::new('fix', 'fix', [CompletionResultType]::ParameterValue, 'Remove comments in place through an atomic, rollback-backed transaction')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Print a unified diff of the changes fix would make')
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'List every comment with its kind, disposition and byte span')
            [CompletionResult]::new('strip', 'strip', [CompletionResultType]::ParameterValue, 'Read source on stdin and write the stripped result to stdout')
            [CompletionResult]::new('lsp', 'lsp', [CompletionResultType]::ParameterValue, 'Run the LSP 3.18 server over stdio')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Write a starter .ocomment.toml or Lefthook configuration')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Show, locate, explain, or export the resolved configuration')
            [CompletionResult]::new('languages', 'languages', [CompletionResultType]::ParameterValue, 'List built-in languages, extensions, and dialects')
            [CompletionResult]::new('plugin', 'plugin', [CompletionResultType]::ParameterValue, 'Manage sandboxed WASM scanner plugins')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Generate shell completions')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Diagnose the environment (config, git, plugins, tools)')
            [CompletionResult]::new('man', 'man', [CompletionResultType]::ParameterValue, 'Render the roff manual page to stdout')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'ocomment;check' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;fix' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Print the patch `fix` would apply and write nothing')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;diff' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;scan' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;strip' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;lsp' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;init' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--fix', '--fix', [CompletionResultType]::ParameterName, 'For the Lefthook hook, run `fix` instead of `check`')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Replace the file if it already exists')
            [CompletionResult]::new('--stdout', '--stdout', [CompletionResultType]::ParameterName, 'Print the template to standard output and write no file')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;config' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;languages' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a plugin and pin its digest in .ocomment.lock')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Uninstall a plugin and drop its lock entry')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the installed plugins and their pinned digests')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Re-fetch plugins and refresh their pinned digests')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Check installed plugins against their pinned digests')
            [CompletionResult]::new('new', 'new', [CompletionResultType]::ParameterValue, 'Scaffold a new plugin crate from the scanner WIT world')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'ocomment;plugin;add' {
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Name to register the plugin under (default: the file stem)')
            [CompletionResult]::new('--sha256', '--sha256', [CompletionResultType]::ParameterName, 'Expected SHA-256 digest of the component, verified before install')
            [CompletionResult]::new('--identity', '--identity', [CompletionResultType]::ParameterName, 'Publisher identity recorded alongside the pinned digest')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin;remove' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin;list' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin;update' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin;verify' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin;new' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;plugin;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a plugin and pin its digest in .ocomment.lock')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Uninstall a plugin and drop its lock entry')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the installed plugins and their pinned digests')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Re-fetch plugins and refresh their pinned digests')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Check installed plugins against their pinned digests')
            [CompletionResult]::new('new', 'new', [CompletionResultType]::ParameterValue, 'Scaffold a new plugin crate from the scanner WIT world')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'ocomment;plugin;help;add' {
            break
        }
        'ocomment;plugin;help;remove' {
            break
        }
        'ocomment;plugin;help;list' {
            break
        }
        'ocomment;plugin;help;update' {
            break
        }
        'ocomment;plugin;help;verify' {
            break
        }
        'ocomment;plugin;help;new' {
            break
        }
        'ocomment;plugin;help;help' {
            break
        }
        'ocomment;completions' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;doctor' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;man' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Read this configuration file instead of discovering `.ocomment.toml`')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'Which classes of comment the run is allowed to remove')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'How the bytes left behind by a removed comment are laid out')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Force this language instead of detecting it from path and contents')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'Force this dialect of the selected language')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to protect on top of the policy')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'Comma-separated comment kinds to remove regardless of the policy')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'When to colour terminal output')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'When to emit terminal hyperlinks for reported paths')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'When to draw the live scanning counter on standard error')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'Apply the edits that are still provably safe when the source fails to scan')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'Remove protected comments such as shebang and encoding preambles')
            [CompletionResult]::new('--no-preview', '--no-preview', [CompletionResultType]::ParameterName, 'Omit the one-line comment text from human `check` and `scan` lines')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Drop the run summary and notes; the command''s product (findings, patch, listing) is still written')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Trace what is scanned and summarize every comment kind and skipped file')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'ocomment;help' {
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Report removable comments (default command)')
            [CompletionResult]::new('fix', 'fix', [CompletionResultType]::ParameterValue, 'Remove comments in place through an atomic, rollback-backed transaction')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Print a unified diff of the changes fix would make')
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'List every comment with its kind, disposition and byte span')
            [CompletionResult]::new('strip', 'strip', [CompletionResultType]::ParameterValue, 'Read source on stdin and write the stripped result to stdout')
            [CompletionResult]::new('lsp', 'lsp', [CompletionResultType]::ParameterValue, 'Run the LSP 3.18 server over stdio')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Write a starter .ocomment.toml or Lefthook configuration')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Show, locate, explain, or export the resolved configuration')
            [CompletionResult]::new('languages', 'languages', [CompletionResultType]::ParameterValue, 'List built-in languages, extensions, and dialects')
            [CompletionResult]::new('plugin', 'plugin', [CompletionResultType]::ParameterValue, 'Manage sandboxed WASM scanner plugins')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Generate shell completions')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Diagnose the environment (config, git, plugins, tools)')
            [CompletionResult]::new('man', 'man', [CompletionResultType]::ParameterValue, 'Render the roff manual page to stdout')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'ocomment;help;check' {
            break
        }
        'ocomment;help;fix' {
            break
        }
        'ocomment;help;diff' {
            break
        }
        'ocomment;help;scan' {
            break
        }
        'ocomment;help;strip' {
            break
        }
        'ocomment;help;lsp' {
            break
        }
        'ocomment;help;init' {
            break
        }
        'ocomment;help;config' {
            break
        }
        'ocomment;help;languages' {
            break
        }
        'ocomment;help;plugin' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a plugin and pin its digest in .ocomment.lock')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Uninstall a plugin and drop its lock entry')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List the installed plugins and their pinned digests')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Re-fetch plugins and refresh their pinned digests')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Check installed plugins against their pinned digests')
            [CompletionResult]::new('new', 'new', [CompletionResultType]::ParameterValue, 'Scaffold a new plugin crate from the scanner WIT world')
            break
        }
        'ocomment;help;plugin;add' {
            break
        }
        'ocomment;help;plugin;remove' {
            break
        }
        'ocomment;help;plugin;list' {
            break
        }
        'ocomment;help;plugin;update' {
            break
        }
        'ocomment;help;plugin;verify' {
            break
        }
        'ocomment;help;plugin;new' {
            break
        }
        'ocomment;help;completions' {
            break
        }
        'ocomment;help;doctor' {
            break
        }
        'ocomment;help;man' {
            break
        }
        'ocomment;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
