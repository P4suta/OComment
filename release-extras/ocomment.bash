_ocomment() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="ocomment"
                ;;
            ocomment,check)
                cmd="ocomment__subcmd__check"
                ;;
            ocomment,completions)
                cmd="ocomment__subcmd__completions"
                ;;
            ocomment,config)
                cmd="ocomment__subcmd__config"
                ;;
            ocomment,diff)
                cmd="ocomment__subcmd__diff"
                ;;
            ocomment,doctor)
                cmd="ocomment__subcmd__doctor"
                ;;
            ocomment,fix)
                cmd="ocomment__subcmd__fix"
                ;;
            ocomment,help)
                cmd="ocomment__subcmd__help"
                ;;
            ocomment,init)
                cmd="ocomment__subcmd__init"
                ;;
            ocomment,languages)
                cmd="ocomment__subcmd__languages"
                ;;
            ocomment,lsp)
                cmd="ocomment__subcmd__lsp"
                ;;
            ocomment,man)
                cmd="ocomment__subcmd__man"
                ;;
            ocomment,plugin)
                cmd="ocomment__subcmd__plugin"
                ;;
            ocomment,scan)
                cmd="ocomment__subcmd__scan"
                ;;
            ocomment,strip)
                cmd="ocomment__subcmd__strip"
                ;;
            ocomment__subcmd__help,check)
                cmd="ocomment__subcmd__help__subcmd__check"
                ;;
            ocomment__subcmd__help,completions)
                cmd="ocomment__subcmd__help__subcmd__completions"
                ;;
            ocomment__subcmd__help,config)
                cmd="ocomment__subcmd__help__subcmd__config"
                ;;
            ocomment__subcmd__help,diff)
                cmd="ocomment__subcmd__help__subcmd__diff"
                ;;
            ocomment__subcmd__help,doctor)
                cmd="ocomment__subcmd__help__subcmd__doctor"
                ;;
            ocomment__subcmd__help,fix)
                cmd="ocomment__subcmd__help__subcmd__fix"
                ;;
            ocomment__subcmd__help,help)
                cmd="ocomment__subcmd__help__subcmd__help"
                ;;
            ocomment__subcmd__help,init)
                cmd="ocomment__subcmd__help__subcmd__init"
                ;;
            ocomment__subcmd__help,languages)
                cmd="ocomment__subcmd__help__subcmd__languages"
                ;;
            ocomment__subcmd__help,lsp)
                cmd="ocomment__subcmd__help__subcmd__lsp"
                ;;
            ocomment__subcmd__help,man)
                cmd="ocomment__subcmd__help__subcmd__man"
                ;;
            ocomment__subcmd__help,plugin)
                cmd="ocomment__subcmd__help__subcmd__plugin"
                ;;
            ocomment__subcmd__help,scan)
                cmd="ocomment__subcmd__help__subcmd__scan"
                ;;
            ocomment__subcmd__help,strip)
                cmd="ocomment__subcmd__help__subcmd__strip"
                ;;
            ocomment__subcmd__help__subcmd__plugin,add)
                cmd="ocomment__subcmd__help__subcmd__plugin__subcmd__add"
                ;;
            ocomment__subcmd__help__subcmd__plugin,list)
                cmd="ocomment__subcmd__help__subcmd__plugin__subcmd__list"
                ;;
            ocomment__subcmd__help__subcmd__plugin,new)
                cmd="ocomment__subcmd__help__subcmd__plugin__subcmd__new"
                ;;
            ocomment__subcmd__help__subcmd__plugin,remove)
                cmd="ocomment__subcmd__help__subcmd__plugin__subcmd__remove"
                ;;
            ocomment__subcmd__help__subcmd__plugin,update)
                cmd="ocomment__subcmd__help__subcmd__plugin__subcmd__update"
                ;;
            ocomment__subcmd__help__subcmd__plugin,verify)
                cmd="ocomment__subcmd__help__subcmd__plugin__subcmd__verify"
                ;;
            ocomment__subcmd__plugin,add)
                cmd="ocomment__subcmd__plugin__subcmd__add"
                ;;
            ocomment__subcmd__plugin,help)
                cmd="ocomment__subcmd__plugin__subcmd__help"
                ;;
            ocomment__subcmd__plugin,list)
                cmd="ocomment__subcmd__plugin__subcmd__list"
                ;;
            ocomment__subcmd__plugin,new)
                cmd="ocomment__subcmd__plugin__subcmd__new"
                ;;
            ocomment__subcmd__plugin,remove)
                cmd="ocomment__subcmd__plugin__subcmd__remove"
                ;;
            ocomment__subcmd__plugin,update)
                cmd="ocomment__subcmd__plugin__subcmd__update"
                ;;
            ocomment__subcmd__plugin,verify)
                cmd="ocomment__subcmd__plugin__subcmd__verify"
                ;;
            ocomment__subcmd__plugin__subcmd__help,add)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__add"
                ;;
            ocomment__subcmd__plugin__subcmd__help,help)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__help"
                ;;
            ocomment__subcmd__plugin__subcmd__help,list)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__list"
                ;;
            ocomment__subcmd__plugin__subcmd__help,new)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__new"
                ;;
            ocomment__subcmd__plugin__subcmd__help,remove)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__remove"
                ;;
            ocomment__subcmd__plugin__subcmd__help,update)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__update"
                ;;
            ocomment__subcmd__plugin__subcmd__help,verify)
                cmd="ocomment__subcmd__plugin__subcmd__help__subcmd__verify"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        ocomment)
            opts="-q -v -h -V --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help --version check fix diff scan strip lsp init config languages plugin completions doctor man help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__check)
            opts="-q -v -h --staged --index-only --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__completions)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help bash elvish fish powershell zsh"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__config)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help show locate explain schema"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__diff)
            opts="-q -v -h --staged --index-only --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__doctor)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__fix)
            opts="-q -v -h --staged --index-only --dry-run --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help)
            opts="check fix diff scan strip lsp init config languages plugin completions doctor man help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__check)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__completions)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__config)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__diff)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__doctor)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__fix)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__languages)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__lsp)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__man)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin)
            opts="add remove list update verify new"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin__subcmd__new)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin__subcmd__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__plugin__subcmd__verify)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__scan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__help__subcmd__strip)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__init)
            opts="-q -v -h --fix --force --stdout --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help config lefthook"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__languages)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__lsp)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__man)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help add remove list update verify new help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__add)
            opts="-q -v -h --name --sha256 --identity --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --sha256)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --identity)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help)
            opts="add remove list update verify new help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__new)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__help__subcmd__verify)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__list)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__new)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__remove)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__update)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__plugin__subcmd__verify)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__scan)
            opts="-q -v -h --staged --index-only --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        ocomment__subcmd__strip)
            opts="-q -v -h --config --policy --layout --language --dialect --keep-kind --remove-kind --force-invalid --force-protected --format --color --hyperlinks --no-preview --progress --quiet --verbose --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --policy)
                    COMPREPLY=($(compgen -W "safe legal all" -- "${cur}"))
                    return 0
                    ;;
                --layout)
                    COMPREPLY=($(compgen -W "lines columns compact" -- "${cur}"))
                    return 0
                    ;;
                --language)
                    COMPREPLY=($(compgen -W "rust ocaml c cpp go java javascript typescript python shell html css jsonc sql kotlin" -- "${cur}"))
                    return 0
                    ;;
                --dialect)
                    COMPREPLY=($(compgen -W "standard jsx tsx objective-c objective-cpp gnu-c gnu-cpp cuda posix-sh bash53 zsh postgresql mysql sqlite t-sql oracle" -- "${cur}"))
                    return 0
                    ;;
                --keep-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --remove-kind)
                    COMPREPLY=($(compgen -W "line block doc-line doc-block directive license html-comment shebang encoding optimizer-hint version-comment" -- "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json jsonl sarif github" -- "${cur}"))
                    return 0
                    ;;
                --color)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --hyperlinks)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                --progress)
                    COMPREPLY=($(compgen -W "auto always never" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _ocomment -o nosort -o bashdefault -o default ocomment
else
    complete -F _ocomment -o bashdefault -o default ocomment
fi
