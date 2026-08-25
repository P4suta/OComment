# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_ocomment_global_optspecs
    string join \n config= format= policy= layout= language= dialect= keep-kind= remove-kind= force-invalid force-protected color= hyperlinks= progress= h/help V/version
end

function __fish_ocomment_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_ocomment_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_ocomment_using_subcommand
    set -l cmd (__fish_ocomment_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c ocomment -n "__fish_ocomment_needs_command" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_needs_command" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_needs_command" -l policy -r
complete -c ocomment -n "__fish_ocomment_needs_command" -l layout -r
complete -c ocomment -n "__fish_ocomment_needs_command" -l language -r
complete -c ocomment -n "__fish_ocomment_needs_command" -l dialect -r
complete -c ocomment -n "__fish_ocomment_needs_command" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_needs_command" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_needs_command" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_needs_command" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_needs_command" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_needs_command" -l force-invalid
complete -c ocomment -n "__fish_ocomment_needs_command" -l force-protected
complete -c ocomment -n "__fish_ocomment_needs_command" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_needs_command" -s V -l version -d 'Print version'
complete -c ocomment -n "__fish_ocomment_needs_command" -a "check"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "fix"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "diff"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "scan"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "strip"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "lsp"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "init"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "config"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "languages"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "plugin"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "completions"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "doctor"
complete -c ocomment -n "__fish_ocomment_needs_command" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l staged -d 'Read and update Git index blobs rather than treating the working tree as the source'
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l index-only -d 'With `--staged`, do not attempt a uniquely mappable working-tree update'
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand check" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l staged -d 'Read and update Git index blobs rather than treating the working tree as the source'
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l index-only -d 'With `--staged`, do not attempt a uniquely mappable working-tree update'
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand fix" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l staged -d 'Read and update Git index blobs rather than treating the working tree as the source'
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l index-only -d 'With `--staged`, do not attempt a uniquely mappable working-tree update'
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand diff" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l staged -d 'Read and update Git index blobs rather than treating the working tree as the source'
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l index-only -d 'With `--staged`, do not attempt a uniquely mappable working-tree update'
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand scan" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand strip" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand lsp" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l fix
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand init" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand config" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand languages" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "add"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "remove"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "list"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "update"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "verify"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "new"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and not __fish_seen_subcommand_from add remove list update verify new help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l name -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l sha256 -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l identity -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from update" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from verify" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from new" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "add"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "remove"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "list"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "update"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "verify"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "new"
complete -c ocomment -n "__fish_ocomment_using_subcommand plugin; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand completions" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l config -d 'Explicit configuration file' -r -F
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l format -d 'Output encoding' -r -f -a "human\t''
json\t''
jsonl\t''
sarif\t''
github\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l policy -r
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l layout -r
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l language -r
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l dialect -r
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l keep-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l remove-kind -r
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l color -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l hyperlinks -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l progress -r -f -a "auto\t''
always\t''
never\t''"
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l force-invalid
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -l force-protected
complete -c ocomment -n "__fish_ocomment_using_subcommand doctor" -s h -l help -d 'Print help'
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "check"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "fix"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "diff"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "scan"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "strip"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "lsp"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "init"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "config"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "languages"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "plugin"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "completions"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "doctor"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and not __fish_seen_subcommand_from check fix diff scan strip lsp init config languages plugin completions doctor help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and __fish_seen_subcommand_from plugin" -f -a "add"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and __fish_seen_subcommand_from plugin" -f -a "remove"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and __fish_seen_subcommand_from plugin" -f -a "list"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and __fish_seen_subcommand_from plugin" -f -a "update"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and __fish_seen_subcommand_from plugin" -f -a "verify"
complete -c ocomment -n "__fish_ocomment_using_subcommand help; and __fish_seen_subcommand_from plugin" -f -a "new"
