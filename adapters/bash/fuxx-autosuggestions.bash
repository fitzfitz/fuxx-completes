# fuxx-autosuggestions for bash: the best suggestion from history on an explicit key, asked of a
# `fuxx-completes serve` core process. One self-contained file, sourced at the end of ~/.bashrc;
# it finds everything through PATH or variables. Needs bash 4.0+ (plain readline). Design:
# docs/design.md §3, §5.5, §5.8, §6 (bash).
#
# What it does: → and End at the end of the line insert the core's best suggestion (on an empty
# line after a failed command, the fix), waiting at most 50 ms for it; elsewhere they move as
# readline's forward-char and end-of-line do. A `PROMPT_COMMAND` entry reports each command bash
# saved to history. Plain bash can't show grey text as you type (design §6).
#
# Rules this file keeps: it prints nothing (stderr of anything that could complain goes to
# /dev/null); keys that only type are never bound and never wait; a key we bind waits at most
# 50 ms (AGENTS rule 6's stated exception); a write never blocks; every variable is read with a
# default, so `set -u` in the user's shell can't make it complain.
#
# Config, read at use and never required:
#   FUXX_COMPLETES_BIN    the core binary (default: `fuxx-completes` on PATH)
#   FUXX_PLUGIN_DATA      our data folder (as in zsh; design §8)
#
# Only one copy runs in a shell (design §8): a second copy sourced from another path stops
# before defining anything.

[[ $- == *i* ]] || return 0

# The resolved path of this file, without a fork: the folder through `cd -P`, OLDPWD kept.
_fuxx_as_here() {
  local src=${BASH_SOURCE[1]-} dir old=${OLDPWD-} here=$PWD
  [[ -n $src ]] || return 1
  [[ $src == */* ]] && dir=${src%/*} || dir=.
  CDPATH='' builtin cd -P -- "$dir" 2>/dev/null || return 1
  REPLY=$PWD/${src##*/}
  builtin cd -- "$here" 2>/dev/null
  OLDPWD=$old
  return 0
}
if _fuxx_as_here; then
  if [[ -n ${_FUXX_AS_FILE-} && $_FUXX_AS_FILE != "$REPLY" ]]; then
    case " ${_FUXX_AS_SECOND[*]-} " in
      *" $REPLY "*) ;;
      *) _FUXX_AS_SECOND+=("$REPLY") ;;
    esac
    return 0
  fi
  _FUXX_AS_FILE=$REPLY
fi

if [[ -z ${_FUXX_AS_STATE+set} ]]; then
  # '' until the first prompt, then `on` or `off`; why it is off (design §7).
  _FUXX_AS_STATE='' _FUXX_AS_WHY=''
  _FUXX_AS_SECOND=()
  _FUXX_AS_DATA=${FUXX_PLUGIN_DATA-}
  _FUXX_AS_PID='' _FUXX_AS_WFD='' _FUXX_AS_RFD='' _FUXX_AS_KEEPFD='' _FUXX_AS_SPARE_FD=''
  _FUXX_AS_DIR=''
  _FUXX_AS_NEXT_ID=1
  # Bytes the core may not have read yet (a write never blocks: design §5.5), the bytes sent
  # since the newest `suggest`, its id, and whether that one was a ping.
  _FUXX_AS_UNREAD=0 _FUXX_AS_SINCE_SUGGEST=0 _FUXX_AS_LAST_SUGGEST='' _FUXX_AS_PING=''
  # Command reports (§5.8): the newest history number seen and where the next command starts.
  _FUXX_AS_HISTNUM='' _FUXX_AS_HISTTEXT='' _FUXX_AS_CWD=$PWD
  _FUXX_AS_T0='' _FUXX_AS_S0=0
  _FUXX_AS_OWN_FN=0
  _FUXX_AS_BLE=''   # `fed` once our ble.sh source is registered, `atuin` when atuin's is there
  # The host channel (design §5.5): 1 in host mode, why it is off when asked for, messages
  # waiting to be written, the last `buffer` id sent.
  _FUXX_AS_HOST=0 _FUXX_AS_HOST_WHY='' _FUXX_AS_HOST_ID=''
  _FUXX_AS_HOST_Q=()
  if (( BASH_VERSINFO[0] < 4 )); then
    _FUXX_AS_STATE=off _FUXX_AS_WHY=bash-too-old
  else
    # "<keymap> <key>" -> `bound` or `kept:<binding>`.
    declare -gA _FUXX_AS_KEYS 2>/dev/null || declare -A _FUXX_AS_KEYS
  fi
fi

# The bash version as one number, 4.0 -> 400, 5.3 -> 503.
_FUXX_AS_V=$(( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] ))

# ---- Characters and bytes (bash before 5.0 counts READLINE_POINT in bytes) ----

# REPLY = the byte length of $1.
_fuxx_as_bytes() {
  local LC_ALL=C
  REPLY=${#1}
}

# REPLY = the first $2 bytes of $1.
_fuxx_as_byte_prefix() {
  local LC_ALL=C
  REPLY=${1:0:$2}
}

# REPLY = READLINE_POINT in characters.
_fuxx_as_point_chars() {
  if (( _FUXX_AS_V >= 500 )); then
    REPLY=$READLINE_POINT
  else
    _fuxx_as_byte_prefix "$READLINE_LINE" "$READLINE_POINT"
    REPLY=${#REPLY}
  fi
}

# Sets READLINE_POINT to the character offset $1.
_fuxx_as_set_point() {
  if (( _FUXX_AS_V >= 500 )); then
    READLINE_POINT=$1
  else
    _fuxx_as_bytes "${READLINE_LINE:0:$1}"
    READLINE_POINT=$REPLY
  fi
}

# ---- The core and its pipes (design §5.5) ----

_fuxx_as_unlink_pipes() {
  [[ -n $_FUXX_AS_DIR ]] || return 0
  command rm -f -- "$_FUXX_AS_DIR/req" "$_FUXX_AS_DIR/resp" 2>/dev/null
  command rmdir -- "$_FUXX_AS_DIR" 2>/dev/null
  _FUXX_AS_DIR=''
  return 0
}

# _fuxx_as_close <fd>
_fuxx_as_close() {
  [[ -n ${1-} ]] && eval "exec $1>&-" 2>/dev/null
  return 0
}

# _fuxx_as_open <mode: < > or <>> <file>: opens a descriptor; its number comes back in REPLY.
# `exec {var}` exists from bash 4.1; on 4.0 the first free descriptor from 10 up is used.
_fuxx_as_open() {
  local mode=$1 file=$2 fd
  if (( _FUXX_AS_V >= 401 )); then
    eval "exec {fd}$mode\"\$file\"" 2>/dev/null || return 1
  else
    fd=10
    # In use when it can be duplicated (bash's own `-e /dev/fd/N` says no for an open one).
    while { true >&"$fd"; } 2>/dev/null; do
      (( fd++ ))
      (( fd < 250 )) || return 1
    done
    eval "exec $fd$mode\"\$file\"" 2>/dev/null || return 1
  fi
  REPLY=$fd
}

# Starts the core on two named pipes, as the zsh adapter does: both pipes are opened read-write
# first so no open blocks; the core is started on them from `/` (it opens them by path); then
# the shell opens its write end of `req` and read end of `resp`. The read-write holder on `resp`
# and the pipes' names stay until `ready` is read (earlier would race the core's own opens).
_fuxx_as_start() {
  local bin=${FUXX_COMPLETES_BIN:-} dir ok=1
  # Host mode (design §5.5): the terminal asked with FUXX_COMPLETES_HOST=1 and no multiplexer
  # sits between. No core; `hello` waits for the next prompt or key, so nothing is written before
  # the first prompt.
  if [[ ${FUXX_COMPLETES_HOST-} == 1 ]]; then
    if [[ -n ${TMUX-} || -n ${STY-} ]]; then
      _FUXX_AS_HOST_WHY=tmux
    else
      local hist=${HISTFILE-}
      [[ -n $hist && $hist != /* ]] && hist=$PWD/$hist
      _FUXX_AS_HOST=1 _FUXX_AS_STATE=on
      _fuxx_as_send hello 1 bash "$BASH_VERSION" bash "$hist" autosuggestions
      if (( _FUXX_AS_V < 402 )); then
        read -r _FUXX_AS_T0 < <(command date +%s 2>/dev/null)
        _FUXX_AS_S0=$SECONDS
      fi
      if [[ -z ${BLE_VERSION-} ]]; then
        _fuxx_as_private_dir && _fuxx_as_bind_keys
        _fuxx_as_unlink_pipes
      fi
      return 0
    fi
  fi
  # No command substitution anywhere on the prompt's path: in bash 4.0 one in PROMPT_COMMAND
  # keeps the first prompt from being drawn (checked 2026-11-23). PATH lookup through `hash`.
  if [[ -z $bin ]] && hash fuxx-completes 2>/dev/null; then
    bin=${BASH_CMDS[fuxx-completes]-}
  fi
  if [[ -z $bin || ! -x $bin || -d $bin ]]; then
    _FUXX_AS_STATE=off _FUXX_AS_WHY=no-binary
    return 0
  fi
  dir=${TMPDIR:-/tmp}
  dir=${dir%/}/fuxx-completes.$$.$RANDOM
  if ! command mkdir -m 700 -- "$dir" 2>/dev/null; then
    _FUXX_AS_STATE=off _FUXX_AS_WHY=start-failed
    return 0
  fi
  _FUXX_AS_DIR=$dir
  if ! command mkfifo -m 600 -- "$dir/req" "$dir/resp" 2>/dev/null; then
    _fuxx_as_unlink_pipes
    _FUXX_AS_STATE=off _FUXX_AS_WHY=start-failed
    return 0
  fi
  _fuxx_as_open '<>' "$dir/req" && _FUXX_AS_KEEPFD=$REPLY || ok=0
  if (( ok )); then _fuxx_as_open '<>' "$dir/resp" && _FUXX_AS_SPARE_FD=$REPLY || ok=0; fi
  if (( ok )); then
    # A subshell that becomes the core: no `[1] pid` line (stderr of the group goes nowhere) and,
    # disowned, no `Done` line. It doesn't keep our spare descriptors, starts from `/` so it
    # never keeps the shell's folder busy, and gets the data folder fuxx named, or none.
    {
      (
        builtin cd / 2>/dev/null
        _fuxx_as_close "$_FUXX_AS_KEEPFD"
        _fuxx_as_close "$_FUXX_AS_SPARE_FD"
        if [[ -n $_FUXX_AS_DATA ]]; then export FUXX_PLUGIN_DATA=$_FUXX_AS_DATA; else unset FUXX_PLUGIN_DATA; fi
        exec "$bin" serve <"$dir/req" >"$dir/resp" 2>/dev/null
      ) &
    } 2>/dev/null
    _FUXX_AS_PID=$!
    disown "$_FUXX_AS_PID" 2>/dev/null
    _fuxx_as_open '>' "$dir/req" && _FUXX_AS_WFD=$REPLY || ok=0
  fi
  if (( ok )); then _fuxx_as_open '<' "$dir/resp" && _FUXX_AS_RFD=$REPLY || ok=0; fi
  if (( ! ok )); then
    _fuxx_as_stop kill start-failed
    return 0
  fi
  local hist=${HISTFILE-}
  [[ -n $hist && $hist != /* ]] && hist=$PWD/$hist
  _FUXX_AS_STATE=on
  if ! _fuxx_as_send hello 1 bash "$BASH_VERSION" bash "$hist" autosuggestions; then
    _fuxx_as_stop kill write-failed
    return 0
  fi
  # Time without a fork per prompt: EPOCHSECONDS (5.0+), printf %(%s)T (4.2+), else one `date`.
  if (( _FUXX_AS_V < 402 )); then
    read -r _FUXX_AS_T0 < <(command date +%s 2>/dev/null)
    _FUXX_AS_S0=$SECONDS
  fi
  # Under ble.sh its own editor reads the keys: our source feeds its grey text instead.
  [[ -n ${BLE_VERSION-} ]] || _fuxx_as_bind_keys
}

# A private folder (`mkdir -m 700`) in $TMPDIR, in _FUXX_AS_DIR.
_fuxx_as_private_dir() {
  local dir=${TMPDIR:-/tmp}
  dir=${dir%/}/fuxx-completes.$$.$RANDOM
  command mkdir -m 700 -- "$dir" 2>/dev/null || return 1
  _FUXX_AS_DIR=$dir
}

# `_fuxx_as_stop kill [why]`: the core may still be alive, so end it. `_fuxx_as_stop gone [why]`:
# it is already gone (EOF), and its pid may belong to another process by now.
_fuxx_as_stop() {
  _fuxx_as_close "$_FUXX_AS_RFD"
  _fuxx_as_close "$_FUXX_AS_WFD"
  _fuxx_as_close "$_FUXX_AS_KEEPFD"
  _fuxx_as_close "$_FUXX_AS_SPARE_FD"
  if [[ ${1-} == kill && -n $_FUXX_AS_PID ]]; then
    builtin kill "$_FUXX_AS_PID" 2>/dev/null
  fi
  _fuxx_as_unlink_pipes
  _FUXX_AS_RFD='' _FUXX_AS_WFD='' _FUXX_AS_KEEPFD='' _FUXX_AS_SPARE_FD='' _FUXX_AS_PID=''
  _FUXX_AS_UNREAD=0 _FUXX_AS_SINCE_SUGGEST=0 _FUXX_AS_LAST_SUGGEST='' _FUXX_AS_PING=''
  _FUXX_AS_STATE=off
  [[ -n ${2-} ]] && _FUXX_AS_WHY=$2
  return 0
}

# _fuxx_as_send <kind> <fields...>: fields joined by 0x1F, a newline in a field written as 0x1E.
# bash can't open a descriptor non-blocking, so the bytes the core may not have read yet are
# counted (an answer to the newest `suggest` shows it read everything up to it) and a write that
# could fill the pipe (a FIFO holds about 8 KiB on macOS) is refused: the caller stops the core.
_fuxx_as_send() {
  if (( _FUXX_AS_HOST )); then
    _fuxx_as_host_queue "$@"
    return 0
  fi
  [[ -n $_FUXX_AS_WFD ]] || return 1
  local f line='' sep='' nl=$'\n' rs=$'\x1e' us=$'\x1f'
  for f in "$@"; do
    line+=$sep${f//$nl/$rs}
    sep=$us
  done
  line+=$nl
  _fuxx_as_bytes "$line"
  local len=$REPLY
  (( _FUXX_AS_UNREAD > 0 && _FUXX_AS_UNREAD + len > 6144 )) && return 1
  printf '%s' "$line" 2>/dev/null >&$_FUXX_AS_WFD || return 1
  (( _FUXX_AS_UNREAD += len ))
  if [[ $1 == suggest ]]; then
    _FUXX_AS_SINCE_SUGGEST=0 _FUXX_AS_LAST_SUGGEST=$2
  else
    (( _FUXX_AS_SINCE_SUGGEST += len ))
  fi
  return 0
}

# Handles one line from the core; _FUXX_AS_RID/_FUXX_AS_RTEXT get a `suggestion`'s id and text.
_fuxx_as_got() {
  local line=$1 us=$'\x1f' kind rest
  kind=${line%%"$us"*}
  rest=${line#*"$us"}
  _FUXX_AS_RID='' _FUXX_AS_RTEXT=''
  case $kind in
    ready)
      _fuxx_as_unlink_pipes
      _fuxx_as_close "$_FUXX_AS_SPARE_FD"
      _FUXX_AS_SPARE_FD=''
      [[ $rest == 1 ]] || { _fuxx_as_stop kill protocol; return 1; }
      ;;
    suggestion)
      _FUXX_AS_RID=${rest%%"$us"*}
      _FUXX_AS_RTEXT=${rest#*"$us"}
      [[ $rest == *"$us"* ]] || _FUXX_AS_RTEXT=''
      if [[ $_FUXX_AS_RID == "$_FUXX_AS_LAST_SUGGEST" ]]; then
        # The core has read everything up to the newest `suggest`.
        _FUXX_AS_UNREAD=$_FUXX_AS_SINCE_SUGGEST
        _FUXX_AS_PING=''
      fi
      ;;
  esac
  return 0
}

# Reads every line the core already sent, without waiting. EOF means the core is gone.
_fuxx_as_drain() {
  local line
  while [[ -n $_FUXX_AS_RFD ]] && read -t 0 -u "$_FUXX_AS_RFD" 2>/dev/null; do
    if ! IFS= read -r -u "$_FUXX_AS_RFD" line 2>/dev/null; then
      _fuxx_as_stop gone exited
      return 1
    fi
    _fuxx_as_got "$line" || return 1
  done
  [[ $_FUXX_AS_STATE == on ]]
}

# REPLY = Unix time in seconds, without a fork.
_fuxx_as_now() {
  if [[ -n ${EPOCHSECONDS-} ]]; then
    REPLY=$EPOCHSECONDS
  elif (( _FUXX_AS_V >= 402 )); then
    printf -v REPLY '%(%s)T' -1
  else
    REPLY=$(( ${_FUXX_AS_T0:-0} + SECONDS - _FUXX_AS_S0 ))
  fi
}

# _fuxx_as_sync_suggest <buffer> <cursor in characters> [ble]: asks for the suggestion and waits
# at most 50 ms (design §3): REPLY gets the text after the buffer, or the function returns 1 (no
# answer in time, none, or the core is gone). A late answer is dropped by its id at the next
# drain. With `ble` (ble.sh's idle task, AGENTS rule 6's second stated wait) it checks every 5 ms
# and returns 148 the moment a key is waiting.
_fuxx_as_sync_suggest() {
  local buffer=$1 cursor=$2 ble=${3-}
  REPLY=''
  [[ $_FUXX_AS_STATE == on ]] || return 1
  _fuxx_as_drain || return 1
  (( ${#buffer} > 1000 )) && return 1
  [[ $buffer == *[$'\x1f\x1e']* ]] && return 1
  local id=$_FUXX_AS_NEXT_ID
  _FUXX_AS_NEXT_ID=$(( id + 1 ))
  local -a f=(suggest "$id" sync "$cursor" "$PWD" "$buffer")
  # CDPATH goes along, one folder per line (design §5.4); left off when `cd`/`pushd` is an
  # alias or function, `cdable_vars` is on, or a folder holds a control character.
  local cdp=${CDPATH-} nl=$'\n'
  if [[ $cdp != *[[:cntrl:]]* ]] && ! shopt -q cdable_vars 2>/dev/null \
     && _fuxx_as_plain cd && _fuxx_as_plain pushd; then
    f+=("${cdp//:/$nl}")
  fi
  _fuxx_as_send "${f[@]}" || { _fuxx_as_stop kill write-failed; return 1; }
  # Read until this request's answer or 50 ms. bash 5.0+ has a clock; 4.x waits once (or, under
  # ble.sh, ten 5 ms slices), then only takes lines already there.
  local line t now deadline='' st slices=10
  if [[ -n ${EPOCHREALTIME-} ]]; then
    now=${EPOCHREALTIME//[!0-9]/}
    deadline=$(( 10#$now + 50000 ))
  fi
  t=0.05
  [[ -n $ble ]] && t=0.005
  while :; do
    if [[ -n $ble ]] && ble/decode/has-input; then
      return 148
    fi
    IFS= read -r -t "$t" -u "$_FUXX_AS_RFD" line 2>/dev/null
    st=$?
    if (( st > 128 )); then
      if [[ -n $ble ]]; then
        # One 5 ms slice is over: go on until 50 ms.
        if [[ -n $deadline ]]; then
          now=${EPOCHREALTIME//[!0-9]/}
          (( 10#$now >= deadline )) && return 1
        else
          (( --slices > 0 )) || return 1
        fi
        continue
      fi
      return 1   # timed out
    elif (( st != 0 )); then
      _fuxx_as_stop gone exited
      return 1
    fi
    _fuxx_as_got "$line" || return 1
    if [[ $_FUXX_AS_RID == "$id" ]]; then
      REPLY=${_FUXX_AS_RTEXT//$'\x1e'/$nl}
      [[ -n $REPLY ]]
      return
    fi
    if [[ -n $deadline ]]; then
      now=${EPOCHREALTIME//[!0-9]/}
      (( 10#$now >= deadline )) && return 1
      [[ -n $ble ]] || printf -v t '0.%06d' $(( deadline - 10#$now ))
    elif [[ -z $ble ]]; then
      read -t 0 -u "$_FUXX_AS_RFD" 2>/dev/null || return 1
      t=0.001
    fi
  done
}

# True when `$1` runs bash's own builtin (no alias or function of that name).
_fuxx_as_plain() {
  [[ -z ${BASH_ALIASES[$1]+set} ]] && ! declare -F "$1" >/dev/null 2>&1
}

# ---- Keys (design §6 bash) ----

# → (`\e[C`, `\eOC`): at the end of the line, inserts the suggestion; elsewhere moves one
# character, as forward-char does.
_fuxx_as_key_right() {
  local len=${#READLINE_LINE}
  _fuxx_as_point_chars
  if (( REPLY < len )); then
    _fuxx_as_set_point $(( REPLY + 1 ))
  else
    _fuxx_as_accept
  fi
}

# End (`\e[F`, `\eOF`, `\e[4~`): at the end of the line, inserts the suggestion; elsewhere moves
# to the end, as end-of-line does.
_fuxx_as_key_end() {
  local len=${#READLINE_LINE}
  _fuxx_as_point_chars
  if (( REPLY < len )); then
    _fuxx_as_set_point "$len"
  else
    _fuxx_as_accept
  fi
}

_fuxx_as_accept() {
  if (( _FUXX_AS_HOST )); then
    # The host draws and accepts (design §5.5): the key only tells it the line.
    _fuxx_as_host_flush
    _fuxx_as_host_forward
    return 0
  fi
  _fuxx_as_point_chars
  _fuxx_as_sync_suggest "$READLINE_LINE" "$REPLY" || return 0
  READLINE_LINE+=$REPLY
  _fuxx_as_set_point "${#READLINE_LINE}"
}

# What `<keymap> <sequence>` was bound to when the keys were set up: REPLY = a function name,
# `"<macro>"`, `-x:<cmd>`, or '' when unbound. bash 4.0 lists ESC sequences in meta form
# (`\M-[C`). The listings come from `_fuxx_as_bind_keys`, in files: `bind` run in a subshell
# initialises readline there, and on bash 4.0 the shell's next prompt is then not drawn.
_fuxx_as_binding() {
  local km=$1 seq=$2 alt line tag
  alt=${seq/#\\e/\\M-}
  REPLY=''
  for tag in p x; do
    [[ -f $_FUXX_AS_DIR/keys-$km-$tag ]] || continue
    while IFS= read -r line; do
      case $line in
        "\"$seq\": "* | "\"$alt\": "*)
          REPLY=${line#*\": }
          [[ $tag == x ]] && REPLY=-x:$REPLY
          return 0 ;;
      esac
    done < "$_FUXX_AS_DIR/keys-$km-$tag"
  done
  return 0
}

# The private two-key sequences that bash before 4.3 needs (it can't `bind -x` a longer one):
# the key becomes a macro typing one of these.
_FUXX_AS_PRIV_RIGHT='\C-x\C-^'
_FUXX_AS_PRIV_END='\C-x\C-]'
_FUXX_AS_PRIV_HOST='\C-x\C-z'

# _fuxx_as_bind_key <keymap> <sequence> <default function> <our function> <private sequence>:
# binds only a key still on its default (or unbound), and records what happened.
_fuxx_as_bind_key() {
  local km=$1 seq=$2 def=$3 fn=$4 priv=$5 k="$1 $2"
  _fuxx_as_binding "$km" "$seq"
  if [[ -n $REPLY && ( -z $def || $REPLY != "$def" ) ]]; then
    _FUXX_AS_KEYS[$k]=kept:$REPLY
    return 0
  fi
  if (( _FUXX_AS_V >= 403 )); then
    bind -m "$km" -x "\"$seq\": $fn" 2>/dev/null
  else
    _fuxx_as_binding "$km" "$priv"
    if [[ -n $REPLY && $REPLY != -x:* ]]; then
      _FUXX_AS_KEYS[$k]=kept:private-sequence-taken
      return 0
    fi
    bind -m "$km" -x "\"$priv\": $fn" 2>/dev/null
    bind -m "$km" "\"$seq\": \"$priv\"" 2>/dev/null
  fi
  _FUXX_AS_KEYS[$k]=bound
}

# Binds the keys in emacs and vi-insert (vi command mode is left alone). Before bash 4.4 every
# `bind -x` key redraws the prompt on a new line (bash CHANGES, 4.4-alpha ggg), so there → is
# left to readline and only End inserts.
_fuxx_as_bind_keys() {
  local km seq
  [[ -n $_FUXX_AS_DIR ]] || return 0
  for km in emacs vi-insert; do
    { bind -m "$km" -p; bind -m "$km" -s; } >"$_FUXX_AS_DIR/keys-$km-p" 2>/dev/null
    (( _FUXX_AS_V >= 403 )) && bind -m "$km" -X >"$_FUXX_AS_DIR/keys-$km-x" 2>/dev/null
    if (( _FUXX_AS_V >= 404 )); then
      for seq in '\e[C' '\eOC'; do
        _fuxx_as_bind_key "$km" "$seq" forward-char _fuxx_as_key_right "$_FUXX_AS_PRIV_RIGHT"
      done
    fi
    for seq in '\e[F' '\eOF' '\e[4~'; do
      _fuxx_as_bind_key "$km" "$seq" end-of-line _fuxx_as_key_end "$_FUXX_AS_PRIV_END"
    done
    # The host's accept, a CSI no keyboard sends, bound only where it is free (design §5.5).
    if (( _FUXX_AS_HOST )); then
      _fuxx_as_bind_key "$km" '\e[7701~' '' _fuxx_as_host_accept "$_FUXX_AS_PRIV_HOST"
    fi
    command rm -f -- "$_FUXX_AS_DIR/keys-$km-p" "$_FUXX_AS_DIR/keys-$km-x" 2>/dev/null
  done
}

# ---- The prompt (design §5.8, §9) ----

_fuxx_as_conflict() {
  REPLY=''
  if [[ ${TERM_PROGRAM-} == WarpTerminal ]]; then
    REPLY=conflict:warp
  elif [[ -n ${BLE_VERSION-} ]] && (( ${_ble_version:-0} < 400 )); then
    REPLY=conflict:ble.sh   # ble.sh before 0.4 has no auto-complete source list to feed
  elif [[ $_FUXX_AS_BLE == atuin ]]; then
    REPLY=conflict:atuin
  fi
}

# The `PROMPT_COMMAND` entry, which goes first so `$?` is the command's: it returns that status
# again, so the entries after it see it too. Starts the core at the first prompt (after the rc
# file); at later prompts reports the command bash saved, when `history 1` shows a new number
# (bash has applied HISTCONTROL and HISTIGNORE by then; an empty Enter, a hidden line and an
# ignored duplicate add none).
_fuxx_as_prompt() {
  local st=$?
  _fuxx_as_prompt_work "$st"
  return "$st"
}

_fuxx_as_prompt_work() {
  local st=$1
  [[ $_FUXX_AS_STATE == off ]] && return 0
  # Coexistence (design §9), checked at every prompt.
  _fuxx_as_conflict
  if [[ -n $REPLY ]]; then
    if [[ $_FUXX_AS_STATE == on ]]; then
      _fuxx_as_stop kill "$REPLY"
    else
      _FUXX_AS_STATE=off _FUXX_AS_WHY=$REPLY
    fi
    return 0
  fi
  local n entry cwd=$_FUXX_AS_CWD
  _FUXX_AS_CWD=$PWD
  _fuxx_as_newest
  n=$_FUXX_AS_N entry=$_FUXX_AS_E
  if [[ -z $_FUXX_AS_STATE ]]; then
    _FUXX_AS_HISTNUM=$n _FUXX_AS_HISTTEXT=$entry
    _fuxx_as_start
    return 0
  fi
  [[ $_FUXX_AS_STATE == on ]] || return 0
  if (( _FUXX_AS_HOST )); then
    # The line was run or abandoned; then what is waiting (hello), then this command's report.
    _fuxx_as_host_osc clear
    _FUXX_AS_HOST_Q+=("$REPLY")
    _fuxx_as_prompt_report "$st" "$n" "$entry" "$cwd"
    _fuxx_as_host_flush
    return 0
  fi
  _fuxx_as_drain || return 0
  _fuxx_as_prompt_report "$st" "$n" "$entry" "$cwd" || return 0
  # Reports get no answer, so a ping (an empty `suggest`) now and then shows the core reads
  # them; without one, the byte count above would stop a healthy core after a few dozen reports.
  if [[ -z $_FUXX_AS_PING ]] && (( _FUXX_AS_UNREAD > 2048 )); then
    local id=$_FUXX_AS_NEXT_ID
    _FUXX_AS_NEXT_ID=$(( id + 1 ))
    _fuxx_as_send suggest "$id" async 0 / '' || { _fuxx_as_stop kill write-failed; return 0; }
    _FUXX_AS_PING=$id
  fi
  return 0
}

# _FUXX_AS_N, _FUXX_AS_E = the newest history entry's number and text ('' when none). `history 1`
# prints "%5d%c %s": the number, `*` when the entry was edited, the entry. Read through process
# substitution: a command substitution here keeps bash 4.0's first prompt from being drawn.
_fuxx_as_newest() {
  local h='' l re='^ *([0-9]+)[ *] (.*)$'
  while IFS= read -r l || [[ -n $l ]]; do
    h+=${h:+$'\n'}$l
  done < <(unset HISTTIMEFORMAT; builtin history 1 2>/dev/null)
  if [[ $h =~ $re ]]; then
    _FUXX_AS_N=${BASH_REMATCH[1]} _FUXX_AS_E=${BASH_REMATCH[2]}
  else
    _FUXX_AS_N='' _FUXX_AS_E=''
  fi
}

# The last PROMPT_COMMAND entry: takes the newest entry again after the user's own entries ran,
# so a list they reloaded or renumbered (`history -a; history -c; history -r`, `history -n`) is
# the baseline, not a command that ran here (fix after the M2 review). Hands `$?` on.
_fuxx_as_prompt_end() {
  local st=$?
  if [[ $_FUXX_AS_STATE == on ]]; then
    _fuxx_as_newest
    _FUXX_AS_HISTNUM=$_FUXX_AS_N _FUXX_AS_HISTTEXT=$_FUXX_AS_E
  fi
  return "$st"
}

# _fuxx_as_prompt_report <status> <history number> <entry> <cwd>: sends `ran` and `prev` when a
# command was saved since the last prompt: the newest number is one more than the baseline, or
# the same with another text (`erasedups` moved an older copy to the end). An unchanged entry (an
# empty Enter, a hidden line, an ignored repeat) and any other jump (a reload, the first load of
# the file under ble.sh) send nothing. Returns 1 when the core had to be stopped.
_fuxx_as_prompt_report() {
  local st=$1 n=$2 entry=$3 cwd=$4 base=$_FUXX_AS_HISTNUM text=$_FUXX_AS_HISTTEXT
  _FUXX_AS_HISTNUM=$n _FUXX_AS_HISTTEXT=$entry
  [[ -n $n ]] || return 0
  if [[ -z $base ]]; then
    (( n == 1 )) || return 0
  elif (( n == base )); then
    [[ $entry != "$text" ]] || return 0
  else
    (( n == base + 1 )) || return 0
  fi
  (( ${#entry} > 1000 )) && return 0
  [[ $entry == *[$'\x1f\x1e']* ]] && return 0   # the wire can't carry them
  _fuxx_as_now
  _fuxx_as_send ran "$cwd" "$st" "$REPLY" "$entry" \
    && _fuxx_as_send prev "$entry" "$st" \
    || { _fuxx_as_stop kill write-failed; return 1; }
  return 0
}

# Our entry goes first in PROMPT_COMMAND (an array from bash 5.1, else a string) and
# `_fuxx_as_prompt_end` last, once.
_fuxx_as_hook_prompt() {
  [[ $_FUXX_AS_STATE == off ]] && return 0
  local decl
  decl=$(declare -p PROMPT_COMMAND 2>/dev/null)
  if [[ $decl == 'declare -a'* ]]; then
    local e
    for e in "${PROMPT_COMMAND[@]}"; do
      [[ $e == _fuxx_as_prompt ]] && return 0
    done
    PROMPT_COMMAND=(_fuxx_as_prompt "${PROMPT_COMMAND[@]}" _fuxx_as_prompt_end)
  else
    case $'\n'${PROMPT_COMMAND-}$'\n' in
      *$'\n'_fuxx_as_prompt$'\n'*) return 0 ;;
    esac
    PROMPT_COMMAND=_fuxx_as_prompt${PROMPT_COMMAND:+$'\n'$PROMPT_COMMAND}$'\n'_fuxx_as_prompt_end
  fi
}
_fuxx_as_hook_prompt

# ---- The host channel (design §5.5, M2) ----

# REPLY = base64 (RFC 4648, padded) of $1's bytes, in pure bash (no fork per key).
_fuxx_as_b64() {
  local LC_ALL=C s=$1 t=ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/
  local out='' i a b d n len=${#s}
  for (( i = 0; i < len; i += 3 )); do
    printf -v a '%d' "'${s:i:1}"
    (( a < 0 )) && (( a += 256 ))
    b=0 d=0
    if (( i + 1 < len )); then printf -v b '%d' "'${s:i+1:1}"; (( b < 0 )) && (( b += 256 )); fi
    if (( i + 2 < len )); then printf -v d '%d' "'${s:i+2:1}"; (( d < 0 )) && (( d += 256 )); fi
    n=$(( a << 16 | b << 8 | d ))
    out+=${t:n>>18&63:1}${t:n>>12&63:1}
    if (( i + 1 < len )); then out+=${t:n>>6&63:1}; else out+='='; fi
    if (( i + 2 < len )); then out+=${t:n&63:1}; else out+='='; fi
  done
  REPLY=$out
}

# REPLY = the bytes $1 (padded base64) encodes; returns 1 when it isn't valid base64.
_fuxx_as_unb64() {
  local LC_ALL=C s=$1 esc='' i k c x v n pad re='^[A-Za-z0-9+/]*={0,2}$'
  (( ${#s} % 4 == 0 )) || return 1
  [[ $s =~ $re ]] || return 1
  for (( i = 0; i < ${#s}; i += 4 )); do
    n=0 pad=0
    for (( k = 0; k < 4; k++ )); do
      c=${s:i+k:1}
      printf -v x '%d' "'$c"
      if (( x == 61 )); then (( pad++ )); v=0
      elif (( x >= 65 && x <= 90 )); then v=$(( x - 65 ))
      elif (( x >= 97 && x <= 122 )); then v=$(( x - 71 ))
      elif (( x >= 48 && x <= 57 )); then v=$(( x + 4 ))
      elif (( x == 43 )); then v=62
      else v=63
      fi
      n=$(( n << 6 | v ))
    done
    (( pad && i + 4 < ${#s} )) && return 1
    printf -v c '\\x%02x' $(( n >> 16 & 255 )); esc+=$c
    if (( pad < 2 )); then printf -v c '\\x%02x' $(( n >> 8 & 255 )); esc+=$c; fi
    if (( pad < 1 )); then printf -v c '\\x%02x' $(( n & 255 )); esc+=$c; fi
  done
  printf -v REPLY '%b' "$esc"
}

# _fuxx_as_host_osc <message> [n:<number> | s:<text> | '']...: REPLY = its OSC 7701.
_fuxx_as_host_osc() {
  local out=$'\e]7701;fuxx-completes;1;'$1 f
  shift
  for f in ${1+"$@"}; do   # (an empty "$@" is an error under set -u before bash 4.4)
    case $f in
      n:*) out+=";${f#n:}" ;;
      *) _fuxx_as_b64 "${f#s:}"; out+=";$REPLY" ;;
    esac
  done
  REPLY=$out$'\a'
}

# What `_fuxx_as_send` would tell a core, queued for the host: hello, ran, prev.
_fuxx_as_host_queue() {
  case $1 in
    hello) _fuxx_as_host_osc hello "s:$3" "s:$4" "s:$5" "s:$6" "s:$7" ;;
    ran) _fuxx_as_host_osc ran "s:$2" "n:$3" "n:$4" "s:$5" ;;
    prev) _fuxx_as_host_osc prev "s:$2" "n:$3" ;;
    *) return 0 ;;
  esac
  _FUXX_AS_HOST_Q+=("$REPLY")
}

# Writes what is queued to the terminal (never stdout).
_fuxx_as_host_flush() {
  (( ${#_FUXX_AS_HOST_Q[@]} )) || return 0
  local m all=''
  for m in "${_FUXX_AS_HOST_Q[@]}"; do all+=$m; done
  _FUXX_AS_HOST_Q=()
  printf '%s' "$all" 2>/dev/null >/dev/tty
  return 0
}

# Sends the line at an explicit key (readline has no buffer-change hook). Over 1000 characters,
# or holding 0x1F/0x1E, it is never sent.
_fuxx_as_host_forward() {
  local line=$READLINE_LINE id
  (( ${#line} > 1000 )) && return 0
  [[ $line == *[$'\x1f\x1e']* ]] && return 0
  id=$_FUXX_AS_NEXT_ID
  _FUXX_AS_NEXT_ID=$(( id + 1 ))
  _FUXX_AS_HOST_ID=$id
  _fuxx_as_point_chars
  _fuxx_as_host_osc buffer "n:$id" "n:$REPLY" "s:$PWD" "s:$line"
  printf '%s' "$REPLY" 2>/dev/null >/dev/tty
}

# `ESC [ 7701 ~` from the host, then base64 of `<id> US <buffer> US <cursor>` and BEL: reads the
# rest within 50 ms and applies it only when the id is the last `buffer` sent; anything else is
# read and dropped, so nothing stray is typed.
_fuxx_as_host_accept() {
  local payload text id cur
  IFS= read -r -s -t 0.05 -d $'\a' payload 2>/dev/null || return 0
  (( ${#payload} <= 8192 )) || return 0
  _fuxx_as_unb64 "$payload" || return 0
  text=$REPLY
  [[ $text == *$'\x1f'*$'\x1f'* ]] || return 0
  id=${text%%$'\x1f'*} cur=${text##*$'\x1f'}
  text=${text#*$'\x1f'}
  text=${text%$'\x1f'*}
  [[ $id =~ ^[0-9]+$ && $cur =~ ^[0-9]+$ && $id == "$_FUXX_AS_HOST_ID" ]] || return 0
  READLINE_LINE=$text
  (( cur > ${#text} )) && cur=${#text}
  _fuxx_as_set_point "$cur"
}

# ---- ble.sh (the user's decision, 2026-11-23: we feed its grey text; design §6 bash) ----

# ble.sh's auto-complete calls its sources in `_ble_complete_auto_source` order when the line has
# been idle; ours goes first, so ble.sh draws our best suggestion as its own grey text (→, End,
# ⌃E and ⌃F take it, as with ble.sh's own). The interface is ble.sh's internal one (atuin uses the
# same). Returns 0 when shown, 1 for nothing (ble.sh then tries its own sources), 148 when a key
# arrived meanwhile.
function ble/complete/auto-complete/source:fuxx-autosuggestions {
  [[ $_FUXX_AS_STATE == on && $_FUXX_AS_BLE == fed ]] || return 1
  # In host mode there is no core to ask: the terminal draws (fix after the M2 review).
  (( _FUXX_AS_HOST )) && return 1
  local s
  for s in "${_ble_complete_auto_source[@]}"; do
    if [[ $s == atuin-history ]]; then
      # atuin feeds ble.sh too: two sources for one line, so we step aside (design §9).
      _FUXX_AS_BLE=atuin
      _fuxx_as_stop kill conflict:atuin
      return 1
    fi
  done
  (( _ble_edit_ind == ${#_ble_edit_str} )) || return 1
  _fuxx_as_sync_suggest "$_ble_edit_str" "$_ble_edit_ind" ble
  local ext=$?
  (( ext == 0 )) || return "$ext"
  ble/complete/auto-complete/enter h 0 "$REPLY" '' "$_ble_edit_str$REPLY"
}

# Registers the source once ble.sh has loaded its completion module (bash 4.0+, ble.sh 0.4+).
_fuxx_as_hook_blesh() {
  [[ -n ${BLE_VERSION-} && $_FUXX_AS_STATE != off && -z ${_FUXX_AS_BLE-} ]] || return 0
  (( ${_ble_version:-0} >= 400 )) || return 0
  _FUXX_AS_BLE=fed
  ble/util/import/eval-after-load core-complete \
    'ble/array#unshift _ble_complete_auto_source fuxx-autosuggestions' 2>/dev/null
}
_fuxx_as_hook_blesh

# ---- doctor and why (design §7) ----

# What this shell knows for `doctor`, one `name=value` per line.
_fuxx_as_facts() {
  local k hist=${HISTFILE-}
  [[ -n $hist && $hist != /* ]] && hist=$PWD/$hist
  printf '%s\n' shell=bash "version=$BASH_VERSION" "state=${_FUXX_AS_STATE:-starting}" \
    "why=${_FUXX_AS_WHY-}" "file=${_FUXX_AS_FILE-}"
  for k in ${_FUXX_AS_SECOND[@]+"${_FUXX_AS_SECOND[@]}"}; do printf 'second=%s\n' "$k"; done
  printf 'data=%s\n' "${_FUXX_AS_DATA-}"
  if (( BASH_VERSINFO[0] >= 4 )); then
    for k in ${_FUXX_AS_KEYS[@]+"${!_FUXX_AS_KEYS[@]}"}; do printf 'key=%s %s\n' "$k" "${_FUXX_AS_KEYS[$k]}"; done
  fi
  printf 'histfile=%s\n' "$hist"
  if (( _FUXX_AS_HOST )); then
    printf 'host=on\n'
  elif [[ -n ${_FUXX_AS_HOST_WHY-} ]]; then
    printf 'host=off:%s\n' "$_FUXX_AS_HOST_WHY"
  else
    printf 'host=off\n'
  fi
}

# `fuxx-completes forget`, `doctor` and `why` typed at this prompt: the binary runs with this
# adapter's copy of FUXX_PLUGIN_DATA, `doctor` gets the shell's facts and `why` this folder and
# history file. Defined only when the user has no function or alias of that name.
if (( _FUXX_AS_OWN_FN )) || { ! declare -F fuxx-completes >/dev/null 2>&1 && ! alias fuxx-completes >/dev/null 2>&1; }; then
  _FUXX_AS_OWN_FN=1
  fuxx-completes() {
    local bin=${FUXX_COMPLETES_BIN:-}
    [[ -n $bin ]] || bin=$(type -P fuxx-completes 2>/dev/null)
    if [[ -z $bin ]]; then
      printf '%s\n' 'fuxx-completes: not found on PATH' >&2
      return 127
    fi
    if [[ ${1-} == doctor ]]; then
      shift
      FUXX_PLUGIN_DATA=$_FUXX_AS_DATA "$bin" doctor --shell-facts "$(_fuxx_as_facts)" ${1+"$@"}
    elif [[ ${1-} == why ]]; then
      shift
      local hist=${HISTFILE-}
      local -a hf=()
      [[ -n $hist && $hist != /* ]] && hist=$PWD/$hist
      [[ -n $hist ]] && hf=(--histfile "$hist")
      FUXX_PLUGIN_DATA=$_FUXX_AS_DATA "$bin" why --format bash --cwd "$PWD" ${hf[@]+"${hf[@]}"} -- ${1+"$@"}
    else
      FUXX_PLUGIN_DATA=$_FUXX_AS_DATA "$bin" ${1+"$@"}
    fi
  }
fi
