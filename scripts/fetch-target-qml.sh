#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "Usage: sh fetch-target-qml.sh root@TV_IP [reported-firmware-version]" >&2
    exit 2
fi

TARGET=$1
VERSION=${2-33.22.75}
case "$TARGET" in
    root@*) ;;
    *) echo "Use a root SSH destination such as root@192.168.1.50." >&2; exit 2 ;;
esac
case "$TARGET" in
    *[!A-Za-z0-9@._:-]*) echo "SSH destination contains unsupported characters." >&2; exit 2 ;;
esac
case "$VERSION" in
    ''|*[!0-9.]*) echo "Firmware label must contain only digits and dots." >&2; exit 2 ;;
esac

for tool in awk dirname mkdir mv rm scp shasum ssh; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "Required local command unavailable: $tool" >&2
        exit 1
    }
done

SCRIPT_DIR=$(CDPATH= cd "$(dirname "$0")" && pwd)
DEST=$SCRIPT_DIR/tv-qml-$VERSION
STAGE=$SCRIPT_DIR/tv-qml-$VERSION.new.$$
if [ -e "$DEST" ] || [ -e "$STAGE" ]; then
    echo "Output already exists; refusing to overwrite: $DEST" >&2
    exit 1
fi

REMOTE_CHECK='
set -eu
[ "$(id -u)" = 0 ] || { echo "SSH session is not root" >&2; exit 1; }
for file in \
  /usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml \
  /usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml \
  /usr/lib/qml/InputPicker/Containers/RootView.qml \
  /usr/lib/qml/InputPicker/Containers/InputPickerListItem.qml \
  /usr/lib/qml/InputPicker/Containers/InputPickerListItemTile.qml \
  /usr/lib/qml/InputPicker/Containers/InputPickerMainView.qml \
  /usr/lib/qml/InputPicker/Controllers/AppInterfaces/Interfaces.qml \
  /usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml \
  /usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/InputBanner.qml \
  /usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml
do
  [ -f "$file" ] && [ ! -L "$file" ] || { echo "Missing/non-regular target file: $file" >&2; exit 1; }
  if awk -v target="$file" '\''$2 == target { found=1 } END { exit !found }'\'' /proc/mounts; then
    echo "Refusing to copy a bind-mounted/overlaid file: $file" >&2
    exit 1
  fi
done
'

ssh "$TARGET" "$REMOTE_CHECK"
mkdir -p "$STAGE/usr/lib/qml/QuickSettings/Containers" \
         "$STAGE/usr/lib/qml/QuickSettings/Component" \
         "$STAGE/usr/lib/qml/InputPicker/Containers" \
         "$STAGE/usr/lib/qml/InputPicker/Controllers/AppInterfaces" \
         "$STAGE/usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces" \
         "$STAGE/usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner"
cleanup() {
    if [ -n "$STAGE" ] && [ -d "$STAGE" ]; then
        rm -rf "$STAGE"
    fi
}
trap cleanup 0 HUP INT TERM

scp "$TARGET:/usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml" \
    "$STAGE/usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml"
scp "$TARGET:/usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml" \
    "$STAGE/usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml"
scp "$TARGET:/usr/lib/qml/InputPicker/Containers/RootView.qml" \
    "$STAGE/usr/lib/qml/InputPicker/Containers/RootView.qml"
scp "$TARGET:/usr/lib/qml/InputPicker/Containers/InputPickerListItem.qml" \
    "$STAGE/usr/lib/qml/InputPicker/Containers/InputPickerListItem.qml"
scp "$TARGET:/usr/lib/qml/InputPicker/Containers/InputPickerListItemTile.qml" \
    "$STAGE/usr/lib/qml/InputPicker/Containers/InputPickerListItemTile.qml"
scp "$TARGET:/usr/lib/qml/InputPicker/Containers/InputPickerMainView.qml" \
    "$STAGE/usr/lib/qml/InputPicker/Containers/InputPickerMainView.qml"
scp "$TARGET:/usr/lib/qml/InputPicker/Controllers/AppInterfaces/Interfaces.qml" \
    "$STAGE/usr/lib/qml/InputPicker/Controllers/AppInterfaces/Interfaces.qml"
scp "$TARGET:/usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml" \
    "$STAGE/usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml"
scp "$TARGET:/usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/InputBanner.qml" \
    "$STAGE/usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/InputBanner.qml"
scp "$TARGET:/usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml" \
    "$STAGE/usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml"

printf '%s\n' "User-reported firmware label: $VERSION" \
    "Source: read-only root SSH/SCP from $TARGET" \
    'The label is caller-supplied, not independently verified.' \
    'The collector refuses files currently bind-mounted at the source paths.' \
    > "$STAGE/TARGET-INFO"
(
    cd "$STAGE"
    shasum -a 256 \
        usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml \
        usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml \
        usr/lib/qml/InputPicker/Containers/RootView.qml \
        usr/lib/qml/InputPicker/Containers/InputPickerListItem.qml \
        usr/lib/qml/InputPicker/Containers/InputPickerListItemTile.qml \
        usr/lib/qml/InputPicker/Containers/InputPickerMainView.qml \
        usr/lib/qml/InputPicker/Controllers/AppInterfaces/Interfaces.qml \
        usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml \
        usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/InputBanner.qml \
        usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml \
        > SOURCE-SHA256SUMS
)
mv "$STAGE" "$DEST"
STAGE=''
trap - 0 HUP INT TERM
echo "Copied target QML files to: $DEST"
echo "The TV was not modified. Review TARGET-INFO and SOURCE-SHA256SUMS before adapting the overlay."
