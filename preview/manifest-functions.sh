# Shared POSIX-shell manifest validation, embedded into owner bundles.
# BEGIN shared manifest functions
valid_qml_path() {
    [ "$#" -eq 1 ] || return 1
    case "$1" in
        *[!A-Za-z0-9._/-]*) return 1 ;;
    esac
    printf '%064d  %s\n' 0 "$1" | validate_manifest target 5
}

validate_target_manifest() {
    [ "$#" -ge 1 ] && [ "$#" -le 2 ] || return 1
    validate_manifest target "${2:-4}" "$1"
}

validate_backup_manifest() {
    [ "$#" -ge 1 ] && [ "$#" -le 2 ] || return 1
    validate_manifest backup "${2:-4}" "$1"
}
backup_original_paths() {
    awk '$2 ~ /^usr\/lib\/qml\// { print $2 }' "$1"
}

validate_manifest() (
    [ "$#" -ge 2 ] || return 1
    manifest_kind=$1
    minimum_depth=$2
    shift 2
    case "$manifest_kind" in
        target|backup) ;;
        *) return 1 ;;
    esac
    case "$minimum_depth" in
        4|5) ;;
        *) return 1 ;;
    esac
    awk -v mode="$manifest_kind" -v min_depth="$minimum_depth" '
    function valid_qml_path(path, min_depth, parts, count, i) {
        if (path !~ /^usr\/lib\/qml\// || path ~ /[^A-Za-z0-9._\/-]/) return 0
        count = split(path, parts, "/")
        if (count < min_depth || parts[1] != "usr" || parts[2] != "lib" || parts[3] != "qml") return 0
        for (i = 1; i <= count; i++) {
            if (parts[i] == "" || parts[i] == "." || parts[i] == "..") return 0
        }
        return parts[count] ~ /\.qml$/
    }
    function valid_backup_path(path, min_depth, parts, count, i, target) {
        if (valid_qml_path(path, min_depth)) return 1
        if (path ~ /[^A-Za-z0-9._\/-]/) return 0
        count = split(path, parts, "/")
        for (i = 1; i <= count; i++) {
            if (parts[i] == "" || parts[i] == "." || parts[i] == "..") return 0
        }
        if (parts[1] !~ /^pre-update-[0-9]+-[0-9]+$/) return 0
        if (count == 2) {
            return parts[2] == "TARGET-SHA256SUMS" ||
                parts[2] == "COMPATIBILITY" || parts[2] == "apply.sh" ||
                parts[2] == "unmount.sh" || parts[2] == "uninstall.sh"
        }
        if (parts[2] != "overlay") return 0
        target = substr(path, index(path, "/overlay/") + length("/overlay/"))
        return valid_qml_path(target, min_depth)
    }
    {
        valid = 0
        if (NF == 2) {
            if (mode == "target") valid = valid_qml_path($2, min_depth)
            if (mode == "backup") valid = valid_backup_path($2, min_depth)
        }
        if (NF != 2 || length($1) != 64 || $1 ~ /[^0-9A-Fa-f]/ ||
            $0 != $1 "  " $2 || !valid || seen[$2]++) {
            invalid = 1
            next
        }
        entries++
        if (mode == "backup" && $2 ~ /^usr\/lib\/qml\//) originals++
    }
    END {
        if (invalid || entries == 0 || (mode == "backup" && originals == 0)) exit 1
    }
    ' "$@"
)
# END shared manifest functions
