#!/usr/bin/env bash
# Runs the Linux build for real: core commands, then the window on a virtual screen.
set -u
BIN="$(realpath "$1")"
OUT="$(realpath -m "$2")"
mkdir -p "$OUT"
WORK="$(mktemp -d)"
export S2M_ROOT="$WORK/Files"
export HOME="$WORK/home"
mkdir -p "$S2M_ROOT/Roms" "$HOME"
: > "$S2M_ROOT/Roms/Test Game [0400000000000000].nup"
FAIL=0

check() { # name, command, args, jq-free text expected in the result
    "$BIN" --cli "$2" "$3" "$WORK/r.json"
    if grep -q "$4" "$WORK/r.json"; then echo "ok   $1"; else echo "FAIL $1: $(cat "$WORK/r.json")"; FAIL=1; fi
}

check "state" state '{}' '"ok":true'
check "one game found" state '{}' 'Test Game'
test -L "$S2M_ROOT/Data" && { echo "FAIL Data is a link"; FAIL=1; }
test -d "$S2M_ROOT/Data/nand/user/save" && echo "ok   data folders" || { echo "FAIL data folders"; FAIL=1; }

# Keys
printf 'header_key = 00\n' > "$WORK/prod.keys"
check "keys install" install_keys "{\"path\":\"$WORK/prod.keys\"}" 'installé'
check "wrong keys refused" install_keys "{\"path\":\"$WORK/r.json\"}" '"ok":false'

# Import by category
SRC="$HOME/.local/share/yuzu"
mkdir -p "$SRC/nand/user/save/0/P/T1" "$SRC/load/X" "$HOME/.config/yuzu"
echo save > "$SRC/nand/user/save/0/P/T1/a.bin"
echo mod > "$SRC/load/X/m.bin"
printf '[Controls]\nvibration=1\n[Network]\nnet=2\n' > "$HOME/.config/yuzu/qt-config.ini"
check "source detected" sources '{}' '"name":"yuzu"'
check "network category" sources '{}' '"id":"network"'
check "import saves only" import "{\"path\":\"$SRC\",\"cats\":[\"saves\",\"controls\"]}" 'Importé'
test -f "$S2M_ROOT/Data/nand/user/save/0/P/T1/a.bin" && echo "ok   save copied" || { echo "FAIL save copied"; FAIL=1; }
test ! -e "$S2M_ROOT/Data/load/X/m.bin" && echo "ok   unticked mods skipped" || { echo "FAIL mods copied"; FAIL=1; }
grep -q 'vibration=1' "$S2M_ROOT/Data/config/qt-config.ini" && echo "ok   controls merged" || { echo "FAIL controls"; FAIL=1; }
grep -q 'net=2' "$S2M_ROOT/Data/config/qt-config.ini" && { echo "FAIL network copied"; FAIL=1; } || echo "ok   unticked network skipped"

# Cloud through a synced folder
mkdir -p "$WORK/cloud"
check "cloud settings" cloud_save "{\"enabled\":false,\"provider\":\"folder\",\"folderPath\":\"$WORK/cloud\",\"keepBackups\":3}" '"ok":true'
check "first upload" cloud_sync '{}' 'envoyée'
check "nothing to do" cloud_sync '{}' 'identiques'
test -f "$WORK/cloud/Switch2Sync/switch2-sync.zip" && echo "ok   archive written" || { echo "FAIL archive"; FAIL=1; }

# Steam shortcuts on a fake Steam folder
export S2M_STEAM="$WORK/steam"
mkdir -p "$S2M_STEAM/userdata/1/config"
check "steam save" steam_save '{"account":"1","games":["Test Game [0400000000000000].nup"]}' 'Enregistr'
check "steam reload" steam_load '{"account":"1"}' 'Test Game'
unset S2M_STEAM

# Versions from the network
check "emulator status" emu_status '{}' '"name":"Eglantine"'

# Window on a virtual screen
Xvfb :99 -screen 0 1280x860x24 >/dev/null 2>&1 &
XVFB=$!
sleep 2
export DISPLAY=:99
"$BIN" &
APP=$!
sleep 15
import -window root "$OUT/first-run.png" || true
if kill -0 "$APP" 2>/dev/null; then echo "ok   window still running"; else echo "FAIL window exited"; FAIL=1; fi
kill "$APP" "$XVFB" 2>/dev/null
exit $FAIL
