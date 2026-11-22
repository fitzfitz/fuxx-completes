# fuxx-autosuggestions for fish: fish has its own autosuggestions and no hook to supply ours, so
# no ghost text here (design §6 fish). What this file does: report each command fish keeps to
# the shared context file with `fuxx-completes report`, so your other shells (and a host such as
# fuxx) rank with it. Put it in ~/.config/fish/conf.d/ or `source` it from config.fish. Needs fish
# 3.0+. Design: docs/design.md §5.5, §5.7, §5.8, §6.
#
# Rules this file keeps: it prints nothing; the prompt never waits on a report (it runs in the
# background and is disowned, so fish prints no job lines); commands fish was told not to keep
# (a leading space, private mode, `fish_history` set empty, a user's fish_should_add_to_history
# saying no on fish 4.0+) are never reported, and nothing is reported in Warp.
#
# Config, read at use and never required:
#   FUXX_COMPLETES_BIN    the core binary (default: `fuxx-completes` on PATH)
#   FUXX_PLUGIN_DATA      our data folder (design §8), copied while this file is sourced

status is-interactive; or return 0

# Only one copy runs (design §8): a copy sourced from another path is noted and stops here.
set -l _fuxx_as_this (builtin realpath (status filename) 2>/dev/null; or status filename)
if set -q _fuxx_as_file; and test "$_fuxx_as_file" != "$_fuxx_as_this"
    contains -- $_fuxx_as_this $_fuxx_as_second; or set -g _fuxx_as_second $_fuxx_as_second $_fuxx_as_this
    return 0
end
if not set -q _fuxx_as_file
    set -g _fuxx_as_file $_fuxx_as_this
    set -g _fuxx_as_second
    set -g _fuxx_as_data "$FUXX_PLUGIN_DATA"
    set -g _fuxx_as_cwd $PWD
end

# The core binary, or nothing.
function _fuxx_as_bin
    if test -n "$FUXX_COMPLETES_BIN"
        echo $FUXX_COMPLETES_BIN
    else
        command -s fuxx-completes 2>/dev/null
    end
end

# fish's history file for this session: `fish_history` names it (`default` is `fish`).
function _fuxx_as_histfile
    set -l name fish
    if set -q fish_history; and test -n "$fish_history"; and test "$fish_history" != default
        set name $fish_history
    end
    set -l data $XDG_DATA_HOME
    test -n "$data"; or set data $HOME/.local/share
    echo $data/fish/{$name}_history
end

# Why reporting is off now (nothing when it is on): Warp, private mode, history off, no binary.
function _fuxx_as_reporting_off
    if test "$TERM_PROGRAM" = WarpTerminal
        echo warp
    else if set -q fish_private_mode; and test -n "$fish_private_mode"
        echo private-mode
    else if set -q fish_history; and test -z "$fish_history"
        echo history-off
    else if test -z (_fuxx_as_bin)
        echo no-binary
    end
end

# Where the command started (design §5.8: the folder at the prompt, not after a `cd`).
function _fuxx_as_preexec --on-event fish_preexec
    set -g _fuxx_as_cwd $PWD
end

# Reports a command fish keeps: `$argv[1]` is the line, `$status` its exit status.
function _fuxx_as_postexec --on-event fish_postexec
    set -l st $status
    set -l line $argv[1]
    test -n "$line"; or return 0
    test -z (_fuxx_as_reporting_off); or return 0
    # fish 4.0+: a user's fish_should_add_to_history decides alone; else a leading space hides
    # the line, as fish itself does.
    if functions -q fish_should_add_to_history
        fish_should_add_to_history $line; or return 0
    else if string match -q -- ' *' $line
        return 0
    end
    test (string length -- $line) -le 1000; or return 0
    set -l bin (_fuxx_as_bin)
    set -l data FUXX_PLUGIN_DATA=$_fuxx_as_data
    test -n "$_fuxx_as_data"; or set data -u FUXX_PLUGIN_DATA
    # In the background and disowned: the prompt never waits, and fish prints no job line.
    command env $data $bin report --cwd $_fuxx_as_cwd --status $st --histfile (_fuxx_as_histfile) -- $line >/dev/null 2>&1 &
    disown $last_pid 2>/dev/null
    return 0
end

# `fuxx-completes doctor` and friends typed at this prompt (design §7): the binary runs with this
# file's copy of FUXX_PLUGIN_DATA, `doctor` gets fish's facts. Defined only when no function of
# that name exists.
if not functions -q fuxx-completes
    function fuxx-completes --description 'fuxx-completes, with this fish shell'"'"'s facts'
        set -l bin (_fuxx_as_bin)
        if test -z "$bin"
            echo 'fuxx-completes: not found on PATH' >&2
            return 127
        end
        set -l data FUXX_PLUGIN_DATA=$_fuxx_as_data
        test -n "$_fuxx_as_data"; or set data -u FUXX_PLUGIN_DATA
        if test "$argv[1]" = doctor
            set -l off (_fuxx_as_reporting_off)
            set -l reporting on
            test -z "$off"; or set reporting off:$off
            set -l facts shell=fish version=$version state=off why=fish-has-its-own \
                reporting=$reporting file=$_fuxx_as_file data=$_fuxx_as_data \
                histfile=(_fuxx_as_histfile)
            for s in $_fuxx_as_second
                set facts $facts second=$s
            end
            set -e argv[1]   # (`$argv[2..-1]` of a one-item list is that item in fish 3)
            # One argument, newlines kept: an empty IFS stops the substitution from splitting
            # (fish 3.0 has no `string collect`).
            set -l IFS ''
            set -l lines (string join \n -- $facts)
            command env $data $bin doctor --shell-facts $lines $argv
        else
            command env $data $bin $argv
        end
    end
end
