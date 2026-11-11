#!/usr/bin/env bash
# Runs the Linux build for real: core commands, then the window on a virtual screen.
set -u
BIN="$(realpath "$1")"
OUT="$(realpath -m "$2")"
mkdir -p "$OUT"
FAIL=0
WORK="$(mktemp -d)"
export FORKBAY_ROOT="$WORK/Files"
export HOME="$WORK/home"
export FORKBAY_LANG=fr
if strings "$BIN" | grep -q "/home/runner"; then echo "FAIL local paths inside the program"; FAIL=1; else echo "ok   no local path inside the program"; fi
mkdir -p "$FORKBAY_ROOT/Roms" "$HOME"
: > "$FORKBAY_ROOT/Roms/Test Game [0400000000000000].nup"

check() { # name, command, args, jq-free text expected in the result
    "$BIN" --cli "$2" "$3" "$WORK/r.json"
    if grep -q "$4" "$WORK/r.json"; then echo "ok   $1"; else echo "FAIL $1: $(cat "$WORK/r.json")"; FAIL=1; fi
}

check "state" state '{}' '"ok":true'
check "one game found" state '{}' 'Test Game'
test -L "$FORKBAY_ROOT/Data" && { echo "FAIL Data is a link"; FAIL=1; }
test -d "$FORKBAY_ROOT/Data/nand/user/save" && echo "ok   data folders" || { echo "FAIL data folders"; FAIL=1; }

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
test -f "$FORKBAY_ROOT/Data/nand/user/save/0/P/T1/a.bin" && echo "ok   save copied" || { echo "FAIL save copied"; FAIL=1; }
test ! -e "$FORKBAY_ROOT/Data/load/X/m.bin" && echo "ok   unticked mods skipped" || { echo "FAIL mods copied"; FAIL=1; }
grep -q 'vibration=1' "$FORKBAY_ROOT/Data/config/qt-config.ini" && echo "ok   controls merged" || { echo "FAIL controls"; FAIL=1; }
grep -q 'net=2' "$FORKBAY_ROOT/Data/config/qt-config.ini" && { echo "FAIL network copied"; FAIL=1; } || echo "ok   unticked network skipped"

# An emulator installed in the system (a distribution package) is used where it is, never copied
check "system emulator used in place" emu_add_local '{"name":"SysEmu","exe":"/usr/bin/true"}' 'sur place'
grep -qx '/usr/bin/true' "$FORKBAY_ROOT/SysEmu/exe.txt" && echo "ok   started by its full path" || { echo "FAIL exe path: $(cat "$FORKBAY_ROOT/SysEmu/exe.txt")"; FAIL=1; }
test "$(find "$FORKBAY_ROOT/SysEmu" -mindepth 1 | wc -l)" -le 3 && echo "ok   system folder not copied" || { echo "FAIL system folder copied"; FAIL=1; }
check "system emulator present" emu_status '{}' '"name":"SysEmu"'

# Cloud through a synced folder
mkdir -p "$WORK/cloud"
check "cloud settings" cloud_save "{\"enabled\":false,\"provider\":\"folder\",\"folderPath\":\"$WORK/cloud\",\"keepBackups\":3}" '"ok":true'
check "first upload" cloud_sync '{}' 'envoyée'
check "nothing to do" cloud_sync '{}' 'identiques'
test -f "$WORK/cloud/ForkbaySync/forkbay-sync.zip" && echo "ok   archive written" || { echo "FAIL archive"; FAIL=1; }

# Steam shortcuts on a fake Steam folder
export FORKBAY_STEAM="$WORK/steam"
mkdir -p "$FORKBAY_STEAM/userdata/1/config"
check "steam save" steam_save '{"account":"1","games":["Test Game [0400000000000000].nup"]}' 'Enregistr'
check "steam reload" steam_load '{"account":"1"}' 'Test Game'
unset FORKBAY_STEAM

# Versions from the network
check "emulator status" emu_status '{}' '"name":"Eglantine"'

# Bug report
mkdir -p "$FORKBAY_ROOT/Data/log"
printf '%s\n' "$HOME/secret/game.nup" "/home/otherperson/file" "header_key = AABBCC" "mail me@example.com" "build_id=BCF5E512" > "$FORKBAY_ROOT/Data/log/eden_log.txt"
"$BIN" --cli report_collect '{"game":"","log":true}' "$WORK/rep.json"
if grep -qE "$HOME|otherperson|AABBCC|example.com" "$WORK/rep.json"; then echo "FAIL report leaks: $(cat "$WORK/rep.json")"; FAIL=1; else echo "ok   report cleaned"; fi
grep -q 'BCF5E512' "$WORK/rep.json" && echo "ok   report keeps data" || { echo "FAIL report data"; FAIL=1; }

# Language
FORKBAY_LANG=en_US.UTF-8 "$BIN" --cli install_keys "{\"path\":\"$WORK/r.json\"}" "$WORK/en.json"
grep -q "Choose your prod.keys" "$WORK/en.json" && echo "ok   english messages" || { echo "FAIL english: $(cat "$WORK/en.json")"; FAIL=1; }
# Game folders, images, Steam mode and first start
mkdir -p "$WORK/games/Sub"; echo x > "$WORK/games/Sub/Other Game [0400000000000001].nsp"
"$BIN" --cli game_dir_add "{\"path\":\"$WORK/games\"}" "$WORK/o.json"; grep -q '"ok":true' "$WORK/o.json" && echo "ok   games folder added" || { echo "FAIL games folder: $(cat "$WORK/o.json")"; FAIL=1; }
"$BIN" --cli state '{}' "$WORK/o.json"; grep -q 'Other Game' "$WORK/o.json" && echo "ok   game found in a subfolder" || { echo "FAIL game in folder: $(cat "$WORK/o.json")"; FAIL=1; }
printf '\377\330\377\340' > "$WORK/a.jpg"; G="$WORK/games/Sub/Other Game [0400000000000001].nsp"; "$BIN" --cli set_image "{\"file\":\"$G\",\"kind\":\"grid\",\"path\":\"$WORK/a.jpg\"}" "$WORK/o.json"; grep -q '"ok":true' "$WORK/o.json" && echo "ok   image named automatically" || { echo "FAIL image: $(cat "$WORK/o.json")"; FAIL=1; }
"$BIN" --cli game_dir_remove "{\"path\":\"$WORK/games\"}" "$WORK/o.json"
SteamGamepadUI=1 "$BIN" --cli steam_ui_detect '{}' "$WORK/o.json"; grep -q '"active":true' "$WORK/o.json" && echo "ok   Steam full-screen mode detected" || { echo "FAIL steam mode: $(cat "$WORK/o.json")"; FAIL=1; }
env -u SteamGamepadUI -u GAMESCOPE_WAYLAND_DISPLAY XDG_CURRENT_DESKTOP=GNOME "$BIN" --cli steam_ui_detect '{}' "$WORK/o.json"; grep -q '"active":false' "$WORK/o.json" && echo "ok   desktop mode detected" || { echo "FAIL desktop mode: $(cat "$WORK/o.json")"; FAIL=1; }
mkdir -p "$WORK/dl"; cp "$BIN" "$WORK/dl/nx2-launcher-linux-x86_64"; OLD="$WORK/dl/nx2-launcher-linux-x86_64"
I="env -u FORKBAY_ROOT FORKBAY_INSTALLTEST=1 FORKBAY_SHORTCUT_DIR=$WORK/links"
$I "$OLD" --cli state '{}' "$WORK/o.json"; grep -q '"installPrompt":true' "$WORK/o.json" && echo "ok   new copy asks where to go" || { echo "FAIL first start: $(cat "$WORK/o.json")"; FAIL=1; }
$I "$OLD" --cli install_to "{\"kind\":\"custom\",\"custom\":\"$WORK/apps\",\"startMenu\":true,\"desktop\":false}" "$WORK/o.json"; [ -x "$WORK/apps/NX2 Launcher/nx2-launcher" ] && [ -f "$WORK/links/menu/nx2-launcher.desktop" ] && echo "ok   program copied, shortcut created" || { echo "FAIL install: $(cat "$WORK/o.json")"; FAIL=1; }
$I "$WORK/apps/NX2 Launcher/nx2-launcher" --cleanup-install "$OLD" --version "$WORK/o.txt"; [ ! -e "$OLD" ] && echo "ok   old copy removed" || { echo "FAIL old copy still there"; FAIL=1; }
FORKBAY_LANG=fr_FR.UTF-8 "$BIN" --cli state '{}' "$WORK/fr.json"
grep -q '"language":"fr"' "$WORK/fr.json" && echo "ok   french detected" || { echo "FAIL french detection"; FAIL=1; }

# Window on a virtual screen
Xvfb :99 -screen 0 1280x860x24 >/dev/null 2>&1 &
XVFB=$!
sleep 2
export DISPLAY=:99
export FORKBAY_SELFTEST="$WORK/page.txt"
export FORKBAY_SECTEST=1
dbus-run-session -- "$BIN" > "$OUT/stdout.txt" 2>&1 &
APP=$!
sleep 25
xwininfo -root -tree > "$OUT/windows.txt" 2>&1 || true
import -window root "$OUT/first-run.png" || true
if kill -0 "$APP" 2>/dev/null; then echo "ok   window still running"; else echo "FAIL window exited"; FAIL=1; fi
if [ -f "$FORKBAY_SELFTEST" ]; then echo "ok   page script reached the program"; else echo "FAIL page never loaded"; FAIL=1; fi
if grep -q "csp=blocked nav=blocked" "$FORKBAY_SELFTEST" 2>/dev/null; then echo "ok   page cannot reach or navigate to the web"; else echo "FAIL page protection: $(cat "$FORKBAY_SELFTEST" 2>/dev/null)"; FAIL=1; fi
if grep -q "NX2 Launcher" "$OUT/windows.txt"; then echo "ok   window on screen"; else echo "FAIL no window on screen"; FAIL=1; fi
COLORS=$(identify -format '%k' "$OUT/first-run.png" 2>/dev/null || echo 0)
if [ "${COLORS:-0}" -gt 50 ]; then echo "ok   image has content ($COLORS colours)"; else echo "FAIL blank image ($COLORS colours)"; FAIL=1; fi
kill "$APP" "$XVFB" 2>/dev/null
exit $FAIL
