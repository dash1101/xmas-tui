#!/bin/sh
# xmas-updater: runs beside the relay. When the relay's console asks for an
# update, it pulls the newest images for the relay and its host and starts
# them again. Players are dropped for a few seconds and come straight back.
#
# It is asked by a file: the relay writes update.request in its data folder.
# Nothing from that folder is ever run, read as a command or used as an
# argument (the request's content isn't read at all), because that folder is
# the relay's, and the relay faces the internet. It answers with
# update.status (one line of JSON) and update.alive (the time, every look).
#
# It holds the Docker socket, which is full control of Docker on the machine.
# That's why it is opt-in, has no ports and no network, and only ever names
# the two services below.
#
# Settings: PROJECT_DIR (the folder compose.yaml is in, mounted at the same
# path as on the machine), DATA (the relay's data folder: /data), EVERY
# (seconds between looks: 5).
set -u
DATA="${DATA:-/data}"
EVERY="${EVERY:-5}"
SERVICES="xmas-relay xmas-host"
: "${PROJECT_DIR:?PROJECT_DIR is not set (the folder compose.yaml is in)}"
case "$DATA" in /?*) ;; *) echo "xmas-updater: DATA must be a full path" >&2; exit 1 ;; esac
[ -d "$DATA" ] || { echo "xmas-updater: no data folder at $DATA" >&2; exit 1; }
cd "$PROJECT_DIR" || { echo "xmas-updater: no folder $PROJECT_DIR" >&2; exit 1; }
[ -f compose.yaml ] || { echo "xmas-updater: no compose.yaml in $PROJECT_DIR" >&2; exit 1; }

now() { date +%s; }

# A file in the data folder, written whole: through a file of its own beside
# it, then moved into place. Never through a link or onto a folder (the
# relay's side of that folder isn't trusted).
put() {
    f="$DATA/$1"
    t="$DATA/.$1.tmp"
    # (a link is taken away, never followed; a folder there is left alone,
    # and nothing is written)
    rm -f -- "$t" 2>/dev/null
    [ -L "$f" ] && rm -f -- "$f" 2>/dev/null
    if [ -e "$t" ] || [ -L "$t" ] || [ -d "$f" ] || [ -L "$f" ]; then
        return 0
    fi
    (umask 022 && printf '%s\n' "$2" >"$t") 2>/dev/null || return 0
    if [ -L "$t" ] || ! mv -f -- "$t" "$f" 2>/dev/null; then
        rm -f -- "$t" 2>/dev/null
    fi
    return 0
}

# Text made safe for a JSON string: one line, plain letters only, 600 at most.
esc() {
    printf '%s' "$1" | tr '\n\r\t' '   ' | LC_ALL=C tr -cd ' -~' | cut -c1-600 | sed 's/\\/\\\\/g; s/"/\\"/g'
}

status() {
    put update.status "{\"at\":$(now),\"state\":\"$1\",\"detail\":\"$(esc "${2:-}")\"}"
    echo "xmas-updater: $1 ${2:-}"
}

# The last few lines of what a command said.
last() {
    printf '%s' "$1" | tail -n 4
}

update() {
    status pulling "the newest images"
    # shellcheck disable=SC2086
    if ! out=$(docker compose pull $SERVICES 2>&1); then
        status failed "pulling: $(last "$out")"
        return 0
    fi
    status restarting "starting them again"
    # shellcheck disable=SC2086
    if ! out=$(docker compose up -d --no-deps $SERVICES 2>&1); then
        status failed "starting: $(last "$out")"
        return 0
    fi
    # shellcheck disable=SC2086
    imgs=$(docker inspect --format '{{.Name}} {{.Config.Image}} {{.Image}}' $SERVICES 2>/dev/null | sed 's#^/##; s#sha256:\([0-9a-f]\{12\}\)[0-9a-f]*#\1#' | tr '\n' ';')
    status done "running: ${imgs:-the two services}"
    return 0
}

[ -f "$DATA/update.status" ] && [ ! -L "$DATA/update.status" ] || status idle "ready"
echo "xmas-updater: watching $DATA for requests, for $SERVICES in $PROJECT_DIR"
while :; do
    put update.alive "$(now)"
    r="$DATA/update.request"
    if [ -e "$r" ] || [ -L "$r" ]; then
        fresh=0
        if [ -f "$r" ] && [ ! -L "$r" ]; then
            made=$(stat -c %Y "$r" 2>/dev/null || echo 0)
            case "$made" in '' | *[!0-9]*) made=0 ;; esac
            [ $(($(now) - made)) -lt 600 ] && fresh=1
        fi
        # (a file or a link is taken away; a folder can't be, and isn't
        # acted on)
        rm -f -- "$r" 2>/dev/null
        if [ -e "$r" ] || [ -L "$r" ]; then
            status failed "the request couldn't be cleared: not acted on"
        elif [ "$fresh" = 1 ]; then
            update
        else
            status failed "an old or odd request was dropped"
        fi
    fi
    sleep "$EVERY"
done
