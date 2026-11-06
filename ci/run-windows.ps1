# Runs the core commands of a Windows build on throwaway folders.
param([string]$Bin)
$Bin = (Resolve-Path -LiteralPath $Bin).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ('s2m-' + [guid]::NewGuid().ToString('N'))
$script:fail = 0
$rootA = "$work\A\Files"; $rootB = "$work\B\Files"
New-Item -ItemType Directory -Force "$rootA\Roms", "$rootB\Roms", "$work\search", "$work\cloud", "$work\steam\userdata\1\config" | Out-Null
[IO.File]::WriteAllText("$rootA\Roms\Test Game [0400000000000000].nup", 'x')
[IO.File]::WriteAllText("$rootB\Roms\Test Game [0400000000000000].nup", 'x')

function App([string]$root, [string]$cmd, $arg = @{}) {
    $env:S2M_ROOT = $root; $env:S2M_SEARCH = "$work\search"; $env:S2M_STEAM = "$work\steam"
    $file = "$work\r.json"
    if (Test-Path $file) { Remove-Item $file }
    $json = ($arg | ConvertTo-Json -Compress -Depth 5).Replace('"', '\"')
    Start-Process -FilePath $Bin -ArgumentList '--cli', $cmd, "`"$json`"", "`"$file`"" -Wait -NoNewWindow
    return Get-Content -LiteralPath $file -Raw -Encoding UTF8 | ConvertFrom-Json
}
function Check([string]$name, $ok, $detail = '') {
    if ($ok) { "ok   $name" } else { "FAIL $name $detail"; $script:fail = 1 }
}

$r = App $rootA 'state'; Check 'state' ($r.ok -and $r.value.games.Count -eq 1) ($r | ConvertTo-Json -Compress)
Check 'wizard pending' (-not $r.value.setupDone)
Check 'no keys yet' (-not $r.value.keys)

"-- keys and firmware"
[IO.File]::WriteAllText("$work\prod.keys", "header_key = 00`n"); [IO.File]::WriteAllText("$work\title.keys", "a = b`n"); [IO.File]::WriteAllText("$work\bad.keys", "hello`n")
$r = App $rootA 'install_keys' @{ path = "$work\bad.keys" }; Check 'bad keys refused' (-not $r.ok)
$r = App $rootA 'install_keys' @{ path = "$work\prod.keys" }; Check 'keys installed' $r.ok $r.error
Check 'title.keys copied' (Test-Path "$rootA\Data\keys\title.keys")
New-Item -ItemType Directory "$work\fw" | Out-Null; [IO.File]::WriteAllText("$work\fw\a.nca", 'n'); [IO.File]::WriteAllText("$work\fw\b.txt", 'n')
Compress-Archive -Path "$work\fw\*" -DestinationPath "$work\fw.zip"
$r = App $rootA 'install_firmware' @{ path = "$work\fw.zip" }; Check 'firmware zip' ($r.ok -and (Test-Path "$rootA\Data\nand\system\Contents\registered\a.nca")) $r.error
Check 'non-nca skipped' (-not (Test-Path "$rootA\Data\nand\system\Contents\registered\b.txt"))
[IO.File]::WriteAllText("$work\empty.zip", 'not a zip')
$r = App $rootA 'install_firmware' @{ path = "$work\empty.zip" }; Check 'bad firmware refused' (-not $r.ok)
$r = App $rootA 'state'; Check 'status updated' ($r.value.keys -and $r.value.firmware -eq 1)

"-- import by category"
$src = "$work\search\yuzu"
New-Item -ItemType Directory -Force "$src\nand\user\save\0\P\T1", "$src\keys", "$src\load\X", "$src\config", "$src\nand\system\save\8000000000000010" | Out-Null
[IO.File]::WriteAllText("$src\nand\user\save\0\P\T1\a.bin", 'save'); [IO.File]::WriteAllText("$src\keys\prod.keys", 'other'); [IO.File]::WriteAllText("$src\load\X\m.bin", 'mod')
[IO.File]::WriteAllText("$src\nand\system\save\8000000000000010\p.dat", 'profile')
[IO.File]::WriteAllText("$src\config\qt-config.ini", "[Controls]`nvibration=1`n[Network]`nnet=2`n[Renderer]`nfx=3`n")
$before = (Get-ChildItem $src -Recurse -File | ForEach-Object { (Get-FileHash $_.FullName).Hash } | Sort-Object) -join ''
$r = App $rootA 'sources'; $s = @($r.value)[0]; Check 'source found' ($s.name -eq 'yuzu')
$ids = @($s.categories | ForEach-Object id); foreach ($c in 'keys', 'saves', 'profiles', 'controls', 'network', 'graphics', 'mods') { Check "category $c" ($ids -contains $c) }
$r = App $rootA 'import' @{ path = $src; cats = @('saves', 'profiles', 'keys', 'network') }; Check 'import' $r.ok $r.error; "     $($r.value.message)"
Check 'save copied' (Test-Path "$rootA\Data\nand\user\save\0\P\T1\a.bin")
Check 'profile copied' (Test-Path "$rootA\Data\nand\system\save\8000000000000010\p.dat")
Check 'keys not overwritten' ((Get-Content "$rootA\Data\keys\prod.keys") -match 'header_key')
Check 'unticked mods skipped' (-not (Test-Path "$rootA\Data\load\X\m.bin"))
$ini = Get-Content "$rootA\Data\config\qt-config.ini" -Raw
Check 'network imported' ($ini -match 'net=2'); Check 'unticked controls skipped' ($ini -notmatch 'vibration'); Check 'unticked graphics skipped' ($ini -notmatch 'fx=3')
Check 'source untouched' ($before -eq ((Get-ChildItem $src -Recurse -File | ForEach-Object { (Get-FileHash $_.FullName).Hash } | Sort-Object) -join ''))

"-- cloud through a folder"
foreach ($root in $rootA, $rootB) { $r = App $root 'cloud_save' @{ enabled = $false; provider = 'folder'; folderPath = "$work\cloud"; keepBackups = 3 }; Check 'cloud settings' $r.ok $r.error }
$r = App $rootA 'cloud_sync'; Check 'first upload' ($r.value.message -match 'envoy') ($r | ConvertTo-Json -Compress)
$r = App $rootA 'cloud_sync'; Check 'nothing to do' ($r.value.message -match 'identiques')
$ini = "$rootA\Data\config\qt-config.ini"; Add-Content $ini "`nPaths\recentFiles=`"other`"";
$r = App $rootA 'cloud_sync'; "     after a first recentFiles line: $($r.value.message)"
$t = Get-Content $ini -Raw; [IO.File]::WriteAllText($ini, $t.Replace('Paths\recentFiles="other"', 'Paths\recentFiles="changed"'))
$r = App $rootA 'cloud_sync'; Check 'recentFiles alone ignored' ($r.value.message -match 'identiques') ($r | ConvertTo-Json -Compress)
Add-Type -AssemblyName System.IO.Compression.FileSystem
$z = [IO.Compression.ZipFile]::OpenRead("$work\cloud\Switch2Sync\switch2-sync.zip"); $names = $z.Entries | ForEach-Object FullName; $z.Dispose()
Check 'keys never sent' (-not ($names -match 'keys')); Check 'firmware never sent' (-not ($names -match '\.nca')); Check 'profile sent' ($names -match 'profiles|8000000000000010')
$s = "$rootA\Data\nand\user\save\0\P\T1\a.bin"; [IO.File]::WriteAllText($s, 'played-on-A')
$r = App $rootA 'cloud_sync'; Check 'local change sent' ($r.value.message -match 'envoy')
"-- second PC"
$env:S2M_CONFLICT = 'cancel'
$r = App $rootB 'cloud_sync'; Check 'first exchange asks, cancel keeps local' ($r.value.message -match 'annul') ($r | ConvertTo-Json -Compress)
$env:S2M_CONFLICT = 'remote'
$r = App $rootB 'cloud_sync'; Check 'choose remote' ($r.value.message -match 'distante') ($r | ConvertTo-Json -Compress)
Check 'B received the save' ((Get-Content "$rootB\Data\nand\user\save\0\P\T1\a.bin") -eq 'played-on-A')
Check 'B paths are its own' ((Get-Content "$rootB\Data\config\qt-config.ini" -Raw) -notmatch '__LOCAL__')
Check 'B backup made' (@(Get-ChildItem "$rootB\Save Backups" -Directory).Count -ge 1)
Remove-Item Env:\S2M_CONFLICT
$r = App $rootB 'cloud_sync'; Check 'B now identical' ($r.value.message -match 'identiques') ($r | ConvertTo-Json -Compress)
[IO.File]::WriteAllText($s, 'A-again'); $r = App $rootA 'cloud_sync'; Check 'A sends' ($r.value.message -match 'envoy')
$r = App $rootB 'cloud_sync'; Check 'B replaced automatically' ($r.value.message -match 'distante') ($r | ConvertTo-Json -Compress)
Check 'B content' ((Get-Content "$rootB\Data\nand\user\save\0\P\T1\a.bin") -eq 'A-again')
$sb = "$rootB\Data\nand\user\save\0\P\T1\a.bin"; [IO.File]::WriteAllText($s, 'A-3'); [IO.File]::WriteAllText($sb, 'B-3'); $r = App $rootA 'cloud_sync'
$env:S2M_CONFLICT = 'local'; $r = App $rootB 'cloud_sync'; Check 'conflict keep local' ($r.value.message -match 'envoy') ($r | ConvertTo-Json -Compress); Remove-Item Env:\S2M_CONFLICT
$r = App $rootA 'cloud_sync'; Check 'A receives B' ((Get-Content $s) -eq 'B-3') ($r | ConvertTo-Json -Compress)
$m = "$work\cloud\Switch2Sync\meta.json"; $j = Get-Content $m -Raw | ConvertFrom-Json; $j.hash = 'AAAA'; [IO.File]::WriteAllText($m, ($j | ConvertTo-Json))
$r = App $rootA 'cloud_sync'; Check 'bad remote rolled back' ((-not $r.ok) -and (Get-Content $s) -eq 'B-3') ($r | ConvertTo-Json -Compress)
$j.hash = (Get-Content "$rootA\Data\sync-state.json" -Raw | ConvertFrom-Json).lastHash; [IO.File]::WriteAllText($m, ($j | ConvertTo-Json))
"-- steam on a fake folder"
[IO.File]::WriteAllBytes("$work\steam\userdata\1\config\shortcuts.vdf", [byte[]](0, 115, 104, 111, 114, 116, 99, 117, 116, 115, 0, 8, 8))
$g = 'Test Game [0400000000000000].nup'
$r = App $rootA 'steam_save' @{ account = '1'; games = @($g) }; Check 'steam save' $r.ok $r.error
$r = App $rootA 'steam_load' @{ account = '1' }; Check 'steam managed' (@($r.value.managed) -contains $g) ($r | ConvertTo-Json -Compress); Check 'steam current' ($r.value.stale -eq 0)
$v = "$work\steam\userdata\1\config\shortcuts.vdf"; Check 'steam backup' (Test-Path "$v.before-switch2")
[IO.File]::WriteAllBytes($v, [byte[]](1, 2, 3)); $h = (Get-FileHash $v).Hash
$r = App $rootA 'steam_save' @{ account = '1'; games = @($g) }; Check 'corrupt vdf refused' ((-not $r.ok) -and $h -eq (Get-FileHash $v).Hash)

"-- emulators"
$r = App $rootA 'emu_status'; Check 'status list' (@($r.value).Count -ge 5); @($r.value) | ForEach-Object { "     $($_.name): latest=$($_.latest) err=[$($_.error)]" }
$r = App $rootA 'emu_add_github' @{ name = '..\evil'; repo = 'a/b' }; Check 'bad name refused' (-not $r.ok)
$r = App $rootA 'emu_add_github' @{ name = 'Lichii'; repo = 'a/b' }; Check 'duplicate refused' (-not $r.ok)
$r = App $rootA 'emu_add_github' @{ name = 'X'; repo = 'nope nope' }; Check 'bad repo refused' (-not $r.ok)

"-- data move"
$r = App $rootA 'move_data' @{ location = 'custom'; custom = "$work\Moved\Data" }; Check 'move' $r.ok $r.error; "     $($r.value.message)"
Check 'moved save' ((Get-Content "$work\Moved\Data\nand\user\save\0\P\T1\a.bin") -eq 'B-3')
Check 'config rewritten' ((Get-Content "$work\Moved\Data\config\qt-config.ini" -Raw) -notmatch [regex]::Escape($rootA.Replace('\', '/') + '/Data'))
Check 'old copy kept' (Test-Path "$rootA\Data.old")

"-- art without a key"
$env:S2M_ROOT = $rootA
[IO.File]::WriteAllText("$rootA\Roms\Pokemon Pokopia [0400A3A01EE4C000].nup", 'x'); [IO.File]::WriteAllText("$rootA\Roms\Fire Emblem Fortunes Weave [04007EF01F8AE000].nup", 'x'); [IO.File]::WriteAllText("$rootA\Roms\The Legend of Zelda Ocarina of Time [0400486024DEC000][v131072].nup", 'x')
$r = App $rootA 'fetch_art'; Check 'art' $r.ok $r.error; "     $($r.value.message)"
Get-ChildItem "$rootA\Covers" -ErrorAction SilentlyContinue | ForEach-Object { "     $($_.Name) $($_.Length)" }
$r = App $rootA 'state'; Check 'cover in state' (@($r.value.games | Where-Object cover).Count -ge 1)

Remove-Item Env:\S2M_ROOT, Env:\S2M_SEARCH, Env:\S2M_STEAM -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($script:fail) { 'RESULT: FAILED' } else { 'RESULT: all passed' }
exit $script:fail
