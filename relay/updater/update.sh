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

# A file in the data folder, written whole. The relay's side of that folder
# isn't trusted: it can swap anything there at any moment. So the file is
# made in a folder of the updater's own (its own user's, closed to everyone
# else, so nothing can be planted in it), and moved into place in one step:
# never through a link, never onto a folder.
OWN="$DATA/.updater"
put() {
    (
        mkdir -m 700 "$OWN" 2>/dev/null
        cd "$OWN" 2>/dev/null || exit 0
        [ "$(stat -c '%u %a' . 2>/dev/null)" = "$(id -u) 700" ] || exit 0
        umask 022
        rm -f -- "./$1" 2>/dev/null
        printf '%s\n' "$2" >"./$1" 2>/dev/null || exit 0
        mv -fT -- "./$1" "$DATA/$1" 2>/dev/null || rm -f -- "./$1" 2>/dev/null
    )
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

# The update itself. Each step is given so long and no longer (a pull that
# hangs doesn't hold the updater for ever).
run_update() {
    status pulling "the newest images"
    # shellcheck disable=SC2086
    if ! out=$(timeout 900 docker compose pull $SERVICES 2>&1); then
        status failed "pulling: $(last "$out")"
        return 0
    fi
    status restarting "starting them again"
    # shellcheck disable=SC2086
    if ! out=$(timeout 300 docker compose up -d --no-deps $SERVICES 2>&1); then
        status failed "starting: $(last "$out")"
        return 0
    fi
    # shellcheck disable=SC2086
    imgs=$(docker inspect --format '{{.Name}} {{.Config.Image}} {{.Image}}' $SERVICES 2>/dev/null | sed 's#^/##; s#sha256:\([0-9a-f]\{12\}\)[0-9a-f]*#\1#' | tr '\n' ';')
    status done "running: ${imgs:-the two services}"
    return 0
}

# (While it's busy, it still says it's here.)
update() {
    (while :; do
        put update.alive "$(now)"
        sleep 5
    done) &
    keeper=$!
    run_update
    kill "$keeper" 2>/dev/null
    wait "$keeper" 2>/dev/null
    return 0
}

# (Always said afresh: a "pulling" left by an updater stopped halfway would
# otherwise stand for ever.)
status idle "ready"
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
