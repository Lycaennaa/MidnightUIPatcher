#!/bin/sh
set -eu

ROOT=/var/lib/webosbrew
STATE=$ROOT/black-theme
HOOK=$ROOT/init.d/50-black-theme

if [ "$(id -u)" != 0 ]; then
    echo "Black theme uninstall requires root." >&2
    exit 1
fi
LOCK_DIR=/run/black-theme-overlay.lock
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "Another black-theme operation is active; retry later." >&2
    exit 1
fi
release_lock() { rmdir "$LOCK_DIR" 2>/dev/null || true; }
trap 'release_lock' 0
trap 'exit 1' HUP INT TERM
if [ ! -d "$STATE" ]; then
    echo "No black theme installation state found." >&2
    exit 1
fi

: > "$STATE/disabled"
rm -f "$HOOK"
"$STATE/unmount.sh" --lock-held
echo "Boot hook removed and active bind mounts undone. Original backups retained at $STATE/backups."
