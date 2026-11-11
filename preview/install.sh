#!/bin/sh
set -eu

ACK_TARGET=--confirm-target-qml-source
ACK_RECOVERY=--confirm-root-ssh-recovery
if [ "$#" -ne 2 ] || [ "$1" != "$ACK_TARGET" ] || [ "$2" != "$ACK_RECOVERY" ]; then
 echo "Usage: sh install.sh $ACK_TARGET $ACK_RECOVERY" >&2
 echo "These confirm exact captured QML hashes and root SSH recovery availability." >&2
 exit 2
fi

if [ "$(id -u)" != 0 ]; then
    echo "Run this installer from a root shell." >&2
    exit 1
fi

for tool in awk chmod cp dirname id mkdir mount mv rm rmdir run-parts sha256sum umount; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "Required command unavailable: $tool" >&2
        exit 1
    }
done

PKG_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
if ! (cd "$PKG_DIR" && sha256sum -c SHA256SUMS); then
    echo "Package checksum verification failed; nothing installed." >&2
    exit 1
fi

ROOT=/var/lib/webosbrew
HOOK_DIR=$ROOT/init.d
STATE=$ROOT/black-theme
HOOK=$HOOK_DIR/50-black-theme
RUN_MARKER=/run/black-theme-overlay.active
# @include-manifest-functions@

if [ ! -d "$HOOK_DIR" ] || ! command -v run-parts >/dev/null 2>&1; then
    echo "Homebrew init.d/run-parts not found; no changes made." >&2
    exit 1
fi
if [ ! -d /run ] || [ ! -w /run ] || [ ! -r /proc/mounts ]; then
    echo "Runtime mount state is unavailable; no changes made." >&2
    exit 1
fi
umask 077
LOCK_DIR=/run/black-theme-overlay.lock
if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "Another black-theme operation is active; retry later." >&2
    exit 1
fi
release_lock() { rmdir "$LOCK_DIR" 2>/dev/null || true; }
trap 'release_lock' 0
trap 'exit 1' HUP INT TERM
if [ -e "$STATE" ] || [ -e "$HOOK" ] || [ -e "$RUN_MARKER" ]; then
    echo "An existing theme state, hook, or mount marker was found; refusing to overwrite it." >&2
    exit 1
fi

mount_present() {
    awk -v target="$1" '$2 == target { found=1 } END { exit !found }' /proc/mounts
}
if [ ! -f "$PKG_DIR/TARGET-SHA256SUMS" ] || [ -L "$PKG_DIR/TARGET-SHA256SUMS" ] ||
   ! validate_target_manifest "$PKG_DIR/TARGET-SHA256SUMS"; then
    echo "Package target manifest is missing, unsafe, or invalid." >&2
    exit 1
fi
FILES=$(awk '{ print $2 }' "$PKG_DIR/TARGET-SHA256SUMS")
for rel in $FILES; do
    if mount_present "/$rel"; then
        echo "Managed target is already a mount point: /$rel" >&2
        exit 1
    fi
done

for rel in $FILES; do
    target=/$rel
    source=$PKG_DIR/overlay/$rel
    if [ ! -f "$target" ] || [ -L "$target" ] || [ ! -f "$source" ] || [ -L "$source" ]; then
        echo "Expected regular QML source/overlay missing: $target" >&2
        exit 1
    fi
done
if ! (cd / && sha256sum -c "$PKG_DIR/TARGET-SHA256SUMS"); then
    echo "Target QML differs from the captured device files; no changes made." >&2
    exit 1
fi

umask 077
STAGE=$ROOT/black-theme.new.$$
if [ -e "$STAGE" ]; then
    echo "Temporary state path already exists: $STAGE" >&2
    exit 1
fi
mkdir -p "$STAGE/backups" "$STAGE/overlay"
: > "$STAGE/BACKUP-SHA256SUMS"
cp "$PKG_DIR/COMPATIBILITY" "$STAGE/COMPATIBILITY"

for rel in $FILES; do
    dir=$(dirname "$rel")
    mkdir -p "$STAGE/backups/$dir" "$STAGE/overlay/$dir"
    cp "/$rel" "$STAGE/backups/$rel"
    cp "$PKG_DIR/overlay/$rel" "$STAGE/overlay/$rel"
    chmod 600 "$STAGE/backups/$rel"
    chmod 644 "$STAGE/overlay/$rel"
    (cd "$STAGE/backups" && sha256sum "$rel") >> "$STAGE/BACKUP-SHA256SUMS"
done
if ! (cd "$STAGE/backups" && sha256sum -c "$STAGE/BACKUP-SHA256SUMS" && sha256sum -c "$PKG_DIR/TARGET-SHA256SUMS"); then
    echo "Captured originals failed backup/hash verification; installation aborted." >&2
    exit 1
fi

cp "$PKG_DIR/apply.sh" "$STAGE/apply.sh"
cp "$PKG_DIR/unmount.sh" "$STAGE/unmount.sh"
cp "$PKG_DIR/uninstall.sh" "$STAGE/uninstall.sh"
chmod 700 "$STAGE/apply.sh" "$STAGE/unmount.sh" "$STAGE/uninstall.sh"
cp "$PKG_DIR/TARGET-SHA256SUMS" "$STAGE/TARGET-SHA256SUMS"
chmod 644 "$STAGE/TARGET-SHA256SUMS"
cleanup_stage() { [ ! -d "$STAGE" ] || rm -rf "$STAGE"; release_lock; }
trap 'cleanup_stage' 0
mv "$STAGE" "$STATE"

HOOK_TMP=$HOOK_DIR/50-black-theme.new.$$
printf '%s\n' '#!/bin/sh' 'exec /var/lib/webosbrew/black-theme/apply.sh' > "$HOOK_TMP"
chmod 755 "$HOOK_TMP"
if ! mv "$HOOK_TMP" "$HOOK"; then
    rm -f "$HOOK_TMP"
    echo "Could not install the boot hook. Backups remain at $STATE/backups." >&2
    exit 1
fi

if ! "$STATE/apply.sh" --lock-held; then
    rm -f "$HOOK"
    "$STATE/unmount.sh" --lock-held || true
    echo "Overlay application failed. Boot hook removed; backups retained at $STATE/backups." >&2
    exit 1
fi

echo "Overlay mounted. Original files remain untouched; backups: $STATE/backups"
echo "To undo: sh $STATE/uninstall.sh"
cat "$STATE/COMPATIBILITY"
