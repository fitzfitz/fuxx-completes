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
  (( $+_FUXX_AS_STATE )) && return 0
  # State. '' until the first prompt, then `on` or `off`.
  typeset -g _FUXX_AS_STATE=''
  typeset -g _FUXX_AS_PID=''
  typeset -g _FUXX_AS_WFD='' _FUXX_AS_RFD='' _FUXX_AS_KEEPFD=''
  typeset -g _FUXX_AS_NEXT_ID=1 _FUXX_AS_INFLIGHT_ID='' _FUXX_AS_INFLIGHT_SINCE=0
  typeset -g _FUXX_AS_INFLIGHT_BUFFER='' _FUXX_AS_LAST_BUFFER=''
  typeset -g _FUXX_AS_DIR=''     # the temp folder holding the pipes, until the core has them open
  typeset -g _FUXX_AS_HL=''      # the exact highlight string we added (used below zsh 5.9)
  typeset -g _FUXX_AS_MEMO=0     # 1 when region_highlight accepts `memo=` (zsh 5.9+)
  typeset -g _FUXX_AS_CLOEXEC=0  # 1 when `sysopen -o cloexec` works on this zsh
  is-at-least 5.9 && _FUXX_AS_MEMO=1
  local fd
  # zsh/system is optional: without sysopen or its cloexec option we open pipes with plain `exec`.
  if zmodload zsh/system 2>/dev/null && sysopen -r -o cloexec -u fd /dev/null 2>/dev/null; then
    { exec {fd}<&- } 2>/dev/null
    _FUXX_AS_CLOEXEC=1
  fi
  return 0
} || return 0

_fuxx_as_unlink_pipes() {
  emulate -L zsh
  [[ -n $_FUXX_AS_DIR ]] || return 0
  zf_rm -f $_FUXX_AS_DIR/req $_FUXX_AS_DIR/resp 2>/dev/null
  zf_rmdir $_FUXX_AS_DIR 2>/dev/null
  _FUXX_AS_DIR=''
  return 0
}

# _fuxx_as_open <r|w|rw> <file>: opens a pipe, close-on-exec where zsh can (so the programs the
# user runs never inherit it); the descriptor number comes back in $REPLY.
_fuxx_as_open() {
  emulate -L zsh
  local mode=$1 file=$2 fd
  if (( _FUXX_AS_CLOEXEC )); then
    sysopen -$mode -o cloexec -u fd $file 2>/dev/null || return 1
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
# write end of `req` and read end of `resp`, and drops the helper descriptors. A dead core gives
# EOF on the read end, and a write never raises EPIPE because the shell keeps a reader on `req`.
# The pipes are unlinked when the core's `ready` arrives, not before: the core opens them by path
# in its own process, and unlinking earlier races that open (the core then fails to start).
_fuxx_as_start() {
  emulate -L zsh
  local bin=${FUXX_COMPLETES_BIN:-${commands[fuxx-completes]-}} dir rs='' ok=1
  if [[ -z $bin || ! -x $bin || -d $bin ]]; then
    _FUXX_AS_STATE=off
    return 0
  fi
  dir=${TMPDIR:-/tmp}/fuxx-completes.$$.$RANDOM
  zf_mkdir -m 700 $dir 2>/dev/null || { _FUXX_AS_STATE=off; return 0 }
  _FUXX_AS_DIR=$dir
  if ! command mkfifo -m 600 $dir/req $dir/resp 2>/dev/null; then
    _fuxx_as_unlink_pipes
    _FUXX_AS_STATE=off
    return 0
  fi
  # Any failure below falls through to the cleanup after the block (never `return` from inside).
  {
    _fuxx_as_open rw $dir/req && _FUXX_AS_KEEPFD=$REPLY || ok=0
    if (( ok )); then _fuxx_as_open rw $dir/resp && rs=$REPLY || ok=0; fi
    if (( ok )); then
      # The core must not inherit our spare descriptors, or it would hold its own pipe ends open.
      "$bin" serve <$dir/req >$dir/resp 2>/dev/null {_FUXX_AS_KEEPFD}>&- {rs}>&- &!
      _FUXX_AS_PID=$!
      _fuxx_as_open w $dir/req && _FUXX_AS_WFD=$REPLY || ok=0
    fi
    if (( ok )); then _fuxx_as_open r $dir/resp && _FUXX_AS_RFD=$REPLY || ok=0; fi
    [[ -n $rs ]] && { exec {rs}>&- }
  } 2>/dev/null
  # A plain function, not `-w`, so it also gets the error argument (`hup`, ...).
  if (( ! ok )) || ! zle -F $_FUXX_AS_RFD _fuxx_as_on_response 2>/dev/null; then
    _fuxx_as_stop kill
    return 0
  fi
  _fuxx_as_send hello 1 zsh "$ZSH_VERSION" zsh "${HISTFILE:+${HISTFILE:a}}" autosuggestions
  _fuxx_as_wrap_widgets
  _FUXX_AS_STATE=on
}

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
  if [[ $why == kill && -n $_FUXX_AS_PID ]]; then
    kill $_FUXX_AS_PID 2>/dev/null
  fi
  _fuxx_as_unlink_pipes
  _FUXX_AS_RFD='' _FUXX_AS_WFD='' _FUXX_AS_KEEPFD='' _FUXX_AS_PID=''
  _FUXX_AS_INFLIGHT_ID=''
  _FUXX_AS_STATE=off
  # Inside a handler the line's variables only exist in a widget; elsewhere clear directly.
  zle fuxx-as-clear 2>/dev/null || _fuxx_as_clear
  return 0
}

# _fuxx_as_send <kind> <fields...>: fields joined by 0x1F, a newline inside a field written as 0x1E.
_fuxx_as_send() {
  emulate -L zsh
  [[ -n $_FUXX_AS_WFD ]] || return 1
  local f line nl=$'\n' rs=$'\x1e'   # $'..' isn't expanded inside "${//}", so use variables
  local -a out
  for f in "$@"; do
    out+=( "${f//$nl/$rs}" )
  done
  line="${(pj:\x1f:)out}"
  print -rn -u $_FUXX_AS_WFD -- "$line"$'\n' 2>/dev/null
}

# Asks for a suggestion for $BUFFER; one request in flight at a time.
_fuxx_as_request() {
  emulate -L zsh
  [[ $_FUXX_AS_STATE == on && -n $BUFFER ]] || return 0
  (( $#BUFFER > 1000 )) && return 0
  [[ $BUFFER == *[$'\x1f\x1e']* ]] && return 0
  if [[ -n $_FUXX_AS_INFLIGHT_ID ]]; then
    # The answer handler re-sends when the buffer moved on; a core silent for 2 s is hung.
    if (( EPOCHREALTIME - _FUXX_AS_INFLIGHT_SINCE > 2 )); then
      _fuxx_as_stop kill
    fi
    return 0
  fi
  local id=$_FUXX_AS_NEXT_ID
  _FUXX_AS_NEXT_ID=$(( id + 1 ))
  _FUXX_AS_INFLIGHT_ID=$id
  _FUXX_AS_INFLIGHT_SINCE=$EPOCHREALTIME
  _FUXX_AS_INFLIGHT_BUFFER=$BUFFER
  _fuxx_as_send suggest $id async $CURSOR "$PWD" "$BUFFER" || _fuxx_as_stop kill
  return 0
}

# Called by `zle -F` when the core's answer pipe is readable (or has failed).
_fuxx_as_on_response() {
  emulate -L zsh
  local fd=$1 err=${2-} line nl=$'\n' rs=$'\x1e'
  local -a f
  if [[ -n $err ]] || ! IFS= read -r -u $fd line 2>/dev/null; then
    _fuxx_as_stop gone
    return 0
  fi
  f=( "${(@ps:\x1f:)line}" )
  case $f[1] in
    ready)
      _fuxx_as_unlink_pipes
      [[ $f[2] == 1 ]] || _fuxx_as_stop kill
      ;;
    suggestion)
      [[ $f[2] == $_FUXX_AS_INFLIGHT_ID && -n $_FUXX_AS_INFLIGHT_ID ]] || return 0   # stale: drop
      _FUXX_AS_INFLIGHT_ID=''
      # $BUFFER and POSTDISPLAY only exist inside a widget, so the rest happens in one.
      zle fuxx-as-answer -- "${f[3]//$rs/$nl}" 2>/dev/null
      ;;
  esac
  return 0
}

# Adds our highlight entry for the current POSTDISPLAY (after the buffer's end).
_fuxx_as_highlight() {
  emulate -L zsh
  [[ -n $POSTDISPLAY ]] || return 0
  local hl="$#BUFFER $(( $#BUFFER + $#POSTDISPLAY )) ${FUXX_AUTOSUGGESTIONS_HIGHLIGHT_STYLE:-fg=8}"
  (( _FUXX_AS_MEMO )) && hl+=" memo=fuxx-autosuggestions"
  _FUXX_AS_HL=$hl
  region_highlight+=( "$hl" )
}

# Removes only our own highlight entry: by its `memo=` tag on 5.9+, by its exact string below.
_fuxx_as_unhighlight() {
  emulate -L zsh
  if (( _FUXX_AS_MEMO )); then
    region_highlight=( "${(@)region_highlight:#*memo=fuxx-autosuggestions}" )
  elif [[ -n $_FUXX_AS_HL ]]; then
    region_highlight=( "${(@)region_highlight:#${(b)_FUXX_AS_HL}}" )
  fi
  _FUXX_AS_HL=
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
  fi
}
zle -N fuxx-as-answer _fuxx_as_widget_answer

_fuxx_as_widget_clear() {
  emulate -L zsh
  _fuxx_as_clear
  zle -R
}
zle -N fuxx-as-clear _fuxx_as_widget_clear

# _fuxx_as_w_modify <original widget> [args]: runs the original; if the buffer changed, drops the
# old ghost text and asks again. Our highlight entry is taken off first, because zsh shifts
# region_highlight entries when text is inserted (below 5.9 we can only match our entry by its exact
# string), and put back if the ghost text stays.
_fuxx_as_w_modify() {
  emulate -L zsh
  local orig=$1
  shift
  local -i ret
  # Only without `memo=`: with it our entry is found by tag, and taking it off would draw the ghost
  # text unstyled whenever the widget redraws (isearch, completion lists, `zle -R`).
  (( _FUXX_AS_MEMO )) || _fuxx_as_unhighlight
  zle fuxx-as-orig-$orig -- "$@"
  ret=$?
  if [[ $BUFFER != "$_FUXX_AS_LAST_BUFFER" ]]; then
    _fuxx_as_clear
    _FUXX_AS_LAST_BUFFER=$BUFFER
    _fuxx_as_request
  elif (( ! _FUXX_AS_MEMO )); then
    _fuxx_as_highlight
  fi
  return $ret
}

_fuxx_as_line_init() {
  emulate -L zsh
  _FUXX_AS_LAST_BUFFER=''
}
_fuxx_as_line_finish() {   # ghost text never stays in the scrollback
  emulate -L zsh
  _fuxx_as_clear
  # Forget the request in flight: a handler doesn't run while the command does, so by the next
  # prompt it would look hung (and be killed). Its late answer is dropped as stale by id.
  _FUXX_AS_INFLIGHT_ID='' _FUXX_AS_INFLIGHT_SINCE=0 _FUXX_AS_INFLIGHT_BUFFER=''
}

# Runs once, at the first precmd, so it sees every widget the user's rc and other plugins made.
# Each widget gets its own small function that names the original, so a widget that another
# widget calls with `zle name` finds its original without relying on $WIDGET.
_fuxx_as_wrap_widgets() {
  emulate -L zsh
  local w
  for w in ${(f)"$(zle -la 2>/dev/null)"}; do
    case $w in
      .*|_*|fuxx-as-*|orig-*|zle-*|azhw:*) continue ;;
      beep|run-help|set-local-history|which-command|yank|yank-pop) continue ;;
    esac
    [[ ${widgets[$w]-} == user:_fuxx_as_bound_* ]] && continue   # already ours
    zle -A $w fuxx-as-orig-$w 2>/dev/null || continue
    functions[_fuxx_as_bound_$w]="emulate -L zsh; _fuxx_as_w_modify ${(q)w} \"\$@\""
    zle -N $w _fuxx_as_bound_$w 2>/dev/null
  done
  add-zle-hook-widget line-init _fuxx_as_line_init 2>/dev/null
  add-zle-hook-widget line-finish _fuxx_as_line_finish 2>/dev/null
  return 0
}

_fuxx_as_precmd() {
  emulate -L zsh
  [[ -z $_FUXX_AS_STATE ]] || return 0
  _fuxx_as_start
  return 0
}

_fuxx_as_zshexit() {
  emulate -L zsh
  [[ $_FUXX_AS_STATE == on ]] && _fuxx_as_send bye
  _fuxx_as_stop kill
  return 0
}

add-zsh-hook precmd _fuxx_as_precmd
add-zsh-hook zshexit _fuxx_as_zshexit
