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
FILES='
usr/lib/qml/QuickSettings/Utils/Style.qml
usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml
usr/lib/qml/SystemUI/Notification/NotificationStyle.qml
usr/lib/qml/SystemUI/Notification/Toast.qml
usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml
'

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
verify_active_state() {
    [ -f "$RUN_MARKER" ] || return 1
    (cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS" >/dev/null) || return 1
    for rel in $FILES; do
        expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$STATE/TARGET-SHA256SUMS") || return 1
        actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
        [ "$expected" = "$actual" ] || return 1
        mount_present "/$rel" || return 1
    done
    return 0
}
verify_no_mounts() {
    [ ! -e "$RUN_MARKER" ] || return 1
    for rel in $FILES; do mount_present "/$rel" && return 1; done
    return 0
}

if [ ! -f "$STATE/TARGET-SHA256SUMS" ] || [ -L "$STATE/TARGET-SHA256SUMS" ] || [ ! -f "$STATE/BACKUP-SHA256SUMS" ] || [ -L "$STATE/BACKUP-SHA256SUMS" ]; then
    echo "Existing target or backup manifest is missing; refusing update." >&2
    exit 1
fi
if ! (cd "$STATE/backups" && sha256sum -c "$STATE/BACKUP-SHA256SUMS"); then
    echo "Existing backup manifest does not verify; no state changed." >&2
    exit 1
fi
installed_entries=$(awk 'NF { count++ } END { print count + 0 }' "$STATE/TARGET-SHA256SUMS")
package_entries=$(awk 'NF { count++ } END { print count + 0 }' "$PKG_DIR/TARGET-SHA256SUMS")
if [ "$installed_entries" -ne 5 ] || [ "$package_entries" -ne 5 ]; then
    echo "Target manifests must each contain exactly five entries." >&2
    exit 1
fi
for rel in $FILES; do
    if ! expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$STATE/TARGET-SHA256SUMS"); then
        echo "Installed target manifest is missing or duplicates $rel; refusing update." >&2
        exit 1
    fi
    if ! package_expected=$(awk -v name="$rel" '$2 == name { count++; value=$1 } END { if (count != 1) exit 1; print value }' "$PKG_DIR/TARGET-SHA256SUMS"); then
        echo "Package target manifest is missing or duplicates $rel; refusing update." >&2
        exit 1
    fi
    if [ "$package_expected" != "$expected" ]; then
        echo "Package does not match the installed captured originals: $rel" >&2
        exit 1
    fi
    actual=$(sha256sum "$STATE/backups/$rel" | awk '{print $1}')
    if [ "$actual" != "$expected" ]; then
        echo "Captured original does not match the installed target hash: $rel" >&2
        exit 1
    fi
done


mounted_count=0
for rel in $FILES; do
    target=/$rel
    if [ ! -f "$target" ] || [ -L "$target" ] || [ ! -f "$STATE/backups/$rel" ] || [ -L "$STATE/backups/$rel" ] || [ ! -f "$PKG_DIR/overlay/$rel" ] || [ -L "$PKG_DIR/overlay/$rel" ]; then
        echo "Required target, backup, or new overlay is missing/non-regular: $target" >&2
        exit 1
    fi
    if mount_present "$target"; then mounted_count=$((mounted_count + 1)); fi
done
if [ -f "$RUN_MARKER" ]; then
    if [ "$mounted_count" -ne 5 ]; then
        echo "Mount marker and mount list disagree; refusing update." >&2
        exit 1
    fi
else
    if [ "$mounted_count" -ne 0 ]; then
        echo "Mounts exist without the active marker; refusing update." >&2
        exit 1
    fi
fi
if [ "$mounted_count" -eq 0 ] && ! (cd / && sha256sum -c "$PKG_DIR/TARGET-SHA256SUMS"); then
    echo "Live targets differ from the captured originals; refusing update." >&2
    exit 1
fi

STAMP=$(date +%Y%m%d%H%M%S)
STAGE=$ROOT/black-theme.next.$$
PREVIOUS=$ROOT/black-theme.previous.$$
PREUPDATE=pre-update-$STAMP-$$
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
mkdir -p "$STAGE/backups/$PREUPDATE/overlay"
for rel in $FILES; do
    dir=$(dirname "$rel")
    mkdir -p "$STAGE/overlay/$dir" "$STAGE/backups/$PREUPDATE/overlay/$dir"
    cp "$STATE/overlay/$rel" "$STAGE/backups/$PREUPDATE/overlay/$rel"
    cp "$PKG_DIR/overlay/$rel" "$STAGE/overlay/$rel"
    chmod 644 "$STAGE/overlay/$rel" "$STAGE/backups/$PREUPDATE/overlay/$rel"
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

{
    for rel in $FILES; do (cd "$STAGE/backups" && sha256sum "$rel"); done
    for rel in $FILES; do (cd "$STAGE/backups" && sha256sum "$PREUPDATE/overlay/$rel"); done
    for metadata in TARGET-SHA256SUMS COMPATIBILITY apply.sh unmount.sh uninstall.sh; do
        if [ -f "$STAGE/backups/$PREUPDATE/$metadata" ]; then
            (cd "$STAGE/backups" && sha256sum "$PREUPDATE/$metadata")
        fi
    done
    for old in "$STAGE"/backups/pre-update-*; do
        [ -d "$old" ] || continue
        case "$old" in "$STAGE/backups/$PREUPDATE") continue ;; esac
        for rel in $FILES; do
            [ ! -f "$old/overlay/$rel" ] || (cd "$STAGE/backups" && sha256sum "${old#"$STAGE/backups/"}/overlay/$rel")
        done
        for metadata in TARGET-SHA256SUMS COMPATIBILITY apply.sh unmount.sh uninstall.sh; do
            [ ! -f "$old/$metadata" ] || (cd "$STAGE/backups" && sha256sum "${old#"$STAGE/backups/"}/$metadata")
        done
    done
} > "$STAGE/BACKUP-SHA256SUMS"
if ! (cd "$STAGE/backups" && sha256sum -c "$STAGE/BACKUP-SHA256SUMS"); then
    echo "Staged original/pre-update backups do not verify; no live state changed." >&2
    exit 1
fi

if [ -f "$RUN_MARKER" ]; then
    unmounted=''
    for rel in $FILES; do
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
    for rel in $FILES; do
        if mount_present "/$rel"; then
            echo "Mount remains after successful unmount command; preserving old state for recovery." >&2
            exit 1
        fi
    done
    rm -f "$RUN_MARKER"
fi
if ! verify_no_mounts; then
    echo "Mount marker or mount list remains active after unmount; no state swap occurred." >&2
    exit 1
fi
if ! (cd / && sha256sum -c "$PKG_DIR/TARGET-SHA256SUMS"); then
    echo "Target hashes changed during unmount. No state swap occurred; the old overlay remains unmounted and backups are preserved." >&2
    exit 1
fi

if ! mv "$STATE" "$PREVIOUS"; then
    if [ "$mounted_count" -eq 5 ] && { ! "$STATE/apply.sh" --lock-held || ! verify_active_state; }; then
        echo "Could not stage update; original state remains, but overlay reactivation is unverified." >&2
    else
        echo "Could not stage update; original state and its prior active state are preserved." >&2
    fi
    exit 1
fi
if ! mv "$STAGE" "$STATE"; then
    if mv "$PREVIOUS" "$STATE"; then
        if [ "$mounted_count" -eq 5 ] && { ! "$STATE/apply.sh" --lock-held || ! verify_active_state; }; then
            echo "Staged state was rejected; original state restored but overlay reactivation is unverified." >&2
        else
            echo "Staged state was rejected; original state restored." >&2
        fi
    else
        echo "Staged state activation failed. Preserve $PREVIOUS and $STAGE for recovery." >&2
    fi
    exit 1
fi
STAGE=''

activation_failed=0
if ! "$STATE/apply.sh" --lock-held || ! verify_active_state; then
    activation_failed=1
fi
if [ "$activation_failed" -ne 0 ]; then
    echo "New overlay activation failed; attempting to restore the pre-update overlay." >&2
    if ! "$STATE/unmount.sh" --lock-held || ! verify_no_mounts; then
        echo "Could not verify zero new mounts. Preserve $STATE, $PREVIOUS, and the transferred bundle for recovery." >&2
        exit 1
    fi
    ROLLBACK_OVERLAY=$ROOT/black-theme.rollback-overlay.$$
    FAILED_OVERLAY=$ROOT/black-theme.failed-overlay.$$
    if [ -e "$ROLLBACK_OVERLAY" ] || [ -e "$FAILED_OVERLAY" ]; then
        echo "Rollback staging path already exists. Preserve $STATE and $PREVIOUS for recovery." >&2
        exit 1
    fi
    mkdir -m 700 "$ROLLBACK_OVERLAY"
    for rel in $FILES; do
        dir=$(dirname "$rel")
        mkdir -p "$ROLLBACK_OVERLAY/$dir"
        cp "$STATE/backups/$PREUPDATE/overlay/$rel" "$ROLLBACK_OVERLAY/$rel"
        chmod 644 "$ROLLBACK_OVERLAY/$rel"
    done
    cp "$STATE/backups/$PREUPDATE/COMPATIBILITY" "$STATE/COMPATIBILITY.rollback.$$"
    if ! mv "$STATE/COMPATIBILITY.rollback.$$" "$STATE/COMPATIBILITY"; then
        echo "Could not restore compatibility metadata. Preserve $STATE, $PREVIOUS, and $ROLLBACK_OVERLAY." >&2
        exit 1
    fi
    if ! mv "$STATE/overlay" "$FAILED_OVERLAY"; then
        echo "Could not stage failed overlay. Preserve $STATE, $PREVIOUS, and $ROLLBACK_OVERLAY." >&2
        exit 1
    fi
    if ! mv "$ROLLBACK_OVERLAY" "$STATE/overlay"; then
        if ! mv "$FAILED_OVERLAY" "$STATE/overlay"; then
            echo "Overlay restoration failed. Preserve $PREVIOUS, $FAILED_OVERLAY, and $ROLLBACK_OVERLAY." >&2
            exit 1
        fi
        echo "Could not activate restored overlay. Preserve $STATE and $PREVIOUS for recovery." >&2
        exit 1
    fi
    if ! "$STATE/apply.sh" --lock-held || ! verify_active_state; then
        if ! "$STATE/unmount.sh" --lock-held || ! verify_no_mounts; then
            echo "Previous overlay activation failed and mounts remain uncertain. Preserve $STATE, $PREVIOUS, and $FAILED_OVERLAY." >&2
        else
            echo "Previous overlay files were restored, but reapplication did not verify. Preserve $STATE, $PREVIOUS, and $FAILED_OVERLAY." >&2
        fi
        exit 1
    fi
    if ! rm -rf "$FAILED_OVERLAY" "$PREVIOUS"; then
        echo "Previous overlay is active and verified, but recovery cleanup is incomplete. Inspect $STATE and remaining recovery paths." >&2
        exit 1
    fi
    echo "New overlay failed; the previous overlay was restored and verified. Original and pre-update backups remain synchronized; transferred bundle retained for recovery." >&2
    exit 1
fi
if ! rm -rf "$PREVIOUS"; then
    echo "New overlay is active and verified, but old state cleanup failed. Preserve backups and inspect $PREVIOUS." >&2
    exit 1
fi
echo "Theme state updated in place. Original and pre-update overlay backups are retained; refresh the display service only after explicit owner approval."
