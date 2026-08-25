
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
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand check 'check'
            cand fix 'fix'
            cand diff 'diff'
            cand scan 'scan'
            cand strip 'strip'
            cand lsp 'lsp'
            cand init 'init'
            cand config 'config'
            cand languages 'languages'
            cand plugin 'plugin'
            cand completions 'completions'
            cand doctor 'doctor'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'ocomment;check'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;fix'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;diff'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;scan'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --staged 'Read and update Git index blobs rather than treating the working tree as the source'
            cand --index-only 'With `--staged`, do not attempt a uniquely mappable working-tree update'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;strip'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;lsp'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;init'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --fix 'fix'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;config'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;languages'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
            cand add 'add'
            cand remove 'remove'
            cand list 'list'
            cand update 'update'
            cand verify 'verify'
            cand new 'new'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'ocomment;plugin;add'= {
            cand --name 'name'
            cand --sha256 'sha256'
            cand --identity 'identity'
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin;remove'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin;list'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin;update'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin;verify'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin;new'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;plugin;help'= {
            cand add 'add'
            cand remove 'remove'
            cand list 'list'
            cand update 'update'
            cand verify 'verify'
            cand new 'new'
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
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;doctor'= {
            cand --config 'Explicit configuration file'
            cand --format 'Output encoding'
            cand --policy 'policy'
            cand --layout 'layout'
            cand --language 'language'
            cand --dialect 'dialect'
            cand --keep-kind 'keep-kind'
            cand --remove-kind 'remove-kind'
            cand --color 'color'
            cand --hyperlinks 'hyperlinks'
            cand --progress 'progress'
            cand --force-invalid 'force-invalid'
            cand --force-protected 'force-protected'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'ocomment;help'= {
            cand check 'check'
            cand fix 'fix'
            cand diff 'diff'
            cand scan 'scan'
            cand strip 'strip'
            cand lsp 'lsp'
            cand init 'init'
            cand config 'config'
            cand languages 'languages'
            cand plugin 'plugin'
            cand completions 'completions'
            cand doctor 'doctor'
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
            cand add 'add'
            cand remove 'remove'
            cand list 'list'
            cand update 'update'
            cand verify 'verify'
            cand new 'new'
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
        &'ocomment;help;help'= {
        }
    ]
    $completions[$command]
}
