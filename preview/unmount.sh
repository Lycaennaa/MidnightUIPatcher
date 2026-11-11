#!/bin/sh
set -eu
STATE=/var/lib/webosbrew/black-theme
RUN_MARKER=/run/black-theme-overlay.active

# @include-manifest-functions@

if [ "$(id -u)" != 0 ]; then
    echo "Black theme rollback requires root." >&2
    exit 1
fi
if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != "--lock-held" ]; }; then
    echo "Unexpected unmount.sh argument." >&2
    exit 2
fi
LOCK_DIR=/run/black-theme-overlay.lock
LOCK_HELD=0
if [ "${1-}" = "--lock-held" ]; then
    if [ ! -d "$LOCK_DIR" ]; then
        echo "Lock-held unmount mode requires the shared operation lock." >&2
        exit 1
    fi
    LOCK_HELD=1
elif ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "Another black-theme operation is active; retry rollback shortly." >&2
    exit 1
fi
release_lock() {
    [ "$LOCK_HELD" -eq 1 ] || rmdir "$LOCK_DIR" 2>/dev/null || true
}
trap 'release_lock' 0
trap 'exit 1' HUP INT TERM

if [ ! -f "$RUN_MARKER" ]; then
    exit 0
fi

if [ ! -f "$STATE/TARGET-SHA256SUMS" ] || [ -L "$STATE/TARGET-SHA256SUMS" ] ||
   [ ! -f "$STATE/BACKUP-SHA256SUMS" ] || [ -L "$STATE/BACKUP-SHA256SUMS" ]; then
    echo "Target or backup manifest is missing/non-regular; refusing to unmount." >&2
    exit 1
fi
if ! validate_target_manifest "$STATE/TARGET-SHA256SUMS" ||
   ! validate_backup_manifest "$STATE/BACKUP-SHA256SUMS"; then
    echo "Target or backup manifest is invalid; refusing to unmount." >&2
    exit 1
fi
FILES=$(awk '{ print $2 }' "$STATE/TARGET-SHA256SUMS")
HISTORICAL_FILES=$(backup_original_paths "$STATE/BACKUP-SHA256SUMS")
MANAGED_FILES="$FILES $HISTORICAL_FILES"

mount_present() {
    awk -v target="$1" '$2 == target { found=1 } END { exit !found }' /proc/mounts
}

status=0
for rel in $MANAGED_FILES; do
    target=/$rel
    if mount_present "$target" && ! umount "$target"; then
        echo "Could not unmount: $target" >&2
        status=1
    fi
done
for rel in $MANAGED_FILES; do
    if mount_present "/$rel"; then
        echo "Managed mount remains after unmount: /$rel" >&2
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    rm -f "$RUN_MARKER"
else
    echo "Some mounts remain; keep backups and retry rollback." >&2
    exit 1
fi
