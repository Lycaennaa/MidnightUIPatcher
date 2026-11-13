#!/bin/sh
set -eu

ACK_TARGET=--confirm-target-qml-source
ACK_RECOVERY=--confirm-root-ssh-recovery
if [ "$#" -ne 2 ] || [ "$1" != "$ACK_TARGET" ] || [ "$2" != "$ACK_RECOVERY" ]; then
    echo "Usage: sh update.sh $ACK_TARGET $ACK_RECOVERY" >&2
    exit 2
fi
if [ "$(id -u)" != 0 ]; then
    echo "Black theme update requires root." >&2
    exit 1
fi

for tool in awk chmod cp date dirname id mkdir mount mv rm rmdir sha256sum umount; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "Required tool unavailable: $tool" >&2
        exit 1
    }
done

PKG_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
if ! (cd "$PKG_DIR" && sha256sum -c SHA256SUMS); then
    echo "Update bundle checksum verification failed; no state changed." >&2
    exit 1
fi

ROOT=/var/lib/webosbrew
STATE=$ROOT/black-theme
HOOK_DIR=$ROOT/init.d
HOOK=$HOOK_DIR/50-black-theme
RUN_MARKER=/run/black-theme-overlay.active
OP_LOCK=/run/black-theme-overlay.lock
# @include-manifest-functions@

if [ ! -d /run ] || [ ! -w /run ] || [ ! -r /proc/mounts ]; then
    echo "Runtime mount state is unavailable; no changes made." >&2
    exit 1
fi
umask 077
if ! mkdir "$OP_LOCK" 2>/dev/null; then
    echo "Another overlay or update operation is active; retry later." >&2
    exit 1
fi
release_lock() { rmdir "$OP_LOCK" 2>/dev/null || true; }
trap 'release_lock' 0
trap 'exit 1' HUP INT TERM
for recovery in "$ROOT"/black-theme.previous.* "$ROOT"/black-theme.failed-overlay.* "$ROOT"/black-theme.rollback-overlay.*; do
    [ -e "$recovery" ] || continue
    echo "Unresolved update recovery path found; preserve and inspect: $recovery" >&2
    exit 1
done
if [ ! -d "$STATE" ] || [ -L "$STATE" ] || [ ! -d "$STATE/backups" ] || [ -L "$STATE/backups" ] || [ ! -d "$STATE/overlay" ] || [ -L "$STATE/overlay" ] || [ ! -f "$STATE/COMPATIBILITY" ] || [ -L "$STATE/COMPATIBILITY" ] || [ ! -f "$HOOK" ] || [ -L "$HOOK" ]; then
    echo "Existing install state or boot hook is incomplete; refusing update." >&2
    exit 1
fi
if [ -f "$STATE/disabled" ]; then
    echo "Theme is disabled; refusing to update inactive state." >&2
    exit 1
fi

mount_present() {
    awk -v target="$1" '$2 == target { found=1 } END { exit !found }' /proc/mounts
}
is_installed() {
    for installed in $INSTALLED_FILES; do
        [ "$installed" = "$1" ] && return 0
    done
    return 1
}
is_historical() {
    for historical in $HISTORICAL_FILES; do
        [ "$historical" = "$1" ] && return 0
    done
    return 1
}
verify_active_state() {
    [ -f "$RUN_MARKER" ] || return 1
    (cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS" >/dev/null) || return 1
    for rel in $INSTALLED_FILES; do
        expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$STATE/TARGET-SHA256SUMS") || return 1
        actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
        [ "$expected" = "$actual" ] || return 1
        mount_present "/$rel" || return 1
    done
    for rel in $MANAGED_FILES; do
        is_installed "$rel" && continue
        mount_present "/$rel" && return 1
    done
    return 0
}
verify_no_mounts() {
    [ ! -e "$RUN_MARKER" ] || return 1
    for rel in $MANAGED_FILES; do mount_present "/$rel" && return 1; done
    return 0
}
restore_previous_overlay() {
    [ "$mounted_count" -gt 0 ] || return 0
    if [ -f "$RUN_MARKER" ]; then
        for rel in $PREVIOUS_FILES; do
            if ! mount_present "/$rel" && ! mount --bind "$STATE/overlay/$rel" "/$rel"; then
                echo "Could not restore the previous mount: /$rel" >&2
                return 1
            fi
        done
        if verify_active_state; then return 0; fi
        echo "Previous mounts remain inconsistent; state and backups are preserved for recovery." >&2
        return 1
    fi
    if "$STATE/apply.sh" --lock-held && verify_active_state; then return 0; fi
    if ! "$STATE/unmount.sh" --lock-held || ! verify_no_mounts; then
        echo "Could not reactivate or safely unmount the previous overlay; mounts require recovery." >&2
        return 1
    fi
    if ! : > "$STATE/disabled"; then
        echo "Previous overlay could not be reactivated; state and backups are preserved for recovery." >&2
        return 1
    fi
    echo "Previous overlay could not be reactivated; the theme is left safely disabled." >&2
    return 1
}

if [ ! -f "$STATE/TARGET-SHA256SUMS" ] || [ -L "$STATE/TARGET-SHA256SUMS" ] ||
   [ ! -f "$STATE/BACKUP-SHA256SUMS" ] || [ -L "$STATE/BACKUP-SHA256SUMS" ] ||
   [ ! -f "$PKG_DIR/TARGET-SHA256SUMS" ] || [ -L "$PKG_DIR/TARGET-SHA256SUMS" ]; then
    echo "Existing or package target/backup manifest is missing/non-regular; refusing update." >&2
    exit 1
fi
if ! validate_target_manifest "$STATE/TARGET-SHA256SUMS" ||
   ! validate_target_manifest "$PKG_DIR/TARGET-SHA256SUMS" ||
   ! validate_backup_manifest "$STATE/BACKUP-SHA256SUMS"; then
    echo "Existing or package target/backup manifest is invalid; refusing update." >&2
    exit 1
fi
INSTALLED_FILES=$(awk '{ print $2 }' "$STATE/TARGET-SHA256SUMS")
PACKAGE_FILES=$(awk '{ print $2 }' "$PKG_DIR/TARGET-SHA256SUMS")
HISTORICAL_FILES=$(backup_original_paths "$STATE/BACKUP-SHA256SUMS")
PREVIOUS_FILES=$INSTALLED_FILES
MANAGED_FILES="$INSTALLED_FILES $PACKAGE_FILES $HISTORICAL_FILES"
if ! (cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS"); then
    echo "Existing backup manifest does not verify; no state changed." >&2
    exit 1
fi
for rel in $INSTALLED_FILES; do
    if ! is_historical "$rel" ||
       [ ! -f "/$rel" ] || [ -L "/$rel" ] ||
       [ ! -f "$STATE/backups/$rel" ] || [ -L "$STATE/backups/$rel" ] ||
       [ ! -f "$STATE/overlay/$rel" ] || [ -L "$STATE/overlay/$rel" ]; then
        echo "Required installed target, original backup, or overlay is missing/non-regular: /$rel" >&2
        exit 1
    fi
    if ! expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$STATE/TARGET-SHA256SUMS") ||
       ! backup_expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$STATE/BACKUP-SHA256SUMS"); then
        echo "Installed target or direct-original backup is missing/duplicated: $rel" >&2
        exit 1
    fi
    actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
    if [ "$actual" != "$expected" ] || [ "$backup_expected" != "$expected" ]; then
        echo "Captured original does not match the installed target hash: $rel" >&2
        exit 1
    fi
done
for rel in $PACKAGE_FILES; do
    if ! package_expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$PKG_DIR/TARGET-SHA256SUMS"); then
        echo "Package target manifest is missing or duplicates $rel; refusing update." >&2
        exit 1
    fi
    if [ ! -f "$PKG_DIR/overlay/$rel" ] || [ -L "$PKG_DIR/overlay/$rel" ]; then
        echo "New overlay is missing or non-regular: /$rel" >&2
        exit 1
    fi
    if is_historical "$rel"; then
        if ! backup_expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$STATE/BACKUP-SHA256SUMS") ||
           [ "$backup_expected" != "$package_expected" ]; then
            echo "Package does not match the preserved captured original: $rel" >&2
            exit 1
        fi
    else
        if mount_present "/$rel"; then
            echo "New target is already mounted and has no preserved original: /$rel" >&2
            exit 1
        fi
        if [ ! -f "/$rel" ] || [ -L "/$rel" ]; then
            echo "New target is missing or non-regular: /$rel" >&2
            exit 1
        fi
        actual=$(sha256sum "/$rel" | awk '{print $1}')
        if [ "$actual" != "$package_expected" ]; then
            echo "New target differs from its package-captured original: $rel" >&2
            exit 1
        fi
    fi
done

mounted_count=0
for rel in $MANAGED_FILES; do
    if mount_present "/$rel"; then mounted_count=$((mounted_count + 1)); fi
done
if [ -f "$RUN_MARKER" ]; then
    if ! verify_active_state; then
        echo "Mount marker, installed targets, backups, and mount list disagree; refusing update." >&2
        exit 1
    fi
else
    if ! verify_no_mounts; then
        echo "Mounts exist without the active marker; refusing update." >&2
        exit 1
    fi
    if ! (cd / && sha256sum -c "$STATE/TARGET-SHA256SUMS"); then
        echo "Live targets differ from the installed captured originals; refusing update." >&2
        exit 1
    fi
fi
if [ "$mounted_count" -eq 0 ] && ! (cd / && sha256sum -c "$PKG_DIR/TARGET-SHA256SUMS"); then
    echo "Live targets differ from the new captured originals; refusing update." >&2
    exit 1
fi
STAMP=$(date +%Y%m%d%H%M%S)
STAGE=$ROOT/black-theme.next.$$
PREVIOUS=$ROOT/black-theme.previous.$$
PREUPDATE=pre-update-$STAMP-$$
write_backup_manifest() {
    {
        cat "$STATE/BACKUP-SHA256SUMS"
        for rel in $PACKAGE_FILES; do
            is_historical "$rel" && continue
            (cd "$1" && sha256sum "$rel")
        done
        for rel in $PREVIOUS_FILES; do
            (cd "$1" && sha256sum "$PREUPDATE/overlay/$rel")
        done
        for metadata in TARGET-SHA256SUMS COMPATIBILITY apply.sh unmount.sh uninstall.sh; do
            if [ -f "$1/$PREUPDATE/$metadata" ]; then
                (cd "$1" && sha256sum "$PREUPDATE/$metadata")
            fi
        done
    } > "$2"
}
if [ -e "$STAGE" ] || [ -e "$PREVIOUS" ] || [ -e "$STATE/backups/$PREUPDATE" ]; then
    echo "A temporary update path already exists; no state changed." >&2
    exit 1
fi
mkdir -m 700 "$STAGE"
cleanup_stage() { [ ! -d "$STAGE" ] || rm -rf "$STAGE"; }
trap 'cleanup_stage; release_lock' 0

if ! cp -R "$STATE/." "$STAGE/"; then
    echo "Could not stage existing state; no state changed." >&2
    exit 1
fi
rm -rf "$STAGE/overlay"
mkdir -p "$STAGE/overlay" "$STAGE/backups/$PREUPDATE/overlay"
for rel in $PREVIOUS_FILES; do
    dir=$(dirname "$rel")
    mkdir -p "$STAGE/backups/$PREUPDATE/overlay/$dir"
    cp "$STATE/overlay/$rel" "$STAGE/backups/$PREUPDATE/overlay/$rel"
    chmod 644 "$STAGE/backups/$PREUPDATE/overlay/$rel"
done
for rel in $PACKAGE_FILES; do
    if ! is_historical "$rel"; then
        target=/$rel
        if mount_present "$target" || [ ! -f "$target" ] || [ -L "$target" ]; then
            echo "New target must be unmounted and regular before its original is captured: $target" >&2
            exit 1
        fi
        package_expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$PKG_DIR/TARGET-SHA256SUMS")
        actual=$(sha256sum "$target" | awk '{print $1}')
        if [ "$actual" != "$package_expected" ]; then
            echo "New target differs from its package-captured original: $rel" >&2
            exit 1
        fi
        if [ -e "$STAGE/backups/$rel" ] || [ -L "$STAGE/backups/$rel" ]; then
            echo "An unmanifested backup already occupies the new target path: $rel" >&2
            exit 1
        fi
        dir=$(dirname "$rel")
        mkdir -p "$STAGE/backups/$dir"
        cp "$target" "$STAGE/backups/$rel"
        chmod 600 "$STAGE/backups/$rel"
        actual=$(sha256sum "$STAGE/backups/$rel" | awk '{print $1}')
        if [ "$actual" != "$package_expected" ]; then
            echo "New target original changed while being captured: $rel" >&2
            exit 1
        fi
    fi
done
for rel in $PACKAGE_FILES; do
    dir=$(dirname "$rel")
    mkdir -p "$STAGE/overlay/$dir"
    cp "$PKG_DIR/overlay/$rel" "$STAGE/overlay/$rel"
    chmod 644 "$STAGE/overlay/$rel"
done
for metadata in TARGET-SHA256SUMS COMPATIBILITY apply.sh unmount.sh uninstall.sh; do
    if [ -f "$STATE/$metadata" ]; then cp "$STATE/$metadata" "$STAGE/backups/$PREUPDATE/$metadata"; fi
done
cp "$PKG_DIR/TARGET-SHA256SUMS" "$STAGE/TARGET-SHA256SUMS"
cp "$PKG_DIR/COMPATIBILITY" "$STAGE/COMPATIBILITY"
cp "$PKG_DIR/apply.sh" "$STAGE/apply.sh"
cp "$PKG_DIR/unmount.sh" "$STAGE/unmount.sh"
cp "$PKG_DIR/uninstall.sh" "$STAGE/uninstall.sh"
chmod 700 "$STAGE/apply.sh" "$STAGE/unmount.sh" "$STAGE/uninstall.sh"

write_backup_manifest "$STAGE/backups" "$STAGE/BACKUP-SHA256SUMS"
if ! validate_backup_manifest "$STAGE/BACKUP-SHA256SUMS" ||
   ! (cd "$STAGE/backups" && sha256sum -c "$STAGE/BACKUP-SHA256SUMS"); then
    echo "Staged original/pre-update backups do not verify; no live state changed." >&2
    exit 1
fi
if [ -f "$RUN_MARKER" ]; then
    unmounted=''
    for rel in $PREVIOUS_FILES; do
        target=/$rel
        if ! umount "$target"; then
            restore_status=0
            for restore in $unmounted; do
                if ! mount --bind "$STATE/overlay/$restore" "/$restore"; then
                    echo "Could not restore mount after failed unmount: /$restore" >&2
                    restore_status=1
                fi
            done
            if [ "$restore_status" -ne 0 ] || ! verify_active_state; then
                echo "Mount recovery is incomplete. Preserve $STATE and backups; do not retry automatically." >&2
            else
                echo "Could not unmount $target; original overlay restored and verified." >&2
            fi
            exit 1
        fi
        unmounted="$rel $unmounted"
    done
    for rel in $MANAGED_FILES; do
        if mount_present "/$rel"; then
            echo "Mount remains after successful unmount command; restoring the previous overlay." >&2
            restore_previous_overlay || true
            exit 1
        fi
    done
    rm -f "$RUN_MARKER"
fi
if ! verify_no_mounts; then
    echo "A managed target mount or marker remains after unmount; no state swap occurred." >&2
    restore_previous_overlay || true
    exit 1
fi
if ! (cd / && sha256sum -c "$STATE/TARGET-SHA256SUMS"); then
    echo "Installed target hashes changed during unmount. No state swap occurred; backups are preserved." >&2
    restore_previous_overlay || true
    exit 1
fi
if ! (cd / && sha256sum -c "$PKG_DIR/TARGET-SHA256SUMS"); then
    echo "New target hashes changed during unmount. No state swap occurred; backups are preserved." >&2
    restore_previous_overlay || true
    exit 1
fi


if ! mv "$STATE" "$PREVIOUS"; then
    restore_previous_overlay || true
    echo "Could not stage update; original state is preserved." >&2
    exit 1
fi
if ! mv "$STAGE" "$STATE"; then
    if mv "$PREVIOUS" "$STATE"; then
        INSTALLED_FILES=$PREVIOUS_FILES
        restore_previous_overlay || true
        echo "Staged state was rejected; original state restored." >&2
    else
        echo "Staged state activation failed. Preserve $PREVIOUS and $STAGE for recovery." >&2
    fi
    exit 1
fi
STAGE=''
INSTALLED_FILES=$PACKAGE_FILES

activation_failed=0
if ! "$STATE/apply.sh" --lock-held || ! verify_active_state; then
    activation_failed=1
fi
if [ "$activation_failed" -ne 0 ]; then
    echo "New overlay activation failed; attempting to restore the pre-update overlay." >&2
    FAILED=$ROOT/black-theme.failed-overlay.$$
    if ! "$STATE/unmount.sh" --lock-held || ! verify_no_mounts; then
        echo "Could not verify zero mounts. Preserve $STATE and $PREVIOUS for recovery." >&2
        exit 1
    fi
    if [ -e "$FAILED" ]; then
        echo "Failed-state recovery path already exists. Preserve $STATE and $PREVIOUS for recovery." >&2
        exit 1
    fi
    if ! mv "$STATE" "$FAILED" || ! mv "$PREVIOUS" "$STATE"; then
        echo "Could not restore the previous state. Preserve $STATE, $PREVIOUS, and $FAILED for recovery." >&2
        exit 1
    fi
    INSTALLED_FILES=$PREVIOUS_FILES
    if ! "$STATE/apply.sh" --lock-held || ! verify_active_state; then
        if ! "$STATE/unmount.sh" --lock-held || ! verify_no_mounts; then
            echo "Previous overlay activation failed and mounts remain uncertain. Preserve $STATE and $FAILED for recovery." >&2
        else
            echo "Previous overlay files were restored, but reapplication did not verify. Preserve $STATE and $FAILED for recovery." >&2
        fi
        exit 1
    fi
    if ! validate_backup_manifest "$FAILED/BACKUP-SHA256SUMS" ||
       ! (cd "$FAILED/backups" && sha256sum -c "$FAILED/BACKUP-SHA256SUMS"); then
        echo "Previous overlay is active and verified, but failed-update backups could not be verified. Preserve $STATE and $FAILED for recovery." >&2
        exit 1
    fi
    echo "New overlay failed; the previous overlay was restored and verified. Failed-update originals and the pre-update overlay are retained at $FAILED; inspect and resolve this recovery path before another update." >&2
    exit 1
fi
if ! rm -rf "$PREVIOUS"; then
    echo "New overlay is active and verified, but old state cleanup failed. Preserve backups and inspect $PREVIOUS." >&2
    exit 1
fi
echo "Theme state updated in place. Original and pre-update overlay backups are retained; refresh the display service only after explicit owner approval."
