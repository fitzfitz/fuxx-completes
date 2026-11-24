# fuxx-autosuggestions for zsh: grey ghost text from history, drawn from a `fuxx-completes serve`
# core process. One self-contained file; it finds everything through PATH or variables, never
# through its own location. Needs zsh 5.3+. Design: docs/design.md §5.5, §5.6, §6.
#
# Rules this file keeps: it prints nothing (every command's stderr goes to /dev/null); a keystroke
# never waits on the core; every function starts with `emulate -L zsh` so the user's options
# (ksharrays, nounset, ...) can't change what it does; hooks go through add-zsh-hook and
# add-zle-hook-widget only.
#
# Config, read at use and never required:
#   FUXX_COMPLETES_BIN                      the core binary (default: `fuxx-completes` on PATH)
#   FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE    default fg=8
#   FUXX_AUTOSUGGESTIONS_FORCE=1            stay on beside zsh-autosuggestions (never in Warp)
#   FUXX_PLUGIN_DATA                        set by fuxx while it sources us: our data folder
#   FUXX_AUTOSUGGESTIONS_LIST               on/off: the suggestion list after a pause (default on)
#   FUXX_AUTOSUGGESTIONS_LIST_EMPTY         on/off: the list on an empty line too (default on)
#   FUXX_AUTOSUGGESTIONS_LIST_DELAY         seconds of pause before the list (default 3)
#   FUXX_AUTOSUGGESTIONS_LIST_SIZE          items in the list, 1–5 (default 5)
#   FUXX_PLUGIN_SETTING_LIST{,_EMPTY,_DELAY,_SIZE}  fuxx's Settings, set while it sources us;
#                                           they win over the shell variables (design spec M1c)
#
# Only one copy runs in a shell (design §8): a second copy sourced from another path (fuxx's
# plugin and a line in .zshrc) stops before defining anything. The first copy's path is passed
# in as $1 below.

# Load-time logic lives in an anonymous function: `emulate -L zsh` at the top of a sourced file
# would change the user's options for the whole shell. State is created only if it doesn't exist,
# so sourcing the file again (`source ~/.zshrc`) never resets a running core's state.
() {
  emulate -L zsh
  [[ -o interactive ]] || return 1
  autoload -Uz is-at-least add-zsh-hook add-zle-hook-widget 2>/dev/null
  is-at-least 5.3 2>/dev/null || return 1
  zmodload zsh/datetime zsh/parameter zsh/zleparameter 2>/dev/null || return 1
  zmodload -F zsh/files b:zf_rm b:zf_rmdir b:zf_mkdir 2>/dev/null || return 1
  if (( $+_FUXX_AS_FILE )) && [[ $_FUXX_AS_FILE != "$1" ]]; then
    (( ${_FUXX_AS_SECOND[(Ie)$1]} )) || _FUXX_AS_SECOND+=( "$1" )
    return 1
  fi
  # zsh's builtin completion widgets (complist's menu-select too); compinit makes them `completion:`.
  # Set on every load (it holds no state), so a newer copy sourced into a running shell has it.
  typeset -ga _FUXX_AS_COMPLETERS=( complete-word delete-char-or-list expand-or-complete
    expand-or-complete-prefix list-choices menu-complete menu-expand-or-complete
    reverse-menu-complete accept-and-menu-complete list-expand menu-select )
  (( $+_FUXX_AS_STATE )) && return 0
  # State. '' until the first prompt, then `on` or `off`; why it is off (design §7).
  typeset -g _FUXX_AS_STATE='' _FUXX_AS_WHY=''
  # Which copy runs, other copies that stayed off, and the data folder fuxx named while it
  # sourced us (it unsets the variable afterwards, design §8).
  typeset -g _FUXX_AS_FILE=$1 _FUXX_AS_DATA=${FUXX_PLUGIN_DATA-}
  typeset -ga _FUXX_AS_SECOND=()
  # "<keymap> <key>" -> `bound` or `kept:<widget>` for the keys we bind only when free.
  typeset -gA _FUXX_AS_KEYS
  typeset -gi _FUXX_AS_OWN_FN=0   # 1 once we defined the `fuxx-completes` function
  # The suggestion list (M1c): fuxx's settings as they were while it sourced us; the resolved
  # settings (size 0 = off) and where each came from; the list on screen; the key counter that
  # makes a late list stale; the request a list may belong to.
  typeset -g _FUXX_AS_PSET_LIST=${FUXX_PLUGIN_SETTING_LIST-} \
    _FUXX_AS_PSET_LIST_EMPTY=${FUXX_PLUGIN_SETTING_LIST_EMPTY-} \
    _FUXX_AS_PSET_LIST_DELAY=${FUXX_PLUGIN_SETTING_LIST_DELAY-} \
    _FUXX_AS_PSET_LIST_SIZE=${FUXX_PLUGIN_SETTING_LIST_SIZE-}
  typeset -gi _FUXX_AS_LIST_SIZE=0 _FUXX_AS_LIST_DELAY_MS=3000 _FUXX_AS_LIST_EMPTY=1
  typeset -gi _FUXX_AS_LIST_SEL=0 _FUXX_AS_KEYSEQ=0 _FUXX_AS_LIST_FOR_SEQ=-1
  typeset -g _FUXX_AS_LIST_FOR_ID='' _FUXX_AS_LIST_FOR_BUFFER='' _FUXX_AS_LIST_GHOST=''
  typeset -g _FUXX_AS_LIST_PREV=main   # the keymap the list's own keymap was copied from
  typeset -gi _FUXX_AS_HIDDEN=0   # 1 while the ghost text is off until the next edit (Tab, Esc)
  typeset -gi _FUXX_AS_FAILED=0   # 1 when the last line entered ended non-zero: ask on the empty line (M1d)
  typeset -gA _FUXX_AS_LIST_VAL _FUXX_AS_LIST_SRC
  typeset -ga _FUXX_AS_LIST_ITEMS=() _FUXX_AS_HLS=()
  typeset -g _FUXX_AS_PID=''
  # The host channel (design §5.5, M2): 1 in host mode (the terminal draws; no core here), why
  # it is off when asked for, the messages waiting for ZLE, the last `buffer` sent and its id.
  typeset -gi _FUXX_AS_HOST=0
  typeset -g _FUXX_AS_HOST_WHY='' _FUXX_AS_HOST_LAST='' _FUXX_AS_HOST_ID=''
  typeset -ga _FUXX_AS_HOST_Q=()
  typeset -g _FUXX_AS_WFD='' _FUXX_AS_RFD='' _FUXX_AS_KEEPFD=''
  typeset -g _FUXX_AS_SPARE_FD=''  # read-write holder on `resp` until `ready` (see _fuxx_as_start)
  typeset -g _FUXX_AS_NEXT_ID=1 _FUXX_AS_INFLIGHT_ID='' _FUXX_AS_INFLIGHT_SINCE=0
  typeset -g _FUXX_AS_INFLIGHT_BUFFER='' _FUXX_AS_LAST_BUFFER=''
  typeset -g _FUXX_AS_DIR=''     # the temp folder holding the pipes, until the core has them open
  typeset -g _FUXX_AS_MEMO=0     # 1 when region_highlight accepts `memo=` (zsh 5.9+)
  typeset -g _FUXX_AS_CLOEXEC=0  # 1 when `sysopen -o cloexec` works on this zsh
  typeset -g _FUXX_AS_NONBLOCK=0 # 1 when `sysopen -o nonblock` works (zsh 5.9+): writes never block
  # Without nonblock (zsh below 5.9) a write could block on a full pipe, so the bytes the core may
  # not have read yet are counted instead: sent minus what an answer proves it consumed.
  typeset -gi _FUXX_AS_UNREAD=0 _FUXX_AS_SINCE_SUGGEST=0
  typeset -g _FUXX_AS_CDABLE=off   # the user's CDABLE_VARS, read at each prompt
  # Command reports (§5.8): the line zshaddhistory saw, where the running command started, and the
  # one command held back until the next prompt (its event number, history text, cwd, status, time).
  typeset -g _FUXX_AS_NOTED='' _FUXX_AS_CMD_PWD=''
  typeset -g _FUXX_AS_PEND_EVENT='' _FUXX_AS_PEND_ENTRY='' _FUXX_AS_PEND_PWD=''
  typeset -g _FUXX_AS_PEND_STATUS='' _FUXX_AS_PEND_TIME=''
  # How many times each widget has been wrapped: each wrap saves the original under a fresh name.
  typeset -gA _FUXX_AS_WRAPS
  # Per widget, the definition (with its function's body) known to still call our wrapper (another
  # plugin wrapped us), so it isn't checked again at every prompt.
  typeset -gA _FUXX_AS_UNDER
  # "<widget> <definition>" -> the generation that wrapped that definition, used again when the
  # widget is set back to it (oh-my-zsh does `zle -N self-insert url-quote-magic` on every source).
  typeset -gA _FUXX_AS_GEN
  is-at-least 5.9 && _FUXX_AS_MEMO=1
  local fd
  # zsh/system is optional: without sysopen or its options we open pipes with plain `exec`.
  # Below 5.4 `sysopen -o cloexec -u fd` is accepted but ineffective (zsh moves the descriptor and
  # drops the flag; fixed 2017-05-23, 5.4): checked on 5.3.1, so there we use the plain path too.
  # `-o nonblock` exists from 5.9 only ("unsupported option" on 5.3.1 and 5.8.1); plain `exec`
  # has no way to ask for O_NONBLOCK, so older zsh counts bytes instead (see _fuxx_as_send).
  if zmodload zsh/system 2>/dev/null; then
    if is-at-least 5.4 && sysopen -r -o cloexec -u fd /dev/null 2>/dev/null; then
      { exec {fd}<&- } 2>/dev/null
      _FUXX_AS_CLOEXEC=1
    fi
    if sysopen -w -o nonblock -u fd /dev/null 2>/dev/null; then
      { exec {fd}>&- } 2>/dev/null
      _FUXX_AS_NONBLOCK=1
    fi
  fi
  return 0
} "${${(%):-%x}:A}" || return 0

_fuxx_as_unlink_pipes() {
  emulate -L zsh
  [[ -n $_FUXX_AS_DIR ]] || return 0
  zf_rm -f $_FUXX_AS_DIR/req $_FUXX_AS_DIR/resp 2>/dev/null
  zf_rmdir $_FUXX_AS_DIR 2>/dev/null
  _FUXX_AS_DIR=''
  return 0
}

# Drops the spare read-write holder on `resp` (kept from the start until the core says `ready`).
_fuxx_as_close_spare() {
  emulate -L zsh
  [[ -n $_FUXX_AS_SPARE_FD ]] && { exec {_FUXX_AS_SPARE_FD}>&- } 2>/dev/null
  _FUXX_AS_SPARE_FD=''
  return 0
}

# _fuxx_as_open <r|w|rw> <file>: opens a pipe, close-on-exec where zsh can (so the programs the
# user runs never inherit it), and the shell's write end also non-blocking where zsh can (a write
# to a full pipe then fails instead of freezing the shell); the descriptor number comes back in
# $REPLY.
_fuxx_as_open() {
  emulate -L zsh
  local mode=$1 file=$2 fd
  local -a o=()
  (( _FUXX_AS_CLOEXEC )) && o+=cloexec
  [[ $mode == w ]] && (( _FUXX_AS_NONBLOCK )) && o+=nonblock
  if (( $#o )); then
    sysopen -$mode -o ${(j:,:)o} -u fd $file 2>/dev/null || return 1
  else
    case $mode in
      rw) { exec {fd}<>$file } 2>/dev/null ;;
      w)  { exec {fd}>$file } 2>/dev/null ;;
      r)  { exec {fd}<$file } 2>/dev/null ;;
    esac || return 1
  fi
  REPLY=$fd
}

# Starts the core on two named pipes. Open order (checked on macOS): the shell opens both pipes
# read-write first, so no open blocks; the core is started on them; the shell then opens its
# write end of `req` and read end of `resp`. A dead core gives EOF on the read end, and a write
# never raises EPIPE because the shell keeps a reader on `req`. The read-write holder on `resp`
# stays until `ready` arrives: closing it earlier lets the shell see EOF if the core hasn't opened
# `resp` yet. The pipes are unlinked at `ready` too, not before: the core opens them by path in its
# own process, and unlinking earlier races that open (the core then fails to start).
_fuxx_as_start() {
  emulate -L zsh
  setopt nobgnice   # emulate -L turns BG_NICE on, which would run the core niced
  local bin=${FUXX_COMPLETES_BIN:-${commands[fuxx-completes]-}} dir rs='' ok=1
  # Host mode (design §5.5): the terminal asked for the buffer with FUXX_COMPLETES_HOST=1 and no
  # multiplexer sits between (tmux and screen drop or misroute unknown OSCs). No core is started
  # and nothing is drawn; `hello` waits for ZLE like every message.
  if [[ ${FUXX_COMPLETES_HOST-} == 1 ]]; then
    if [[ -n ${TMUX-} || -n ${STY-} ]]; then
      _FUXX_AS_HOST_WHY=tmux
    else
      _FUXX_AS_HOST=1
      _fuxx_as_send hello 1 zsh "$ZSH_VERSION" zsh "${HISTFILE:+${HISTFILE:a}}" autosuggestions
      _fuxx_as_wrap_widgets
      _fuxx_as_install_once
      _FUXX_AS_STATE=on
      return 0
    fi
  fi
  if [[ -z $bin || ! -x $bin || -d $bin ]]; then
    _FUXX_AS_STATE=off _FUXX_AS_WHY=no-binary
    return 0
  fi
  dir=${TMPDIR:-/tmp}/fuxx-completes.$$.$RANDOM
  zf_mkdir -m 700 $dir 2>/dev/null || { _FUXX_AS_STATE=off _FUXX_AS_WHY=start-failed; return 0 }
  _FUXX_AS_DIR=$dir
  if ! command mkfifo -m 600 $dir/req $dir/resp 2>/dev/null; then
    _fuxx_as_unlink_pipes
    _FUXX_AS_STATE=off _FUXX_AS_WHY=start-failed
    return 0
  fi
  # Any failure below falls through to the cleanup after the block (never `return` from inside).
  {
    _fuxx_as_open rw $dir/req && _FUXX_AS_KEEPFD=$REPLY || ok=0
    if (( ok )); then _fuxx_as_open rw $dir/resp && _FUXX_AS_SPARE_FD=$REPLY rs=$REPLY || ok=0; fi
    if (( ok )); then
      # The core must not inherit our spare descriptors, or it would hold its own pipe ends open.
      # It starts from `/`, so it never keeps the shell's folder busy (hello and suggest carry the
      # cwd); `exec` keeps the subshell's pid, so $! is the core's. It gets the data folder fuxx
      # named while sourcing us, or none (design §8).
      ( { builtin cd -q / } 2>/dev/null
        if [[ -n $_FUXX_AS_DATA ]]; then export FUXX_PLUGIN_DATA=$_FUXX_AS_DATA; else unset FUXX_PLUGIN_DATA; fi
        exec "$bin" serve <$dir/req >$dir/resp 2>/dev/null {_FUXX_AS_KEEPFD}>&- {rs}>&- ) &!
      _FUXX_AS_PID=$!
      _fuxx_as_open w $dir/req && _FUXX_AS_WFD=$REPLY || ok=0
    fi
    if (( ok )); then _fuxx_as_open r $dir/resp && _FUXX_AS_RFD=$REPLY || ok=0; fi
  } 2>/dev/null
  # A plain function, not `-w`, so it also gets the error argument (`hup`, ...).
  if (( ! ok )) || ! zle -F $_FUXX_AS_RFD _fuxx_as_on_response 2>/dev/null; then
    _fuxx_as_stop kill start-failed
    return 0
  fi
  if ! _fuxx_as_send hello 1 zsh "$ZSH_VERSION" zsh "${HISTFILE:+${HISTFILE:a}}" autosuggestions; then
    _fuxx_as_stop kill write-failed
    return 0
  fi
  _fuxx_as_list_settings
  if (( _FUXX_AS_LIST_SIZE )) \
     && ! _fuxx_as_send listcfg $_FUXX_AS_LIST_SIZE $_FUXX_AS_LIST_DELAY_MS $_FUXX_AS_LIST_EMPTY; then
    _fuxx_as_stop kill write-failed
    return 0
  fi
  _fuxx_as_wrap_widgets
  _fuxx_as_install_once
  _FUXX_AS_STATE=on
}

# The second argument, when given, is why (shown by `doctor`, design §7).
# `_fuxx_as_stop kill`: the core may still be alive (hung, wrong protocol, shell exit), so end it.
# `_fuxx_as_stop` with anything else: the core is already gone (EOF or read error), and its pid
# may belong to another process by now, so it is never signalled.
_fuxx_as_stop() {
  emulate -L zsh
  local why=${1-}
  if [[ -n $_FUXX_AS_RFD ]]; then
    zle -F $_FUXX_AS_RFD 2>/dev/null
    { exec {_FUXX_AS_RFD}<&- } 2>/dev/null
  fi
  [[ -n $_FUXX_AS_WFD ]] && { exec {_FUXX_AS_WFD}>&- } 2>/dev/null
  [[ -n $_FUXX_AS_KEEPFD ]] && { exec {_FUXX_AS_KEEPFD}>&- } 2>/dev/null
  _fuxx_as_close_spare
  if [[ $why == kill && -n $_FUXX_AS_PID ]]; then
    kill $_FUXX_AS_PID 2>/dev/null
  fi
  _fuxx_as_unlink_pipes
  _FUXX_AS_RFD='' _FUXX_AS_WFD='' _FUXX_AS_KEEPFD='' _FUXX_AS_PID=''
  _FUXX_AS_INFLIGHT_ID=''
  _FUXX_AS_UNREAD=0 _FUXX_AS_SINCE_SUGGEST=0
  _FUXX_AS_STATE=off
  [[ -n ${2-} ]] && _FUXX_AS_WHY=$2
  # Inside a handler the line's variables only exist in a widget; elsewhere clear directly.
  zle fuxx-as-clear 2>/dev/null || _fuxx_as_clear
  return 0
}

# _fuxx_as_send <kind> <fields...>: fields joined by 0x1F, a newline inside a field written as 0x1E.
# A write must never block the shell (hard rule 6). With a non-blocking descriptor (zsh 5.9+) a
# full pipe makes `print` fail. Below 5.9 there is no such descriptor, so the bytes the core may
# not have read yet are counted (_FUXX_AS_UNREAD; an answer to a `suggest` resets it to what was
# sent after that `suggest`) and a write that could fill the pipe fails the same way. Callers
# treat a failure as "the core isn't reading" and stop it.
_fuxx_as_send() {
  emulate -L zsh
  if (( _FUXX_AS_HOST )); then
    _fuxx_as_host_queue "$@"
    return 0
  fi
  [[ -n $_FUXX_AS_WFD ]] || return 1
  local f line nl=$'\n' rs=$'\x1e'   # $'..' isn't expanded inside "${//}", so use variables
  local -a out
  for f in "$@"; do
    out+=( "${f//$nl/$rs}" )
  done
  line="${(pj:\x1f:)out}"$'\n'
  local -i len=0
  if (( ! _FUXX_AS_NONBLOCK )); then
    { local LC_ALL=C; len=$#line }   # bytes, not characters
    # A pipe holds at least 8 KiB; stay well under it. The first message into a drained pipe
    # always goes (one message is at most a few KiB).
    (( _FUXX_AS_UNREAD > 0 && _FUXX_AS_UNREAD + len > 6144 )) && return 1
  fi
  print -rn -u $_FUXX_AS_WFD -- "$line" 2>/dev/null || return 1
  if (( ! _FUXX_AS_NONBLOCK )); then
    _FUXX_AS_UNREAD+=len
    if [[ $1 == suggest ]]; then _FUXX_AS_SINCE_SUGGEST=0; else _FUXX_AS_SINCE_SUGGEST+=len; fi
  fi
  return 0
}

# Asks for a suggestion for $BUFFER; one request in flight at a time.
_fuxx_as_request() {
  emulate -L zsh
  [[ $_FUXX_AS_STATE == on ]] || return 0
  if (( _FUXX_AS_HOST )); then
    _fuxx_as_host_forward
    return 0
  fi
  [[ -n $BUFFER ]] || (( (_FUXX_AS_LIST_SIZE && _FUXX_AS_LIST_EMPTY) || _FUXX_AS_FAILED )) || return 0
  (( $#BUFFER > 1000 )) && return 0
  [[ $BUFFER == *[$'\x1f\x1e']* ]] && return 0
  if [[ -n $_FUXX_AS_INFLIGHT_ID ]]; then
    # The answer handler re-sends when the buffer moved on; a core silent for 2 s is hung.
    if (( EPOCHREALTIME - _FUXX_AS_INFLIGHT_SINCE > 2 )); then
      _fuxx_as_stop kill hung
    fi
    return 0
  fi
  local id=$_FUXX_AS_NEXT_ID
  _FUXX_AS_NEXT_ID=$(( id + 1 ))
  _FUXX_AS_INFLIGHT_ID=$id
  _FUXX_AS_INFLIGHT_SINCE=$EPOCHREALTIME
  _FUXX_AS_INFLIGHT_BUFFER=$BUFFER
  _FUXX_AS_LIST_FOR_ID=$id _FUXX_AS_LIST_FOR_SEQ=$_FUXX_AS_KEYSEQ _FUXX_AS_LIST_FOR_BUFFER=$BUFFER
  # `cdpath` goes along, one folder per line: the core checks `cd` targets through it (design
  # §5.4). The field is left off, so the core treats `cdpath` as unknown and keeps relative
  # targets it can't find, when a folder holds a control character the wire can't carry, or when
  # `cd`/`pushd` is an alias or function (zoxide's `--cmd cd`, a wrapper) or CDABLE_VARS is on
  # (read at the prompt: this runs under `emulate`): those reach folders the core can't see.
  local -a f=( suggest $id async $CURSOR "$PWD" "$BUFFER" )
  if (( ! ${#${(@M)cdpath:#*[[:cntrl:]]*}} \
        && ! $+aliases[cd] && ! $+functions[cd] && ! $+aliases[pushd] && ! $+functions[pushd] )) \
     && [[ $_FUXX_AS_CDABLE != on ]]; then
    f+=( "${(pj:\n:)cdpath}" )
  fi
  _fuxx_as_send "${f[@]}" || _fuxx_as_stop kill write-failed
  return 0
}

# Called by `zle -F` when the core's answer pipe is readable (or has failed).
_fuxx_as_on_response() {
  emulate -L zsh
  local fd=$1 err=${2-} line nl=$'\n' rs=$'\x1e'
  local -a f
  if [[ -n $err ]] || ! IFS= read -r -u $fd line 2>/dev/null; then
    _fuxx_as_stop gone exited
    return 0
  fi
  f=( "${(@ps:\x1f:)line}" )
  case $f[1] in
    ready)
      _fuxx_as_unlink_pipes
      _fuxx_as_close_spare
      [[ $f[2] == 1 ]] || _fuxx_as_stop kill protocol
      ;;
    suggestion)
      [[ $f[2] == $_FUXX_AS_INFLIGHT_ID && -n $_FUXX_AS_INFLIGHT_ID ]] || return 0   # stale: drop
      _FUXX_AS_INFLIGHT_ID=''
      _FUXX_AS_UNREAD=$_FUXX_AS_SINCE_SUGGEST   # the core has read everything up to that request
      # $BUFFER and POSTDISPLAY only exist inside a widget, so the rest happens in one.
      zle fuxx-as-answer -- "${f[3]//$rs/$nl}" 2>/dev/null
      ;;
    list)
      # Only for the newest request, and only if no key came since it was sent (design spec M1c).
      [[ $f[2] == "$_FUXX_AS_LIST_FOR_ID" ]] && (( _FUXX_AS_LIST_FOR_SEQ == _FUXX_AS_KEYSEQ )) || return 0
      zle fuxx-as-list-open -- "${(@)f[3,-1]//$rs/$nl}" 2>/dev/null
      ;;
  esac
  return 0
}

# Adds our highlight entry for the current POSTDISPLAY (after the buffer's end).
_fuxx_as_highlight() {
  emulate -L zsh
  [[ -n $POSTDISPLAY ]] || return 0
  _fuxx_as_hl_add "$#BUFFER $(( $#BUFFER + $#POSTDISPLAY )) ${FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE:-fg=8}"
}

# Adds one highlight entry of ours (tagged with `memo=` on 5.9+, remembered exactly below).
_fuxx_as_hl_add() {
  emulate -L zsh
  local hl=$1
  (( _FUXX_AS_MEMO )) && hl+=" memo=fuxx-autosuggestions"
  _FUXX_AS_HLS+=( "$hl" )
  region_highlight+=( "$hl" )
}

# Removes only our own highlight entries: by their `memo=` tag on 5.9+, by their exact strings
# below.
_fuxx_as_unhighlight() {
  emulate -L zsh
  local hl
  if (( _FUXX_AS_MEMO )); then
    region_highlight=( "${(@)region_highlight:#*memo=fuxx-autosuggestions}" )
  else
    for hl in "${(@)_FUXX_AS_HLS}"; do
      region_highlight=( "${(@)region_highlight:#${(b)hl}}" )
    done
  fi
  _FUXX_AS_HLS=()
  return 0
}

_fuxx_as_show() {
  emulate -L zsh
  _fuxx_as_clear
  POSTDISPLAY=$1
  _fuxx_as_highlight
}

_fuxx_as_clear() {
  emulate -L zsh
  POSTDISPLAY=
  _fuxx_as_unhighlight
  return 0
}

_fuxx_as_widget_show() {
  emulate -L zsh
  _fuxx_as_show "$1"
  zle -R
}
zle -N fuxx-as-show _fuxx_as_widget_show

# The answer for the in-flight request: if the line moved on while we waited, ask again and
# draw nothing; otherwise draw it.
_fuxx_as_widget_answer() {
  emulate -L zsh
  if [[ $BUFFER != "$_FUXX_AS_INFLIGHT_BUFFER" ]]; then
    _fuxx_as_request
  else
    zle fuxx-as-show -- "$1"
    # A key that didn't edit came while we waited: ask again, so the list's pause restarts.
    # Not while the ghost text is hidden, nor while a widget still runs (⌃R's search, a menu).
    (( _FUXX_AS_LIST_SIZE && _FUXX_AS_LIST_FOR_SEQ != _FUXX_AS_KEYSEQ && ! _FUXX_AS_HIDDEN )) \
      && [[ -z ${_fuxx_as_inside-} ]] && _fuxx_as_request
  fi
  return 0
}
zle -N fuxx-as-answer _fuxx_as_widget_answer

_fuxx_as_widget_clear() {
  emulate -L zsh
  _fuxx_as_clear
  zle -R
}
zle -N fuxx-as-clear _fuxx_as_widget_clear

# Hides the ghost text until the next edit: what is shown goes, nothing held by an outer wrapper
# comes back, and the answer to a request in flight is dropped as stale when it arrives. The next
# key that changes the buffer asks again as usual.
_fuxx_as_hide() {
  emulate -L zsh
  _FUXX_AS_HIDDEN=1
  _fuxx_as_list_reset
  _fuxx_as_clear
  (( ${+_fuxx_as_held} )) && _fuxx_as_held=
  _FUXX_AS_INFLIGHT_ID=''
  return 0
}

# Esc (design §6): clears a completion list below the prompt (`zle -R -c`; nothing happens when
# none is shown) and hides the ghost text until the next edit.
_fuxx_as_dismiss() {
  emulate -L zsh
  _fuxx_as_hide
  zle -R -c
  return 0
}

# Bare Esc in emacs mode, bound only when `^[` is free. With `^[` bound, zsh waits KEYTIMEOUT after
# an Esc for the rest of a sequence (we never change KEYTIMEOUT), and a bound sequence (Esc f, ⌥→,
# arrows, bracketed paste) still runs as before. A sequence that is not bound (⌃→ `^[[1;5C`, F1
# `^[OP`) now falls back to this binding with its tail left to be read as typed text, so the bytes
# already waiting are read and dropped here: after `[` up to the final byte (@ to ~), after `O` one
# more. Anything else is a key of its own typed within KEYTIMEOUT (a second Esc starting ↑ or a
# paste, é, a letter Esc doesn't bind) and is handed back with `zle -U`, untouched. `read -k`
# reads a whole character, and `read -t 0` never waits for a key not yet typed.
_fuxx_as_widget_escape() {
  emulate -L zsh
  local c
  if read -k 1 -t 0 c 2>/dev/null; then
    if [[ $c == '[' ]]; then
      while read -k 1 -t 0.05 c 2>/dev/null; do
        [[ $c == [@-~] ]] && break
      done
    elif [[ $c == O ]]; then
      read -k 1 -t 0.05 c 2>/dev/null
    else
      zle -U -- "$c"
    fi
  fi
  _fuxx_as_dismiss
}
zle -N fuxx-as-escape _fuxx_as_widget_escape

# _fuxx_as_w_modify <original widget> [args]: runs the original with the ghost text taken off (and
# our highlight entry with it); then, if the buffer changed, asks again, otherwise puts the same
# ghost text back, styled. zsh-autosuggestions does the same. Kept on, it would show as typed text
# whenever the widget redraws: zsh draws the line without region_highlight before completion's "do
# you wish to see all" question, and completion leaves our entry zero-width afterwards.
# While the outermost of our wrappers runs, the hidden suggestion is held in `_fuxx_as_held` (a
# local the wrappers it calls see), so a widget that calls an accept or partial widget
# (`my-end() { zle end-of-line }`) still accepts it, and a widget that changes the buffer drops it.
_fuxx_as_w_modify() {
  emulate -L zsh
  _fuxx_as_list_key || return 0
  local orig=$1
  shift
  # Another plugin that wrapped us can put our wrapper of a widget inside another of ours for the
  # same widget (ours, theirs, ours): only the outermost acts, the inner one just runs its original.
  # Keyed on the widget, so a widget that calls another wrapped widget (`zle end-of-line`) still
  # gets that one's work.
  local self=${orig#fuxx-as-orig-<->-}
  if [[ ${_fuxx_as_inside-} == "$self" ]]; then zle $orig -- "$@"; return; fi
  # Whatever widget Tab runs (expand-or-complete, fzf-completion, fzf-tab-complete, ...): the key
  # decides, not the widget's name, so a Tab rebound later needs nothing but the wrap at precmd.
  # vicmd is left as it was. Esc into vi command mode also clears the list and the ghost text.
  if [[ $KEYS == $'\t' && $KEYMAP != vicmd ]]; then
    _fuxx_as_tab $orig "$@"
    return
  fi
  # Any other completion widget (Shift-Tab's reverse-menu-complete, a ⌃Space bound to
  # complete-word, compinit's `completion:` widgets) leaves the ghost text off as Tab does, so a
  # suggestion that came back never takes the next Tab out of the menu.
  # ⌃D (delete-char-or-list) lists only at the end of the line; before that it is a plain delete,
  # an edit like any other, so it takes the normal path below and asks again.
  local v=${widgets[$orig]-}
  if [[ $self == delete-char-or-list ]] && (( CURSOR < $#BUFFER )); then
    v=''
  fi
  if [[ $v == completion:* ]] || { [[ $v == builtin ]] && (( ${_FUXX_AS_COMPLETERS[(Ie)$self]} )) }; then
    _fuxx_as_complete $orig "$@"
    return
  fi
  local _fuxx_as_inside=$self
  local -i ret holder=0
  if [[ $KEYS == $'\e' && $self == vi-cmd-mode ]]; then
    _fuxx_as_dismiss
    zle $orig -- "$@"
    ret=$?
    _FUXX_AS_LAST_BUFFER=$BUFFER
    return $ret
  fi
  if (( ! ${+_fuxx_as_held} )); then
    local _fuxx_as_held=$POSTDISPLAY
    holder=1
  elif [[ -n $POSTDISPLAY ]]; then
    _fuxx_as_held=$POSTDISPLAY
  fi
  _fuxx_as_clear
  zle $orig -- "$@"
  ret=$?
  if [[ $BUFFER != "$_FUXX_AS_LAST_BUFFER" ]]; then
    _fuxx_as_held=
    _fuxx_as_clear
    _FUXX_AS_LAST_BUFFER=$BUFFER
    _FUXX_AS_HIDDEN=0
    _fuxx_as_request
  else
    (( holder )) && [[ -n $_fuxx_as_held ]] && _fuxx_as_show "$_fuxx_as_held"
    # A key that only moved: ask again, so the list's pause starts over (design spec M1c);
    # not while the ghost text is hidden until the next edit (after Tab, Esc, vi command mode).
    (( holder && _FUXX_AS_LIST_SIZE && ! _FUXX_AS_HIDDEN )) && _fuxx_as_request
  fi
  return $ret
}

# _fuxx_as_tab <original widget> [args]: Tab (design §6). With ghost text showing and the cursor
# at the end, the whole suggestion is accepted (as → does) and the original never runs, so no
# completion and no list. Otherwise the original runs as it did before we were loaded, and the
# ghost text stays off until the next edit: a list may now be on screen, and the next Tab must go
# on completing (auto_menu, menu-select) rather than accept a suggestion that came back.
_fuxx_as_tab() {
  emulate -L zsh
  local orig=$1
  shift
  local -i ret
  if [[ -n $POSTDISPLAY && $CURSOR -eq $#BUFFER ]]; then
    BUFFER+=$POSTDISPLAY
    _fuxx_as_dismiss   # a list from an earlier completion may still be on screen
    CURSOR=$#BUFFER
    _FUXX_AS_LAST_BUFFER=$BUFFER
    return 0
  fi
  _fuxx_as_complete $orig "$@"
}

# _fuxx_as_complete <original widget> [args]: runs a completion widget with the ghost text off, and
# keeps it off until the next edit (see _fuxx_as_tab).
_fuxx_as_complete() {
  emulate -L zsh
  local orig=$1
  shift
  local -i ret
  local _fuxx_as_inside=${orig#fuxx-as-orig-<->-}
  _fuxx_as_hide
  zle $orig -- "$@"
  ret=$?
  _FUXX_AS_LAST_BUFFER=$BUFFER
  return $ret
}

# _fuxx_as_w_accept <original widget> [args]: at the end of the buffer (the last character in vicmd,
# where the cursor can't pass it) the suggestion becomes part of the buffer and the original then
# runs on the longer text; anywhere else the key just moves the cursor and the ghost text stays.
_fuxx_as_w_accept() {
  emulate -L zsh
  _fuxx_as_list_key || return 0
  local orig=$1
  shift
  local self=${orig#fuxx-as-orig-<->-}
  if [[ ${_fuxx_as_inside-} == "$self" ]]; then zle $orig -- "$@"; return; fi
  local -i ret max=$#BUFFER
  local sugg=${POSTDISPLAY:-${_fuxx_as_held-}}
  [[ $KEYMAP == vicmd ]] && max=$(( max - 1 ))
  if [[ -z $sugg || $CURSOR -ne $max ]]; then
    _fuxx_as_w_modify $orig "$@"
    return
  fi
  local _fuxx_as_inside=$self
  BUFFER+=$sugg
  _fuxx_as_clear
  (( ${+_fuxx_as_held} )) && _fuxx_as_held=
  zle $orig -- "$@"
  ret=$?
  # The original may have left vicmd (vi-add-eol), so the keymap is asked again.
  if [[ $KEYMAP == vicmd ]]; then CURSOR=$(( $#BUFFER - 1 )); else CURSOR=$#BUFFER; fi
  _FUXX_AS_LAST_BUFFER=$BUFFER
  return $ret
}

# _fuxx_as_w_partial <original widget> [args]: runs the original on buffer + suggestion; if the cursor
# ended up past the typed text, the part up to the cursor is accepted and the rest stays as ghost
# text, otherwise the buffer is put back. The rest of the suggestion is still right, so no request.
_fuxx_as_w_partial() {
  emulate -L zsh
  _fuxx_as_list_key || return 0
  local orig=$1 saved=$BUFFER sugg=${POSTDISPLAY:-${_fuxx_as_held-}} rest
  shift
  local self=${orig#fuxx-as-orig-<->-}
  if [[ ${_fuxx_as_inside-} == "$self" ]]; then zle $orig -- "$@"; return; fi
  local -i ret loc
  if [[ -z $sugg ]]; then
    _fuxx_as_w_modify $orig "$@"
    return
  fi
  local _fuxx_as_inside=$self
  # As accept does: the suggestion moves into the buffer and the ghost text (with our entry) goes,
  # so an original that calls another partial widget (`my-fw() { zle emacs-forward-word }`) finds
  # nothing to append again; it also finds the buffer already seen, so it asks the core nothing.
  BUFFER+=$sugg
  _fuxx_as_clear
  (( ${+_fuxx_as_held} )) && _fuxx_as_held=
  _FUXX_AS_LAST_BUFFER=$BUFFER
  zle $orig -- "$@"
  ret=$?
  loc=$CURSOR
  [[ $KEYMAP == vicmd ]] && loc+=1   # in vicmd the cursor is ON a character, so it is that one too
  if (( loc > $#saved )); then
    rest=${BUFFER[loc+1,-1]}
    BUFFER=${BUFFER[1,loc]}
  else
    BUFFER=$saved
    rest=$sugg
  fi
  # Called from a widget that one of our modify wrappers runs: that wrapper shows it at its end.
  if (( ${+_fuxx_as_held} )); then
    _fuxx_as_held=$rest
  else
    POSTDISPLAY=$rest
    _fuxx_as_highlight
  fi
  _FUXX_AS_LAST_BUFFER=$BUFFER
  return $ret
}

# _fuxx_as_w_clear <original widget> [args]: widgets that replace the line or leave it.
_fuxx_as_w_clear() {
  emulate -L zsh
  _fuxx_as_list_key || return 0
  local orig=$1
  shift
  local self=${orig#fuxx-as-orig-<->-}
  if [[ ${_fuxx_as_inside-} == "$self" ]]; then zle $orig -- "$@"; return; fi
  local _fuxx_as_inside=$self
  local -i ret
  _fuxx_as_clear
  (( ${+_fuxx_as_held} )) && _fuxx_as_held=
  zle $orig -- "$@"
  ret=$?
  _FUXX_AS_LAST_BUFFER=$BUFFER
  return $ret
}

# _fuxx_as_bind_if_free <keymap> <sequence> <widget>: binds only a key nothing else has bound.
_fuxx_as_bind_if_free() {
  emulate -L zsh
  local cur=$(bindkey -M $1 -- $2 2>/dev/null) k="$1 $2"
  if [[ $cur == *' undefined-key' ]]; then
    bindkey -M $1 -- $2 $3 2>/dev/null
    _FUXX_AS_KEYS[$k]=bound
  else
    _FUXX_AS_KEYS[$k]=kept:${cur##* }
  fi
  return 0
}

_fuxx_as_line_init() {
  emulate -L zsh
  if (( _FUXX_AS_HOST )); then
    # What precmd queued (hello, ran, prev) goes now, from ZLE: nothing is written at startup or
    # in precmd (powerlevel10k's instant prompt).
    local m
    for m in "${_FUXX_AS_HOST_Q[@]}"; do _fuxx_as_host_put "$m"; done
    _FUXX_AS_HOST_Q=() _FUXX_AS_HOST_LAST=''
    return 0
  fi
  _FUXX_AS_LAST_BUFFER=''
  _FUXX_AS_KEYSEQ+=1
  # ⌃C runs no line-finish: drop a list it left, and its ghost text, keymap and highlights.
  _FUXX_AS_LIST_GHOST='' _FUXX_AS_HIDDEN=0
  _fuxx_as_list_reset
  _fuxx_as_clear
  # The list on an empty line (design spec M1c): ask at once, the core times the pause. After a
  # failure ask too, even with the list off: the core may have a fix (M1d).
  [[ -z $BUFFER ]] && (( (_FUXX_AS_LIST_SIZE && _FUXX_AS_LIST_EMPTY) || _FUXX_AS_FAILED )) \
    && _fuxx_as_request
  return 0
}
_fuxx_as_line_finish() {   # ghost text of an accepted line never stays in the scrollback
  # (after ⌃C it can: zsh doesn't redraw the line on SIGINT)
  emulate -L zsh
  if (( _FUXX_AS_HOST )); then
    _fuxx_as_host_osc clear
    _fuxx_as_host_put "$REPLY"
    return 0
  fi
  _fuxx_as_list_reset
  _FUXX_AS_KEYSEQ+=1
  _fuxx_as_clear
  # Forget the request in flight: a handler doesn't run while the command does, so by the next
  # prompt it would look hung (and be killed). Its late answer is dropped as stale by id.
  _FUXX_AS_INFLIGHT_ID='' _FUXX_AS_INFLIGHT_SINCE=0 _FUXX_AS_INFLIGHT_BUFFER=''
}

# Runs at every precmd while the core is on, so it sees every widget the user's rc, other plugins
# and later commands made: oh-my-zsh redefining a widget after the first prompt, plugins loaded late
# (zinit turbo, zsh-defer), a `zle -N` typed at the prompt. It is idempotent: a widget that is
# already ours is left alone, a new one is wrapped, and one the user redefined since (its current
# definition isn't one of our `_fuxx_as_bound_*` functions) is wrapped again around its new
# definition. Each widget gets its own small function that names the original, so a widget that
# another widget calls with `zle name` finds its original without relying on $WIDGET. Every wrap
# is a new generation N with its own names, `fuxx-as-orig-N-<w>` and `_fuxx_as_bound_N_<w>`
# (zsh-autosuggestions' scheme), and older ones are never redefined: another plugin that wrapped
# us (zsh-syntax-highlighting after `source ~/.zshrc`) keeps calling the generation it saved, which
# calls the original that existed then, never our newer wrapper around that plugin (with one
# shared name that loop recursed until FUNCNEST). A widget whose definition still calls our wrapper
# for it (another plugin wrapped us) is left alone: wrapping it again would add a layer at every
# prompt when that plugin also re-wraps at every precmd (zsh-autosuggestions does), and would
# grow the chain until FUNCNEST. A plugin that names our wrapper in a way we can't see is caught
# by a cap of 20 of our generations per widget that some widget still runs (a lifetime count ran
# out after ~20 `source ~/.zshrc` with oh-my-zsh plus a highlighter that wraps under a fresh name
# per load); setting a widget back to a `user:`/`completion:` definition we wrapped before
# (oh-my-zsh on every `source ~/.zshrc`) uses that generation again. Cheap by design: no
# subshell, one pass over the names, and only strings are compared.
_fuxx_as_wrap_widgets() {
  emulate -L zsh
  local w v key gk fn orig
  local -i n named
  local -a live
  # zsh-autosuggestions' default lists; every other widget gets the plain `modify` wrapper.
  local -a accept=( forward-char end-of-line vi-forward-char vi-end-of-line vi-add-eol )
  local -a partial=( forward-word emacs-forward-word vi-forward-word vi-forward-word-end
    vi-forward-blank-word vi-forward-blank-word-end vi-find-next-char vi-find-next-char-skip )
  local -a clear=( history-search-forward history-search-backward history-beginning-search-forward
    history-beginning-search-backward history-beginning-search-forward-end
    history-beginning-search-backward-end history-substring-search-up history-substring-search-down
    up-line-or-beginning-search down-line-or-beginning-search up-line-or-history
    down-line-or-history accept-line copy-earlier-word )
  # Every name except the ones we never wrap, minus the widgets that already run our functions
  # (matched by value, so an alias made by the user of one of ours counts as ours too). Other
  # plugins' saved originals (`orig-*` of zsh-syntax-highlighting, `autosuggest-orig-*` of
  # zsh-autosuggestions) are never wrapped: with a plugin that wraps at every precmd, wrapping each
  # other's saved copies would make new widgets at every prompt.
  local -a todo=( ${(k)widgets:#(.*|_*|fuxx-as-*|orig-*|*-orig-*|zle-*|azhw:*|beep|run-help|set-local-history|which-command|yank|yank-pop)} )
  local -a ours=( ${(k)widgets[(R)user:_fuxx_as_bound_*]} )
  todo=( ${todo:|ours} )
  for w in $todo; do
    v=${widgets[$w]}
    # Whether it calls us depends on the function's body too, not only its name.
    key=$v
    [[ $v == user:* ]] && key+=$'\n'${functions[${v#user:}]-}
    [[ ${_FUXX_AS_UNDER[$w]-} == "$key" ]] && continue
    if _fuxx_as_calls_us $w; then
      _FUXX_AS_UNDER[$w]=$key
      continue
    fi
    # A definition wrapped before is still held by that generation's saved original. Only a
    # `user:` or `completion:` value names its definition: a builtin's is just `builtin`, the same
    # after `zle -A .end-of-line forward-char` as before, so a builtin always gets a new generation.
    gk="$w $v"
    # (No `(a|b)` patterns in this file: it is parsed under the user's options, shglob included.)
    named=0
    [[ $v == user:* || $v == completion:* ]] && named=1
    n=0
    (( named )) && n=${_FUXX_AS_GEN[$gk]-0}
    if (( ! n )); then
      # The cap counts our generations of this widget that some widget still runs (its value is
      # our `_fuxx_as_bound_N_<w>`: a plugin saved it), not every wrap ever made. Once `source
      # ~/.zshrc` sets the widget back, the old generations are named by nothing and don't count;
      # a plugin that saves each of our wrappers under a fresh name at every prompt, where we can't
      # see that it calls us, keeps them all named and is stopped at 20. That definition is then
      # left alone until it changes.
      live=( ${(u)${(M)${(v)widgets}:#user:_fuxx_as_bound_<->_${(b)w}}} )
      if (( $#live >= 20 )); then
        [[ $v == builtin ]] || _FUXX_AS_UNDER[$w]=$key
        continue
      fi
      n=$(( ${_FUXX_AS_WRAPS[$w]-0} + 1 ))
      orig=fuxx-as-orig-$n-$w
      zle -A $w $orig 2>/dev/null || continue
      _FUXX_AS_WRAPS[$w]=$n
      (( named )) && _FUXX_AS_GEN[$gk]=$n
      fn=modify
      (( ${accept[(Ie)$w]} )) && fn=accept
      (( ${partial[(Ie)$w]} )) && fn=partial
      (( ${clear[(Ie)$w]} )) && fn=clear
      functions[_fuxx_as_bound_${n}_$w]="emulate -L zsh; _fuxx_as_w_$fn ${(q)orig} \"\$@\""
    fi
    zle -N $w _fuxx_as_bound_${n}_$w 2>/dev/null
  done
  return 0
}

# _fuxx_as_calls_us <widget>: true when the widget's function still reaches our wrapper for that
# same widget through the widgets it names, as another plugin's wrapper of us does
# (zsh-syntax-highlighting, zsh-autosuggestions: `_their_call orig-…-<widget> "$@"`, where that
# saved widget is our `_fuxx_as_bound_N_<widget>`). Words of each function are checked against
# $widgets only; at most 8 widgets are looked at, and our wrappers of other widgets are not
# followed (a user widget that calls `zle self-insert` is not "us").
_fuxx_as_calls_us() {
  emulate -L zsh
  setopt extendedglob
  local w=$1 x v
  local -a queue=( $w )
  local -i i=1
  while (( i <= $#queue && i <= 8 )); do
    v=${widgets[${queue[i]}]-}
    (( i++ ))
    [[ $v == user:* ]] || continue
    for x in ${(Q)${(z)functions[${v#user:}]-}}; do
      [[ $x == [[:alnum:]_.:-]## ]] || continue
      (( ${+widgets[$x]} && ! ${queue[(Ie)$x]} )) || continue
      v=${widgets[$x]}
      [[ $v == user:_fuxx_as_bound_<->_${(b)w} ]] && return 0
      [[ $v == user:_fuxx_as_bound_* ]] || queue+=( $x )
    done
  done
  return 1
}

# Once, at start: the keys nobody has bound, and the line hooks.
_fuxx_as_install_once() {
  emulate -L zsh
  local km
  # End and ⌥→ have no default binding; vicmd is left alone.
  for km in emacs viins; do
    _fuxx_as_bind_if_free $km '^[[F' end-of-line
    _fuxx_as_bind_if_free $km '^[[4~' end-of-line
    _fuxx_as_bind_if_free $km '^[[1;3C' forward-word
  done
  # The host's accept (design §5.5): a CSI no keyboard sends, bound only where it is free.
  if (( _FUXX_AS_HOST )); then
    for km in emacs viins; do
      _fuxx_as_bind_if_free $km '^[[7701~' fuxx-as-host-accept
    done
  fi
  # Bare Esc in emacs mode (design §6); in viins it is vi-cmd-mode, handled in _fuxx_as_w_modify.
  _fuxx_as_bind_if_free emacs '^[' fuxx-as-escape
  add-zle-hook-widget line-init _fuxx_as_line_init 2>/dev/null
  add-zle-hook-widget line-finish _fuxx_as_line_finish 2>/dev/null
  return 0
}

# Notes the line zsh is about to consider for history. Always returns 0: this hook never decides
# what zsh keeps (a nonzero status would make zsh drop the line, or skip other hooks' say).
_fuxx_as_addhistory() {
  emulate -L zsh
  _FUXX_AS_NOTED=${1%$'\n'}
  return 0
}

_fuxx_as_preexec() {
  emulate -L zsh
  _FUXX_AS_CMD_PWD=$PWD
  return 0
}

# Reports each command zsh kept, one prompt late (design §5.8). A line zsh won't keep stays in
# $history at its event number until the next command is entered, then that command takes the same
# number. So at each prompt where a line was entered, the pending command (event p) was kept only
# if the newest event n is beyond p and still holds the same text; the new command then becomes
# pending. `$?` and the options are read before `emulate -L zsh` (which resets the options).
_fuxx_as_precmd() {
  local -i st=$?
  local ign_space=${options[histignorespace]-off} no_store=${options[histnostore]-off}
  _FUXX_AS_CDABLE=${options[cdablevars]-off}   # for _fuxx_as_request, which runs emulated
  # Every option that changes how a pattern is read; HISTORY_IGNORE is matched under the user's.
  local -a pat_opts=()
  local o
  for o in kshglob shglob extendedglob nocasematch nocaseglob globstarshort; do
    [[ ${options[$o]-off} == on ]] && pat_opts+=( $o )
  done
  emulate -L zsh
  # Coexistence (design §9): checked at every prompt, so a plugin loaded late stops us too.
  if [[ $_FUXX_AS_STATE != off ]]; then
    _fuxx_as_conflict
    if [[ -n $REPLY ]]; then
      if [[ $_FUXX_AS_STATE == on ]]; then
        _fuxx_as_stop kill $REPLY
      else
        _FUXX_AS_STATE=off _FUXX_AS_WHY=$REPLY
      fi
      return 0
    fi
  fi
  if [[ -z $_FUXX_AS_STATE ]]; then
    _fuxx_as_start
    return 0
  fi
  [[ $_FUXX_AS_STATE == on ]] && _fuxx_as_wrap_widgets
  [[ -n $_FUXX_AS_NOTED ]] || return 0
  local noted=$_FUXX_AS_NOTED E
  local -i n=$(( HISTCMD - 1 )) p
  _FUXX_AS_NOTED=''
  # A fact for the core (design §5.5, M1d): after a failure the empty line asks for a fix.
  _FUXX_AS_FAILED=$(( st != 0 ))
  [[ $_FUXX_AS_STATE == on ]] || return 0
  if [[ -n $_FUXX_AS_PEND_EVENT ]]; then
    p=$_FUXX_AS_PEND_EVENT
    if (( n > p )) && [[ ${history[$p]-} == "$_FUXX_AS_PEND_ENTRY" ]]; then
      _fuxx_as_send ran "$_FUXX_AS_PEND_PWD" "$_FUXX_AS_PEND_STATUS" "$_FUXX_AS_PEND_TIME" "$_FUXX_AS_PEND_ENTRY" \
        || _fuxx_as_stop kill write-failed
    fi
    _FUXX_AS_PEND_EVENT='' _FUXX_AS_PEND_ENTRY='' _FUXX_AS_PEND_PWD=''
    _FUXX_AS_PEND_STATUS='' _FUXX_AS_PEND_TIME=''
    [[ $_FUXX_AS_STATE == on ]] || return 0
  fi
  # The core learns the command just run and its exit status (`prev`, design §5.5) as soon as it
  # is pending; a line the filters drop leaves it unknown.
  if _fuxx_as_take_pending; then
    _fuxx_as_send prev "$_FUXX_AS_PEND_ENTRY" "$_FUXX_AS_PEND_STATUS" || _fuxx_as_stop kill write-failed
  else
    _fuxx_as_send prev '' || _fuxx_as_stop kill write-failed
  fi
  return 0
}

# Called from precmd only (it reads precmd's locals: st, noted, n, ign_space, no_store,
# pat_opts): makes the line just entered the pending report when zsh will keep it. Returns 0
# when it did.
_fuxx_as_take_pending() {
  local E
  # Report E, the entry as zsh stored it (HIST_REDUCE_BLANKS etc.), so it matches the file.
  E=${history[$n]-}
  [[ -n $E ]] || return 1
  [[ $ign_space == on && $noted == ' '* ]] && return 1
  [[ $no_store == on && ( $E == history* || $E == 'fc -l'* ) ]] && return 1
  # HISTORY_IGNORE only filters what zsh writes to the file, so it is checked here, with the
  # options zsh itself compiles the pattern under (kshglob and shglob change what it means). Only
  # a clean "no match" lets the command through: a bad pattern is silent and means don't report.
  if [[ -n ${HISTORY_IGNORE-} ]]; then
    local -i rc
    (( $#pat_opts )) && setopt $pat_opts
    { [[ $E == ${~HISTORY_IGNORE} ]]; rc=$? } 2>/dev/null
    (( rc == 1 )) || return 1
  fi
  (( $#E > 1000 )) && return 1
  [[ $E == *[$'\x1f\x1e']* ]] && return 1   # the wire can't carry them
  _FUXX_AS_PEND_EVENT=$n _FUXX_AS_PEND_ENTRY=$E _FUXX_AS_PEND_PWD=$_FUXX_AS_CMD_PWD
  _FUXX_AS_PEND_STATUS=$st _FUXX_AS_PEND_TIME=$EPOCHSECONDS
  return 0
}

# ---- The suggestion list (design spec M1c) ----

# Resolves the list settings once the user's rc has run: fuxx's Settings (copied at load) win,
# then the shell variables, then the defaults (on, on an empty line, 3 s, 5 items). An unreadable
# value counts as unset. Sets _FUXX_AS_LIST_SIZE (0 = off), _DELAY_MS, _EMPTY and, for doctor,
# each value and where it came from.
_fuxx_as_list_settings() {
  emulate -L zsh
  local name var v src ok cand
  local -A defaults=( list on list_empty on list_delay 3 list_size 5 )
  for name in list list_empty list_delay list_size; do
    var=${(U)name}
    src=default v=$defaults[$name]
    for cand in "fuxx:${(P)${:-_FUXX_AS_PSET_$var}}" "shell:${(P)${:-FUXX_AUTOSUGGESTIONS_$var}-}"; do
      local where=${cand%%:*} val=${cand#*:}
      [[ -n $val ]] || continue
      ok=''
      case $name in
        list|list_empty)
          case ${(L)val} in
            on|1|true|yes) val=on ok=1 ;;
            off|0|false|no) val=off ok=1 ;;
          esac ;;
        list_delay) [[ $val == <-> || $val == <->.<-> || $val == .<-> ]] && ok=1 ;;
        list_size) [[ $val == <-> ]] && ok=1 ;;
      esac
      if [[ -n $ok ]]; then
        v=$val src=$where
        break
      fi
    done
    _FUXX_AS_LIST_VAL[$name]=$v _FUXX_AS_LIST_SRC[$name]=$src
  done
  local -F secs=$_FUXX_AS_LIST_VAL[list_delay]
  local -i ms=$(( secs * 1000 )) size=$_FUXX_AS_LIST_VAL[list_size]
  (( ms < 100 )) && ms=100
  (( ms > 60000 )) && ms=60000
  (( size < 1 )) && size=1
  (( size > 5 )) && size=5
  _FUXX_AS_LIST_DELAY_MS=$ms
  _FUXX_AS_LIST_SIZE=0
  [[ $_FUXX_AS_LIST_VAL[list] == on ]] && _FUXX_AS_LIST_SIZE=$size
  _FUXX_AS_LIST_EMPTY=0
  [[ $_FUXX_AS_LIST_VAL[list_empty] == on ]] && _FUXX_AS_LIST_EMPTY=1
  return 0
}

# Forgets the list on screen and leaves its keymap (no redraw: the caller draws what's next).
_fuxx_as_list_reset() {
  emulate -L zsh
  _FUXX_AS_LIST_ITEMS=() _FUXX_AS_LIST_SEL=0
  [[ ${KEYMAP-} == fuxx-as-list ]] && zle -K $_FUXX_AS_LIST_PREV 2>/dev/null
  return 0
}

# The ghost text the line should keep when the list closes: the highlighted item's rest, or the
# ghost text the list opened over.
_fuxx_as_list_ghost() {
  emulate -L zsh
  REPLY=$_FUXX_AS_LIST_GHOST
  if (( _FUXX_AS_LIST_SEL )); then
    REPLY=${_FUXX_AS_LIST_ITEMS[_FUXX_AS_LIST_SEL]}
    [[ $REPLY == "$BUFFER"* ]] && REPLY=${REPLY#"$BUFFER"} || REPLY=''
  fi
}

# While the list shows, its keys live in a keymap of our own copied from the current one, so
# every other key works as before and no binding outside the list changes.
_fuxx_as_list_keymap() {
  emulate -L zsh
  [[ $KEYMAP == fuxx-as-list ]] && return 0
  _FUXX_AS_LIST_PREV=$KEYMAP
  bindkey -N fuxx-as-list $KEYMAP 2>/dev/null || return 0
  local k
  for k in '^[[B' '^[OB'; do bindkey -M fuxx-as-list -- $k fuxx-as-list-down; done
  for k in '^[[A' '^[OA'; do bindkey -M fuxx-as-list -- $k fuxx-as-list-up; done
  for k in '^[1' '^[2' '^[3' '^[4' '^[5' '¡' '™' '£' '¢' '∞'; do
    bindkey -M fuxx-as-list -- $k fuxx-as-list-pick
  done
  bindkey -M fuxx-as-list -- '^M' fuxx-as-list-enter
  bindkey -M fuxx-as-list -- '^I' fuxx-as-list-tab
  bindkey -M fuxx-as-list -- '^[' fuxx-as-list-esc
  zle -K fuxx-as-list
}

# Runs a binding found with `bindkey -M`: a widget by name. A `bindkey -s` string (shown quoted)
# is left alone rather than run as a widget name, which would print an error at the prompt.
_fuxx_as_run_binding() {
  emulate -L zsh
  [[ -n $1 && $1 != undefined-key && $1 != \"* ]] || return 0
  zle $1
}

# Puts list item $1 into the line (never runs it); the list closes and the ghost text is asked
# for the new line.
_fuxx_as_list_insert() {
  emulate -L zsh
  local item=${_FUXX_AS_LIST_ITEMS[$1]-}
  [[ -n $item ]] || return 0
  _FUXX_AS_KEYSEQ+=1
  _fuxx_as_list_reset
  _fuxx_as_clear
  BUFFER=$item CURSOR=$#item
  _FUXX_AS_LAST_BUFFER=$BUFFER
  _fuxx_as_request
  zle -R
}

# Closes the list (keeping the ghost text that belongs to the line) and runs what the key does
# in the keymap the list was opened over.
_fuxx_as_list_passthrough() {
  emulate -L zsh
  _fuxx_as_list_ghost
  local ghost=$REPLY w
  _fuxx_as_list_reset
  _fuxx_as_show "$ghost"
  w=$(bindkey -M $_FUXX_AS_LIST_PREV -- "$KEYS" 2>/dev/null)
  _fuxx_as_run_binding "${w##* }"
  return 0
}

_fuxx_as_widget_list_down() {
  emulate -L zsh
  (( _FUXX_AS_LIST_SEL < $#_FUXX_AS_LIST_ITEMS )) && _FUXX_AS_LIST_SEL+=1
  _fuxx_as_list_draw
  zle -R
}
zle -N fuxx-as-list-down _fuxx_as_widget_list_down

# ↑ moves up inside the list; with nothing highlighted it is the key it always was (history).
_fuxx_as_widget_list_up() {
  emulate -L zsh
  if (( _FUXX_AS_LIST_SEL == 0 )); then
    _fuxx_as_list_passthrough
    return 0
  fi
  _FUXX_AS_LIST_SEL+=-1
  _fuxx_as_list_draw
  zle -R
}
zle -N fuxx-as-list-up _fuxx_as_widget_list_up

# ⌥1–⌥5: Esc then the digit, or the characters macOS composes for them on a US layout.
_fuxx_as_widget_list_pick() {
  emulate -L zsh
  local -i n
  case $KEYS in
    $'\e1'|'¡') n=1 ;;
    $'\e2'|'™') n=2 ;;
    $'\e3'|'£') n=3 ;;
    $'\e4'|'¢') n=4 ;;
    $'\e5'|'∞') n=5 ;;
  esac
  (( n >= 1 && n <= $#_FUXX_AS_LIST_ITEMS )) && _fuxx_as_list_insert $n
  return 0
}
zle -N fuxx-as-list-pick _fuxx_as_widget_list_pick

# Enter and Tab: a highlighted item goes into the line (nothing runs); with none highlighted they
# do what they always did.
_fuxx_as_widget_list_enter() {
  emulate -L zsh
  if (( _FUXX_AS_LIST_SEL )); then
    _fuxx_as_list_insert $_FUXX_AS_LIST_SEL
  else
    _fuxx_as_list_passthrough
  fi
}
zle -N fuxx-as-list-enter _fuxx_as_widget_list_enter
zle -N fuxx-as-list-tab _fuxx_as_widget_list_enter

# Esc closes the list and hides the ghost text, then does what Esc does in the keymap the list
# opened over: vi insert mode still goes on to command mode, and a user's own Esc binding runs
# (our own Esc widget has nothing left to do).
_fuxx_as_widget_list_esc() {
  emulate -L zsh
  local w
  w=$(bindkey -M $_FUXX_AS_LIST_PREV -- "$KEYS" 2>/dev/null)
  w=${w##* }
  _FUXX_AS_KEYSEQ+=1
  if [[ $w == fuxx-as-escape ]]; then
    # Our own Esc also drops the tail of a sequence nothing binds (⌃→), so it isn't typed.
    _fuxx_as_list_reset
    zle fuxx-as-escape
    return 0
  fi
  _fuxx_as_dismiss
  zle -R
  _fuxx_as_run_binding "$w"
  return 0
}
zle -N fuxx-as-list-esc _fuxx_as_widget_list_esc

# Every wrapped key: the list closes (the plain ghost text comes back, so the wrappers see
# POSTDISPLAY as the ghost text, as always) and the key counter moves on, so a list for an
# earlier request is never drawn.
# Returns 1 when the key was the list's own: zsh had already read it in the old keymap when the
# list opened (it arrives while zsh waits for a key), so it lands here; its list widget is run and
# the caller stops.
_fuxx_as_list_key() {
  emulate -L zsh
  if (( $#_FUXX_AS_LIST_ITEMS )) && [[ $KEYMAP == fuxx-as-list ]]; then
    local w=$(bindkey -M fuxx-as-list -- "$KEYS" 2>/dev/null)
    w=${w##* }
    if [[ $w == fuxx-as-list-* ]]; then
      zle $w
      return 1
    fi
  fi
  _FUXX_AS_KEYSEQ+=1
  (( $#_FUXX_AS_LIST_ITEMS )) || return 0
  _fuxx_as_list_ghost
  local ghost=$REPLY
  _fuxx_as_list_reset
  _fuxx_as_show "$ghost"
  return 0
}

# Draws the ghost text and the list under it (design spec §9; the gutter look, the user's pick,
# 2026-11-24): a blank line under the input, then rows "  ▎ N  <item>", the whole list dim; the
# selected row's bar in the accent colour (blue) and its command at normal brightness, its number
# dim. No title, no hint. Newlines show as ↵, tabs and control characters as a space; every line is
# cut to the width by screen columns with …. A short window loses the blank line before any item.
_fuxx_as_list_draw() {
  emulate -L zsh
  local ghost=$_FUXX_AS_LIST_GHOST out row
  local -i i n cols=$(( COLUMNS - 1 )) avail=$(( LINES - 3 )) pos
  local -a spans
  (( avail < 1 )) && avail=1
  if (( _FUXX_AS_LIST_SEL )); then
    ghost=${_FUXX_AS_LIST_ITEMS[_FUXX_AS_LIST_SEL]}
    [[ $ghost == "$BUFFER"* ]] && ghost=${ghost#"$BUFFER"} || ghost=''
  fi
  n=$#_FUXX_AS_LIST_ITEMS
  (( n > avail )) && n=avail
  out=$ghost
  pos=$(( $#BUFFER + $#ghost ))
  (( $#ghost )) && spans+=( "$#BUFFER $pos ${FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE:-fg=8}" )
  if (( avail >= n + 1 )); then
    out+=$'\n'
    pos=$(( pos + 1 ))
  fi
  for (( i = 1; i <= n; i++ )); do
    row="  ▎ $i  "
    row+=${_FUXX_AS_LIST_ITEMS[i]//$'\n'/ ↵ }
    _fuxx_as_list_fit $row $cols
    row=$REPLY
    if (( i == _FUXX_AS_LIST_SEL )); then
      # The row starts at pos+1: the bar is its 3rd character, the number its 5th.
      spans+=( "$(( pos + 3 )) $(( pos + 4 )) fg=blue" "$(( pos + 5 )) $(( pos + 5 + ${#i} )) fg=8" )
    else
      spans+=( "$(( pos + 1 )) $(( pos + 1 + $#row )) fg=8" )
    fi
    out+=$'\n'$row
    pos=$(( pos + 1 + $#row ))
  done
  _fuxx_as_clear
  POSTDISPLAY=$out
  for row in "${(@)spans}"; do _fuxx_as_hl_add "$row"; done
  return 0
}

# REPLY = $1 with control characters as spaces, cut to $2 screen columns with … (CJK and emoji
# take two columns each).
_fuxx_as_list_fit() {
  emulate -L zsh
  REPLY=${1//[[:cntrl:]]/ }
  if (( ${(m)#REPLY} > $2 )); then
    while (( ${(m)#REPLY} > $2 - 1 )); do REPLY=${REPLY[1,-2]}; done
    REPLY+=…
  fi
}

# A pushed list arrived for the newest request (checked by the response handler): draw it, if
# the line is still the one it was asked for and the editor is in a plain keymap.
_fuxx_as_widget_list_open() {
  emulate -L zsh
  [[ $BUFFER == "$_FUXX_AS_LIST_FOR_BUFFER" ]] || return 0
  [[ $KEYMAP == main || $KEYMAP == emacs || $KEYMAP == viins ]] || return 0
  # Never over a widget that is still running (⌃R's search, a menu) or a hidden ghost text.
  [[ -z ${_fuxx_as_inside-} ]] && (( ! _FUXX_AS_HIDDEN )) || return 0
  (( $# )) || return 0
  (( $#_FUXX_AS_LIST_ITEMS )) || _FUXX_AS_LIST_GHOST=$POSTDISPLAY
  _FUXX_AS_LIST_ITEMS=( "$@" ) _FUXX_AS_LIST_SEL=0
  _fuxx_as_list_draw
  _fuxx_as_list_keymap
  zle -R
}
zle -N fuxx-as-list-open _fuxx_as_widget_list_open

# Sets REPLY to why we must stay off in this shell (design §9), or '': Warp replaces the line
# editor, and zsh-autosuggestions draws its own ghost text (unless FUXX_AUTOSUGGESTIONS_FORCE=1).
# ---- The host channel (design §5.5, M2) ----

# REPLY = base64 (RFC 4648, padded) of $1's bytes, in pure zsh: one fork per keystroke would slow
# typing.
_fuxx_as_b64() {
  emulate -L zsh
  setopt nomultibyte
  local s=$1 t='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/' out='' c
  local -i i a b d n k1 k2 k3 k4 len=$#s
  for (( i = 1; i <= len; i += 3 )); do
    c=$s[i]; (( a = #c ))
    b=0 d=0
    if (( i + 1 <= len )); then c=$s[i+1]; (( b = #c )); fi
    if (( i + 2 <= len )); then c=$s[i+2]; (( d = #c )); fi
    (( n = (a << 16) | (b << 8) | d ))
    (( k1 = (n >> 18 & 63) + 1, k2 = (n >> 12 & 63) + 1, k3 = (n >> 6 & 63) + 1, k4 = (n & 63) + 1 ))
    out+=$t[k1]$t[k2]
    if (( i + 1 <= len )); then out+=$t[k3]; else out+='='; fi
    if (( i + 2 <= len )); then out+=$t[k4]; else out+='='; fi
  done
  REPLY=$out
}

# REPLY = the bytes $1 (padded base64) encodes; returns 1 when it isn't valid base64.
_fuxx_as_unb64() {
  emulate -L zsh
  setopt nomultibyte
  local s=$1 out='' c body rest
  local -i i k n v pad x
  (( $#s % 4 == 0 )) || return 1
  # Letters, digits, + and /, then at most two `=` (no parentheses here: a user's SHGLOB parses
  # this file too).
  body=${s%%=*}
  rest=${s#"$body"}
  [[ $rest == '' || $rest == '=' || $rest == '==' ]] || return 1
  [[ $body == *[^A-Za-z0-9+/]* ]] && return 1
  for (( i = 1; i <= $#s; i += 4 )); do
    n=0 pad=0
    for (( k = 0; k < 4; k++ )); do
      c=$s[i+k]
      (( x = #c ))
      if (( x == 61 )); then           # =
        (( pad++, v = 0 ))
      elif (( x >= 65 && x <= 90 )); then (( v = x - 65 ))   # A-Z
      elif (( x >= 97 && x <= 122 )); then (( v = x - 71 ))  # a-z
      elif (( x >= 48 && x <= 57 )); then (( v = x + 4 ))    # 0-9
      elif (( x == 43 )); then v=62                          # +
      else v=63                                              # / (the pattern above allows no other)
      fi
      (( n = (n << 6) | v ))
    done
    (( pad && i + 4 <= $#s )) && return 1   # padding only at the end
    x=$(( n >> 16 & 255 )); out+=${(#)x}
    if (( pad < 2 )); then x=$(( n >> 8 & 255 )); out+=${(#)x}; fi
    if (( pad < 1 )); then x=$(( n & 255 )); out+=${(#)x}; fi
  done
  REPLY=$out
}

# _fuxx_as_host_osc <message> [n:<number> | s:<text>]...: REPLY = the OSC 7701 for it.
_fuxx_as_host_osc() {
  emulate -L zsh
  local out=$'\e]7701;fuxx-completes;1;'$1 f
  shift
  for f in "$@"; do
    case $f in
      n:*) out+=";${f#n:}" ;;
      *) _fuxx_as_b64 "${f#s:}"; out+=";$REPLY" ;;
    esac
  done
  REPLY=$out$'\a'
}

# Writes to the terminal, never to stdout.
_fuxx_as_host_put() {
  emulate -L zsh
  [[ -n ${TTY-} ]] || return 0
  print -rn -- "$1" >>$TTY 2>/dev/null
  return 0
}

# What `_fuxx_as_send` would tell a core, queued for the host until ZLE runs: hello, ran, prev.
_fuxx_as_host_queue() {
  emulate -L zsh
  case $1 in
    hello) _fuxx_as_host_osc hello s:$3 s:$4 s:$5 s:$6 s:$7 ;;
    ran) _fuxx_as_host_osc ran s:$2 n:$3 n:$4 s:$5 ;;
    prev)
      if [[ -n ${3-} ]]; then
        _fuxx_as_host_osc prev s:$2 n:$3
      else
        _fuxx_as_host_osc prev s:$2 ''
      fi ;;
    *) return 0 ;;
  esac
  _FUXX_AS_HOST_Q+=( "$REPLY" )
}

# Sends the line to the host when it changed (from a widget). Over 1000 characters, or holding
# 0x1F/0x1E, it is never sent (design §5.5).
_fuxx_as_host_forward() {
  emulate -L zsh
  local key="$CURSOR:$BUFFER"
  [[ $key == "$_FUXX_AS_HOST_LAST" ]] && return 0
  _FUXX_AS_HOST_LAST=$key
  (( $#BUFFER > 1000 )) && return 0
  [[ $BUFFER == *[$'\x1f\x1e']* ]] && return 0
  local id=$_FUXX_AS_NEXT_ID
  _FUXX_AS_NEXT_ID=$(( id + 1 ))
  _FUXX_AS_HOST_ID=$id
  _fuxx_as_host_osc buffer n:$id n:$CURSOR s:$PWD s:$BUFFER
  _fuxx_as_host_put "$REPLY"
}

# `ESC [ 7701 ~` from the host, then base64 of `<id> US <buffer> US <cursor>` and BEL: reads the
# rest as long as it keeps coming (each next character within 50 ms; at most 1 s and 16 KiB in
# all) and applies it only when the id is the last `buffer` sent. Anything else (late, stale,
# broken, too long) is read and dropped up to its BEL, so no stray characters are typed. (A
# single 50 ms for the whole payload typed the tail of a long one: fix after the M2 review.)
_fuxx_as_widget_host_accept() {
  emulate -L zsh
  local c payload=''
  local -i got=0 n=0
  local -F end=$(( EPOCHREALTIME + 1 ))
  while (( EPOCHREALTIME < end )); do
    read -k 1 -t 0.05 c 2>/dev/null || break
    if [[ $c == $'\a' ]]; then
      got=1
      break
    fi
    (( ++n <= 16384 )) && payload+=$c
  done
  (( n <= 16384 )) || return 0
  (( got )) || return 0
  _fuxx_as_unb64 "$payload" || return 0
  local text=$REPLY id cur
  [[ $text == *$'\x1f'*$'\x1f'* ]] || return 0
  id=${text%%$'\x1f'*} cur=${text##*$'\x1f'}
  text=${text#*$'\x1f'}
  text=${text%$'\x1f'*}
  [[ $id == <-> && $cur == <-> && $id == "$_FUXX_AS_HOST_ID" ]] || return 0
  BUFFER=$text
  CURSOR=$cur
  _fuxx_as_host_forward
}
zle -N fuxx-as-host-accept _fuxx_as_widget_host_accept

_fuxx_as_conflict() {
  emulate -L zsh
  REPLY=''
  if [[ ${TERM_PROGRAM-} == WarpTerminal ]]; then
    REPLY=conflict:warp
  elif (( $+functions[_zsh_autosuggest_start] )) && [[ ${FUXX_AUTOSUGGESTIONS_FORCE-} != 1 ]]; then
    REPLY=conflict:zsh-autosuggestions
  fi
}

# What this shell knows for `doctor` (design §7), one `name=value` per line. $1, when given, is
# the user's SHGLOB as the caller saw it before its own `emulate`.
_fuxx_as_facts() {
  local shglob=${1:-${options[shglob]-off}}
  emulate -L zsh
  local k
  print -r -- shell=zsh
  print -r -- version=$ZSH_VERSION
  print -r -- state=${_FUXX_AS_STATE:-starting}
  print -r -- why=${_FUXX_AS_WHY-}
  print -r -- file=${_FUXX_AS_FILE-}
  for k in "${(@)_FUXX_AS_SECOND}"; do print -r -- second=$k; done
  print -r -- data=${_FUXX_AS_DATA-}
  for k in "${(@ko)_FUXX_AS_KEYS}"; do print -r -- "key=$k $_FUXX_AS_KEYS[$k]"; done
  print -r -- shglob=$shglob
  print -r -- histfile=${HISTFILE:+${HISTFILE:a}}
  print -r -- force=${FUXX_AUTOSUGGESTIONS_FORCE:-0}
  if (( _FUXX_AS_HOST )); then
    print -r -- host=on
  elif [[ -n $_FUXX_AS_HOST_WHY ]]; then
    print -r -- host=off:$_FUXX_AS_HOST_WHY
  else
    print -r -- host=off
  fi
  for k in list list_empty list_delay list_size; do
    [[ -n ${_FUXX_AS_LIST_VAL[$k]-} ]] && print -r -- "setting=$k ${_FUXX_AS_LIST_VAL[$k]} ${_FUXX_AS_LIST_SRC[$k]}"
  done
}

_fuxx_as_zshexit() {
  emulate -L zsh
  [[ $_FUXX_AS_STATE == on ]] && _fuxx_as_send bye
  _fuxx_as_stop kill
  return 0
}

# `fuxx-completes forget`, `doctor` and `why` typed at this prompt (design §7): the binary is
# run with this adapter's copy of FUXX_PLUGIN_DATA (fuxx unsets the variable after sourcing us,
# so without this `forget` would wipe another folder), `doctor` gets the shell's facts and `why`
# this folder and history file. Defined only when the user has no function or alias of that name (ours is
# replaced when the same file is sourced again).
() {
  emulate -L zsh
  (( _FUXX_AS_OWN_FN || ! ( $+functions[fuxx-completes] || $+aliases[fuxx-completes] ) )) || return 0
  typeset -gi _FUXX_AS_OWN_FN=1
  fuxx-completes() {
    local shglob=${options[shglob]-off}   # the user's, before emulate resets it
    emulate -L zsh
    local bin=${FUXX_COMPLETES_BIN:-${commands[fuxx-completes]-}}
    if [[ -z $bin ]]; then
      print -ru2 -- 'fuxx-completes: not found on PATH'
      return 127
    fi
    if [[ $1 == doctor ]]; then
      shift
      FUXX_PLUGIN_DATA=$_FUXX_AS_DATA "$bin" doctor --shell-facts "$(_fuxx_as_facts $shglob)" "$@"
    elif [[ $1 == why ]]; then
      # `why` reads what this shell's core reads: this folder and this history file (M1d).
      shift
      local -a hf=()
      [[ -n ${HISTFILE-} ]] && hf=( --histfile "${HISTFILE:a}" )
      FUXX_PLUGIN_DATA=$_FUXX_AS_DATA "$bin" why --cwd "$PWD" "${hf[@]}" -- "$@"
    else
      FUXX_PLUGIN_DATA=$_FUXX_AS_DATA "$bin" "$@"
    fi
  }
}

add-zsh-hook precmd _fuxx_as_precmd
add-zsh-hook preexec _fuxx_as_preexec
add-zsh-hook zshaddhistory _fuxx_as_addhistory
add-zsh-hook zshexit _fuxx_as_zshexit
