#!/bin/sh
set -eu

prompt=${1:-"OpenSSH authentication"}
exec /usr/bin/osascript - "$prompt" <<'APPLESCRIPT'
on run argv
    set promptText to item 1 of argv
    set response to display dialog promptText with title "TV SSH Authentication" default answer "" with hidden answer buttons {"Cancel", "Continue"} default button "Continue" cancel button "Cancel"
    return text returned of response
end run
APPLESCRIPT
