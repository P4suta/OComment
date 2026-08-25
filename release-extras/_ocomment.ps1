
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
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'check')
            [CompletionResult]::new('fix', 'fix', [CompletionResultType]::ParameterValue, 'fix')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'diff')
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'scan')
            [CompletionResult]::new('strip', 'strip', [CompletionResultType]::ParameterValue, 'strip')
            [CompletionResult]::new('lsp', 'lsp', [CompletionResultType]::ParameterValue, 'lsp')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'init')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'config')
            [CompletionResult]::new('languages', 'languages', [CompletionResultType]::ParameterValue, 'languages')
            [CompletionResult]::new('plugin', 'plugin', [CompletionResultType]::ParameterValue, 'plugin')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'completions')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'doctor')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'ocomment;check' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;fix' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;diff' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;scan' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--staged', '--staged', [CompletionResultType]::ParameterName, 'Read and update Git index blobs rather than treating the working tree as the source')
            [CompletionResult]::new('--index-only', '--index-only', [CompletionResultType]::ParameterName, 'With `--staged`, do not attempt a uniquely mappable working-tree update')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;strip' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;lsp' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;init' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--fix', '--fix', [CompletionResultType]::ParameterName, 'fix')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;config' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;languages' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'add')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'remove')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'list')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'update')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'verify')
            [CompletionResult]::new('new', 'new', [CompletionResultType]::ParameterValue, 'new')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'ocomment;plugin;add' {
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'name')
            [CompletionResult]::new('--sha256', '--sha256', [CompletionResultType]::ParameterName, 'sha256')
            [CompletionResult]::new('--identity', '--identity', [CompletionResultType]::ParameterName, 'identity')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin;remove' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin;list' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin;update' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin;verify' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin;new' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;plugin;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'add')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'remove')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'list')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'update')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'verify')
            [CompletionResult]::new('new', 'new', [CompletionResultType]::ParameterValue, 'new')
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
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;doctor' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit configuration file')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output encoding')
            [CompletionResult]::new('--policy', '--policy', [CompletionResultType]::ParameterName, 'policy')
            [CompletionResult]::new('--layout', '--layout', [CompletionResultType]::ParameterName, 'layout')
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'language')
            [CompletionResult]::new('--dialect', '--dialect', [CompletionResultType]::ParameterName, 'dialect')
            [CompletionResult]::new('--keep-kind', '--keep-kind', [CompletionResultType]::ParameterName, 'keep-kind')
            [CompletionResult]::new('--remove-kind', '--remove-kind', [CompletionResultType]::ParameterName, 'remove-kind')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'color')
            [CompletionResult]::new('--hyperlinks', '--hyperlinks', [CompletionResultType]::ParameterName, 'hyperlinks')
            [CompletionResult]::new('--progress', '--progress', [CompletionResultType]::ParameterName, 'progress')
            [CompletionResult]::new('--force-invalid', '--force-invalid', [CompletionResultType]::ParameterName, 'force-invalid')
            [CompletionResult]::new('--force-protected', '--force-protected', [CompletionResultType]::ParameterName, 'force-protected')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'ocomment;help' {
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'check')
            [CompletionResult]::new('fix', 'fix', [CompletionResultType]::ParameterValue, 'fix')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'diff')
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'scan')
            [CompletionResult]::new('strip', 'strip', [CompletionResultType]::ParameterValue, 'strip')
            [CompletionResult]::new('lsp', 'lsp', [CompletionResultType]::ParameterValue, 'lsp')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'init')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'config')
            [CompletionResult]::new('languages', 'languages', [CompletionResultType]::ParameterValue, 'languages')
            [CompletionResult]::new('plugin', 'plugin', [CompletionResultType]::ParameterValue, 'plugin')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'completions')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'doctor')
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
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'add')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'remove')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'list')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'update')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'verify')
            [CompletionResult]::new('new', 'new', [CompletionResultType]::ParameterValue, 'new')
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
        'ocomment;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
