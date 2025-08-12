# Bash completion for ccstat
# Install: source this file or copy to /etc/bash_completion.d/

_ccstat() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Main commands
    local commands="daily monthly session blocks help"

    # Global options
    local global_opts="--help --version"

    # Command-specific options
    local daily_opts="--since --until --project --json --mode --verbose --parallel --intern --arena --by-instance"
    local monthly_opts="--since --until --project --json --mode"
    local session_opts="--since --until --project --json --mode --models"
    local blocks_opts="--project --json --active --recent --limit"

    # Cost modes
    local cost_modes="auto calculate display"

    # Complete main command
    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${commands}" -- ${cur}) )
        return 0
    fi

    # Complete options based on command
    case "${COMP_WORDS[1]}" in
        daily)
            case "${prev}" in
                --mode)
                    COMPREPLY=( $(compgen -W "${cost_modes}" -- ${cur}) )
                    ;;
                --since|--until)
                    # Suggest date format
                    COMPREPLY=( $(compgen -W "$(date +%Y-%m-%d)" -- ${cur}) )
                    ;;
                *)
                    COMPREPLY=( $(compgen -W "${daily_opts}" -- ${cur}) )
                    ;;
            esac
            ;;
        monthly)
            case "${prev}" in
                --mode)
                    COMPREPLY=( $(compgen -W "${cost_modes}" -- ${cur}) )
                    ;;
                --since|--until)
                    # Suggest month format
                    COMPREPLY=( $(compgen -W "$(date +%Y-%m)" -- ${cur}) )
                    ;;
                *)
                    COMPREPLY=( $(compgen -W "${monthly_opts}" -- ${cur}) )
                    ;;
            esac
            ;;
        session)
            case "${prev}" in
                --mode)
                    COMPREPLY=( $(compgen -W "${cost_modes}" -- ${cur}) )
                    ;;
                --since|--until)
                    COMPREPLY=( $(compgen -W "$(date +%Y-%m-%d)" -- ${cur}) )
                    ;;
                *)
                    COMPREPLY=( $(compgen -W "${session_opts}" -- ${cur}) )
                    ;;
            esac
            ;;
        blocks)
            COMPREPLY=( $(compgen -W "${blocks_opts}" -- ${cur}) )
            ;;
        *)
            COMPREPLY=( $(compgen -W "${global_opts}" -- ${cur}) )
            ;;
    esac
}

complete -F _ccstat ccstat
