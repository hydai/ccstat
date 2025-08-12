#compdef ccstat
# Zsh completion for ccstat
# Install: copy to a directory in $fpath

_ccstat() {
    local -a commands
    commands=(
        'daily:Show daily usage summary'
        'monthly:Show monthly usage summary'
        'session:Show session-based usage'
        'blocks:Show 5-hour billing blocks'
        'help:Show help information'
    )

    local -a global_options
    global_options=(
        '--help[Show help information]'
        '--version[Show version information]'
    )

    local -a date_options
    date_options=(
        '--since=[Start date]:date:'
        '--until=[End date]:date:'
    )

    local -a common_options
    common_options=(
        '--project=[Filter by project name]:project:'
        '--json[Output in JSON format]'
        '--mode=[Cost calculation mode]:mode:(auto calculate display)'
    )

    case "$words[1]" in
        daily)
            _arguments \
                $date_options \
                $common_options \
                '--verbose[Show detailed token information]' \
                '--parallel[Enable parallel processing]' \
                '--intern[Use string interning]' \
                '--arena[Use arena allocation]' \
                '--by-instance[Group by instance ID]'
            ;;
        monthly)
            _arguments \
                '--since=[Start month]:month:' \
                '--until=[End month]:month:' \
                $common_options
            ;;
        session)
            _arguments \
                $date_options \
                $common_options \
                '--models[Show models used in sessions]'
            ;;
        blocks)
            _arguments \
                '--project=[Filter by project name]:project:' \
                '--json[Output in JSON format]' \
                '--active[Show only active blocks]' \
                '--recent[Show blocks from last 24 hours]' \
                '--limit=[Token limit for warnings]:limit:'
            ;;
        *)
            _arguments -C \
                $global_options \
                '1:command:->commands'

            case $state in
                commands)
                    _describe -t commands 'ccstat commands' commands
                    ;;
            esac
            ;;
    esac
}

_ccstat "$@"
