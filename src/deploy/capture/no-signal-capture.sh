set -eu

target_input=$1
restore_source=$2
capture_limit=$3

status_has_value() {
    property_key=$1
    property_value=$2
    printf '%s' "$input_status" | grep -Eq "\"$property_key\"[[:space:]]*:[[:space:]]*$property_value"
}

input_app_id_pattern() {
    case "$1" in
        hdmi1) printf '%s' 'com\.webos\.app\.hdmi1' ;;
        hdmi2) printf '%s' 'com\.webos\.app\.hdmi2' ;;
        hdmi3) printf '%s' 'com\.webos\.app\.hdmi3' ;;
        hdmi4) printf '%s' 'com\.webos\.app\.hdmi4' ;;
        *) return 1 ;;
    esac
}

input_device_index() {
    app_id_pattern=$(input_app_id_pattern "$1") || return 1
    found_index=
    for device_index in 0 1 2 3; do
        property_key=$(printf 'devices\\[%s\\]\\.appId' "$device_index")
        if status_has_value "$property_key" "\"$app_id_pattern\""; then
            if [ -n "$found_index" ]; then
                return 1
            fi
            found_index=$device_index
        fi
    done
    [ -n "$found_index" ] || return 1
    printf '%s' "$found_index"
}

input_property_key() {
    case "$2" in
        active) property=activate ;;
        plugged) property=hdmiPlugIn ;;
        signal) property=hdmiSignalExist ;;
        *) return 1 ;;
    esac
    printf 'devices\\[%s\\]\\.%s' "$1" "$property"
}

input_status_is() {
    device_index=$(input_device_index "$1") || return 1
    property_key=$(input_property_key "$device_index" "$2") || return 1
    status_has_value "$property_key" "$3"
}
read_input_status() {
    script -q -c 'luna-send -w 5000 -n 1 -f -q "devices[0].appId" -q "devices[0].activate" -q "devices[0].hdmiPlugIn" -q "devices[0].hdmiSignalExist" -q "devices[1].appId" -q "devices[1].activate" -q "devices[1].hdmiPlugIn" -q "devices[1].hdmiSignalExist" -q "devices[2].appId" -q "devices[2].activate" -q "devices[2].hdmiPlugIn" -q "devices[2].hdmiSignalExist" -q "devices[3].appId" -q "devices[3].activate" -q "devices[3].hdmiPlugIn" -q "devices[3].hdmiSignalExist" "luna://com.webos.service.eim/getAllInputStatus" "{}"' /dev/null
}

input_app_id() {
    case "$1" in
        hdmi1) printf '%s' 'com.webos.app.hdmi1' ;;
        hdmi2) printf '%s' 'com.webos.app.hdmi2' ;;
        hdmi3) printf '%s' 'com.webos.app.hdmi3' ;;
        hdmi4) printf '%s' 'com.webos.app.hdmi4' ;;
        *) return 1 ;;
    esac
}

launch_input() {
    app_id=$(input_app_id "$1") || return 1
    response=$(script -q -c "luna-send -w 5000 -n 1 -f 'luna://com.webos.applicationManager/launch' '{\"id\":\"$app_id\",\"params\":{}}'" /dev/null) || return 1
    if ! printf '%s' "$response" | grep -Eq '"returnValue"[[:space:]]*:[[:space:]]*true'; then
        printf 'TV rejected the request to select %s.\n' "$1" >&2
        return 1
    fi
}

input_status=$(read_input_status) || {
    printf 'Could not recheck TV input state before switching.\n' >&2
    exit 1
}
if ! input_status_is "$restore_source" active true; then
    printf 'The active HDMI input changed before No Signal capture.\n' >&2
    exit 1
fi
if ! input_status_is "$target_input" plugged false || ! input_status_is "$target_input" signal false; then
    printf 'Refusing No Signal capture because %s is connected or has signal.\n' "$target_input" >&2
    exit 1
fi

restore_input_on_exit() {
    exit_status=$?
    trap - EXIT
    trap '' HUP INT TERM
    if ! launch_input "$restore_source" >/dev/null 2>&1; then
        printf 'Could not restore the previously active HDMI input (%s).\n' "$restore_source" >&2
        if [ "$exit_status" -eq 0 ]; then
            exit_status=1
        fi
    elif ! input_status=$(read_input_status) || ! input_status_is "$restore_source" active true; then
        printf 'Could not verify restoration of the previously active HDMI input (%s).\n' "$restore_source" >&2
        if [ "$exit_status" -eq 0 ]; then
            exit_status=1
        fi
    fi
    exit "$exit_status"
}

trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
trap restore_input_on_exit EXIT

launch_input "$target_input"
sleep 5
input_status=$(read_input_status) || {
    printf 'Could not verify the selected HDMI input after switching.\n' >&2
    exit 1
}
if ! input_status_is "$target_input" active true; then
    printf 'TV accepted the request, but %s is not active; refusing to capture.\n' "$target_input" >&2
    exit 1
fi
if ! input_status_is "$target_input" plugged false || ! input_status_is "$target_input" signal false; then
    printf 'The selected input is connected or has signal; refusing No Signal capture.\n' >&2
    exit 1
fi
sh -s -- "$capture_limit" <<'HALGAL_CAPTURE_SCRIPT'
