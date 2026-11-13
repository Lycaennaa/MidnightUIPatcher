[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Target,
    [Parameter(Position = 1)]
    [string]$FirmwareLabel = '33.22.75'
)

$ErrorActionPreference = 'Stop'

if ($Target -notmatch '^root@[A-Za-z0-9.-]+$') {
    throw 'Use a root SSH destination such as root@192.168.1.50.'
}
if ($FirmwareLabel -notmatch '^[0-9.]+$') {
    throw 'Firmware label must contain only digits and dots.'
}
if (-not (Get-Command ssh.exe -ErrorAction SilentlyContinue) -or
    -not (Get-Command scp.exe -ErrorAction SilentlyContinue)) {
    throw 'OpenSSH Client (ssh.exe and scp.exe) is required. Enable the Windows OpenSSH Client optional feature.'
}

$downloads = Join-Path $env:USERPROFILE 'Downloads'
if (-not (Test-Path -LiteralPath $downloads -PathType Container)) {
    New-Item -ItemType Directory -Path $downloads -ErrorAction Stop | Out-Null
}
$destination = Join-Path $downloads "LG-G4-QML-$FirmwareLabel"
if (Test-Path -LiteralPath $destination) {
    throw "Output already exists; refusing to overwrite: $destination"
}
$stage = Join-Path $downloads ".LG-G4-QML-$FirmwareLabel-$([guid]::NewGuid().ToString('N')).partial"

$relativePaths = @(
    'usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml',
    'usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml',
    'usr/lib/qml/InputPicker/Containers/RootView.qml',
    'usr/lib/qml/InputPicker/Containers/InputPickerListItem.qml',
    'usr/lib/qml/InputPicker/Containers/InputPickerListItemTile.qml',
    'usr/lib/qml/InputPicker/Containers/InputPickerMainView.qml',
    'usr/lib/qml/InputPicker/Controllers/AppInterfaces/Interfaces.qml',
    'usr/lib/qml/InputPicker/Controllers/ConnectionsInterfaces/Interfaces.qml',
    'usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/InputBanner.qml',
    'usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml',
    'usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/BgViewLayer/Screensaver.qml',
    'usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/BgViewLayer/ScreensaverMainText.qml'
)
$remoteCheck = @'
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
  /usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/MainViewLayer/InputBanner/SimpleBanner.qml \
  /usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/BgViewLayer/Screensaver.qml \
  /usr/palm/applications/com.webos.app.inputcommon/qml/UserInterfaceLayer/Containers/BgViewLayer/ScreensaverMainText.qml
do
  [ -f "$file" ] && [ ! -L "$file" ] || { echo "Missing/non-regular target file: $file" >&2; exit 1; }
  if awk -v target="$file" '$2 == target { found=1 } END { exit !found }' /proc/mounts; then
    echo "Refusing to copy a bind-mounted/overlaid file: $file" >&2
    exit 1
  fi
done
'@

& ssh.exe $Target $remoteCheck
if ($LASTEXITCODE -ne 0) {
    throw 'Read-only TV preflight failed; no files were copied.'
}

New-Item -ItemType Directory -Path $stage -ErrorAction Stop | Out-Null
try {
    foreach ($relative in $relativePaths) {
        $localPath = Join-Path $stage $relative.Replace('/', '\')
        $localDirectory = Split-Path -Parent $localPath
        New-Item -ItemType Directory -Path $localDirectory -Force -ErrorAction Stop | Out-Null
        $remotePath = "${Target}:/$relative"
        & scp.exe $remotePath $localPath
        if ($LASTEXITCODE -ne 0) {
            throw "SCP failed for $relative."
        }
    }

    $hashLines = foreach ($relative in $relativePaths) {
        $localPath = Join-Path $stage $relative.Replace('/', '\')
        $hash = Get-FileHash -LiteralPath $localPath -Algorithm SHA256
        "$($hash.Hash.ToLowerInvariant())  $relative"
    }
    Set-Content -LiteralPath (Join-Path $stage 'SOURCE-SHA256SUMS') -Value $hashLines -Encoding ASCII
    @(
        "User-reported firmware label: $FirmwareLabel"
        "Source: read-only root SSH/SCP from $Target"
        'The label is caller-supplied and was not independently verified.'
        'The collector refuses files currently bind-mounted at the source paths.'
        'No TV files were modified.'
    ) | Set-Content -LiteralPath (Join-Path $stage 'TARGET-INFO.txt') -Encoding ASCII

    Move-Item -LiteralPath $stage -Destination $destination -ErrorAction Stop
    $stage = $null
    Write-Output "Copied target QML files to: $destination"
    Write-Output 'The TV was not modified. Review TARGET-INFO.txt and SOURCE-SHA256SUMS.'
}
finally {
    if ($stage -and (Test-Path -LiteralPath $stage)) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
}
