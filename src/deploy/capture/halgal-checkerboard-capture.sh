set -eu
umask 077
capture_dir="$(mktemp -d /tmp/systemui-capture-XXXXXX)"
trap 'rm -rf "$capture_dir"' EXIT
capture_file="$capture_dir/capture.bgra"
metadata_file="$capture_dir/device-info.json"
node - "$metadata_file" <<'INFO_NODE'
var fs = require('fs');
var palmbus = require('/usr/lib/nodejs/palmbus.js');
var bus = new palmbus.Handle('', true);
var finished = false;
var timeout;
function finish(status, error) {
    if (finished) {
        return;
    }
    finished = true;
    clearTimeout(timeout);
    if (error) {
        process.stderr.write(error + '\n', function () {
            process.exit(status);
        });
    } else {
        process.exit(status);
    }
}
timeout = setTimeout(function () {
    finish(124, 'TV information service response timeout');
}, 15000);
try {
    var call = bus.call(
        'luna://com.webos.service.tv.systemproperty/getSystemInfo',
        JSON.stringify({ keys: ['modelName', 'firmwareVersion'] })
    );
    call.setResponseTimeout(15000);
    call.addListener('response', function (message) {
        if (finished) {
            return;
        }
        var result;
        try {
            result = JSON.parse(message.payload());
        } catch (error) {
            finish(1, 'TV information service returned an invalid response');
            return;
        }
        if (
            !result ||
            result.returnValue !== true ||
            typeof result.modelName !== 'string' ||
            !result.modelName.trim() ||
            typeof result.firmwareVersion !== 'string' ||
            !result.firmwareVersion.trim()
        ) {
            finish(1, 'TV did not return model and firmware information');
            return;
        }
        try {
            fs.writeFileSync(
                process.argv[2],
                JSON.stringify({
                    modelName: result.modelName,
                    firmwareVersion: result.firmwareVersion
                }),
                { mode: 0o600 }
            );
        } catch (error) {
            finish(1, 'Could not save TV metadata temporarily');
            return;
        }
        finish(0, null);
    });
} catch (error) {
    finish(1, 'Could not request TV model and firmware information');
}
INFO_NODE
if [ ! -s "$metadata_file" ]; then
    echo 'TV returned no model or firmware metadata' >&2
    exit 1
fi
metadata_size="$(wc -c < "$metadata_file" | tr -d '[:space:]')"
case "$metadata_size" in
    ''|*[!0-9]*)
        echo 'TV metadata had an invalid byte count' >&2
        exit 1
        ;;
esac
if [ "$metadata_size" -gt 4096 ]; then
    echo 'TV metadata exceeded the size limit' >&2
    exit 1
fi
metadata_json="$(cat "$metadata_file")"
printf 'CAPTURE_DEVICE_METADATA=%s\n' "$metadata_json" >&2
python3 - "$capture_file" "$1" <<'CAPTURE_PY'
import ctypes
import json
import mmap
import os
import sys

width, height = 1920, 1080
max_capture_bytes = int(sys.argv[2])

class Surface(ctypes.Structure):
    _fields_ = [
        ('offset', ctypes.c_uint32),
        ('physical_address', ctypes.POINTER(ctypes.c_uint32)),
        ('pitch', ctypes.c_uint16),
        ('bpp', ctypes.c_uint16),
        ('width', ctypes.c_uint16),
        ('height', ctypes.c_uint16),
        ('pixel_format', ctypes.c_uint16),
        ('palette_info', ctypes.c_uint32 * 257),
        ('property', ctypes.c_uint32),
        ('vendor_data', ctypes.c_uint32),
    ]

class Rect(ctypes.Structure):
    _fields_ = [
        ('x', ctypes.c_uint16),
        ('y', ctypes.c_uint16),
        ('w', ctypes.c_uint16),
        ('h', ctypes.c_uint16),
    ]

class DrawFlags(ctypes.Structure):
    _fields_ = [('pflag', ctypes.c_uint32)]

class DrawSettings(ctypes.Structure):
    _fields_ = [
        ('srcblending1', ctypes.c_uint32),
        ('dstblending2', ctypes.c_uint32),
        ('dstcolor', ctypes.c_uint32),
    ]

lib = ctypes.CDLL('/usr/lib/libhalgal.so.2')
lib.HAL_GAL_Init.restype = ctypes.c_int
lib.HAL_GAL_CreateSurface.argtypes = [
    ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32, ctypes.POINTER(Surface)
]
lib.HAL_GAL_CreateSurface.restype = ctypes.c_int
lib.HAL_GAL_FillRectangle.argtypes = [
    ctypes.POINTER(Surface), ctypes.POINTER(Rect), ctypes.c_uint32,
    ctypes.POINTER(DrawFlags), ctypes.POINTER(DrawSettings)
]
lib.HAL_GAL_FillRectangle.restype = ctypes.c_int
lib.HAL_GAL_CaptureFrameBuffer.argtypes = [ctypes.POINTER(Surface)]
lib.HAL_GAL_CaptureFrameBuffer.restype = ctypes.c_int
lib.HAL_GAL_DestroySurface.argtypes = [ctypes.POINTER(Surface)]
lib.HAL_GAL_DestroySurface.restype = ctypes.c_int

status = lib.HAL_GAL_Init()
if status != 0:
    raise SystemExit('HAL_GAL_Init failed')

surface = Surface()
created = False
fd = -1
mapped = None
try:
    status = lib.HAL_GAL_CreateSurface(width, height, 0, ctypes.byref(surface))
    if status != 0:
        raise SystemExit(f'HAL_GAL_CreateSurface failed: {status}')
    created = True

    flags = DrawFlags(0)
    settings = DrawSettings(2, 0, 0)
    tile = 48
    for y in range(0, height, tile):
        for x in range(0, width, tile):
            rect = Rect(x, y, min(tile, width - x), min(tile, height - y))
            color = 0xffb0b0b0 if ((x // tile + y // tile) & 1) else 0xff383838
            status = lib.HAL_GAL_FillRectangle(
                ctypes.byref(surface), ctypes.byref(rect), color,
                ctypes.byref(flags), ctypes.byref(settings)
            )
            if status != 0:
                raise SystemExit(f'HAL_GAL_FillRectangle failed: {status}')

    status = lib.HAL_GAL_CaptureFrameBuffer(ctypes.byref(surface))
    if status != 0:
        raise SystemExit(f'HAL_GAL_CaptureFrameBuffer failed: {status}')

    length = surface.property or surface.height * surface.pitch
    if surface.width != width or surface.height != height or surface.bpp != 32:
        raise SystemExit(
            f'Unexpected surface: {surface.width}x{surface.height}, '
            f'{surface.bpp} bpp, pixel format {surface.pixel_format}'
        )
    if surface.pixel_format != 0:
        raise SystemExit(f'Unsupported HAL_GAL pixel format: {surface.pixel_format}')
    if surface.pitch < width * 4 or length < surface.pitch * height:
        raise SystemExit(
            f'Unexpected surface layout: pitch={surface.pitch}, length={length}'
        )
    if length > max_capture_bytes:
        raise SystemExit('HAL_GAL surface exceeded the size limit')

    fd = os.open('/dev/gfx', os.O_RDWR)
    mapped = mmap.mmap(
        fd, length, flags=mmap.MAP_SHARED,
        prot=mmap.PROT_READ | mmap.PROT_WRITE, offset=surface.offset
    )
    with open(sys.argv[1], 'wb') as capture:
        capture.write(mapped)
    print(
        'CAPTURE_SURFACE_METADATA=' + json.dumps({
            'width': surface.width,
            'height': surface.height,
            'pitch': surface.pitch,
            'bpp': surface.bpp,
            'pixelFormat': surface.pixel_format,
            'length': length
        }, separators=(',', ':')),
        file=sys.stderr,
        flush=True
    )
finally:
    if mapped is not None:
        mapped.close()
    if fd >= 0:
        os.close(fd)
    if created:
        lib.HAL_GAL_DestroySurface(ctypes.byref(surface))
CAPTURE_PY
capture_size="$(wc -c < "$capture_file" | tr -d '[:space:]')"
case "$capture_size" in
    ''|*[!0-9]*)
        echo 'HAL_GAL capture returned an invalid byte count' >&2
        exit 1
        ;;
esac
if [ "$capture_size" -gt "$1" ]; then
    echo 'HAL_GAL capture exceeded the size limit' >&2
    exit 1
fi
cat "$capture_file"
