# Runs the core commands of a Windows build on throwaway folders.
param([string]$Bin)
$Bin = (Resolve-Path -LiteralPath $Bin).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ('forkbay-' + [guid]::NewGuid().ToString('N'))
$script:fail = 0
$rootA = "$work\A\Files"; $rootB = "$work\B\Files"
New-Item -ItemType Directory -Force "$rootA\Roms", "$rootB\Roms", "$work\search", "$work\cloud", "$work\steam\userdata\1\config" | Out-Null
[IO.File]::WriteAllText("$rootA\Roms\Test Game [0400000000000000].nup", 'x')
[IO.File]::WriteAllText("$rootB\Roms\Test Game [0400000000000000].nup", 'x')

function App([string]$root, [string]$cmd, $arg = @{}) {
    $env:FORKBAY_LANG = 'fr'; $env:FORKBAY_ROOT = $root; $env:FORKBAY_SEARCH = "$work\search"; $env:FORKBAY_STEAM = "$work\steam"
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
$z = [IO.Compression.ZipFile]::OpenRead("$work\cloud\ForkbaySync\forkbay-sync.zip"); $names = $z.Entries | ForEach-Object FullName; $z.Dispose()
Check 'keys never sent' (-not ($names -match 'keys')); Check 'firmware never sent' (-not ($names -match '\.nca')); Check 'profile sent' ($names -match 'profiles|8000000000000010')
$s = "$rootA\Data\nand\user\save\0\P\T1\a.bin"; [IO.File]::WriteAllText($s, 'played-on-A')
$r = App $rootA 'cloud_sync'; Check 'local change sent' ($r.value.message -match 'envoy')
"-- second PC"
$env:FORKBAY_CONFLICT = 'cancel'
$r = App $rootB 'cloud_sync'; Check 'first exchange asks, cancel keeps local' ($r.value.message -match 'annul') ($r | ConvertTo-Json -Compress)
$env:FORKBAY_CONFLICT = 'remote'
$r = App $rootB 'cloud_sync'; Check 'choose remote' ($r.value.message -match 'du cloud') ($r | ConvertTo-Json -Compress)
Check 'B received the save' ((Get-Content "$rootB\Data\nand\user\save\0\P\T1\a.bin") -eq 'played-on-A')
Check 'B paths are its own' ((Get-Content "$rootB\Data\config\qt-config.ini" -Raw) -notmatch '__LOCAL__')
Check 'B backup made' (@(Get-ChildItem "$rootB\Save Backups" -Directory).Count -ge 1)
Remove-Item Env:\FORKBAY_CONFLICT
$r = App $rootB 'cloud_sync'; Check 'B now identical' ($r.value.message -match 'identiques') ($r | ConvertTo-Json -Compress)
[IO.File]::WriteAllText($s, 'A-again'); $r = App $rootA 'cloud_sync'; Check 'A sends' ($r.value.message -match 'envoy')
$r = App $rootB 'cloud_sync'; Check 'B replaced automatically' ($r.value.message -match 'du cloud') ($r | ConvertTo-Json -Compress)
Check 'B content' ((Get-Content "$rootB\Data\nand\user\save\0\P\T1\a.bin") -eq 'A-again')
$sb = "$rootB\Data\nand\user\save\0\P\T1\a.bin"; [IO.File]::WriteAllText($s, 'A-3'); [IO.File]::WriteAllText($sb, 'B-3'); $r = App $rootA 'cloud_sync'
$env:FORKBAY_CONFLICT = 'local'; $r = App $rootB 'cloud_sync'; Check 'conflict keep local' ($r.value.message -match 'envoy') ($r | ConvertTo-Json -Compress); Remove-Item Env:\FORKBAY_CONFLICT
$r = App $rootA 'cloud_sync'; Check 'A receives B' ((Get-Content $s) -eq 'B-3') ($r | ConvertTo-Json -Compress)
$m = "$work\cloud\ForkbaySync\meta.json"; $j = Get-Content $m -Raw | ConvertFrom-Json; $j.hash = 'AAAA'; [IO.File]::WriteAllText($m, ($j | ConvertTo-Json))
$r = App $rootA 'cloud_sync'; Check 'bad remote rolled back' ((-not $r.ok) -and (Get-Content $s) -eq 'B-3') ($r | ConvertTo-Json -Compress)
$j.hash = (Get-Content "$rootA\Data\sync-state.json" -Raw | ConvertFrom-Json).lastHash; [IO.File]::WriteAllText($m, ($j | ConvertTo-Json))
"-- steam on a fake folder"
[IO.File]::WriteAllBytes("$work\steam\userdata\1\config\shortcuts.vdf", [byte[]](0, 115, 104, 111, 114, 116, 99, 117, 116, 115, 0, 8, 8))
$g = 'Test Game [0400000000000000].nup'
$r = App $rootA 'steam_save' @{ account = '1'; games = @($g) }; Check 'steam save' $r.ok $r.error
$r = App $rootA 'steam_load' @{ account = '1' }; Check 'steam managed' (@($r.value.managed) -contains $g) ($r | ConvertTo-Json -Compress); Check 'steam current' ($r.value.stale -eq 0)
$v = "$work\steam\userdata\1\config\shortcuts.vdf"; Check 'steam backup' (Test-Path "$v.before-forkbay")
[IO.File]::WriteAllBytes($v, [byte[]](1, 2, 3)); $h = (Get-FileHash $v).Hash
$r = App $rootA 'steam_save' @{ account = '1'; games = @($g) }; Check 'corrupt vdf refused' ((-not $r.ok) -and $h -eq (Get-FileHash $v).Hash)

"-- emulators"
$r = App $rootA 'emu_status'; Check 'status list' (@($r.value).Count -ge 5); @($r.value) | ForEach-Object { "     $($_.name): latest=$($_.latest) err=[$($_.error)]" }
$r = App $rootA 'emu_add_github' @{ name = '..\evil'; repo = 'a/b' }; Check 'bad name refused' (-not $r.ok)
$r = App $rootA 'emu_add_github' @{ name = 'Lichii'; repo = 'a/b' }; Check 'duplicate refused' (-not $r.ok)
$r = App $rootA 'emu_add_github' @{ name = 'X'; repo = 'nope nope' }; Check 'bad repo refused' (-not $r.ok)

"-- bug report"
$logDir = "$rootA\Data\log"; New-Item -ItemType Directory -Force $logDir | Out-Null
$secret = @("C:\Users\$env:USERNAME\Documents\game.nup", "H:\Somewhere\$env:COMPUTERNAME", "$rootA\Roms\x", "C:\Users\OtherPerson\file", "mail me@example.com from 192.168.1.20", "header_key = AABBCC", "token=abc123", "build_id=BCF5E512E7B0DF65", "Booting game: 0400486024DEC000") -join "`n"
[IO.File]::WriteAllText("$logDir\eden_log.txt", $secret)
$r = App $rootA 'report_collect' @{ game = ''; log = $true }; Check 'report made' $r.ok $r.error
$txt = $r.value.text
Check 'no home path' ($txt -notmatch [regex]::Escape($env:USERPROFILE))
Check 'no user name in paths' ($txt -notmatch ('Users\\' + [regex]::Escape($env:USERNAME)))
Check 'no other user' ($txt -notmatch 'OtherPerson')
Check 'no computer name' ($txt -notmatch [regex]::Escape($env:COMPUTERNAME))
Check 'no email' ($txt -notmatch 'example\.com'); Check 'no ip' ($txt -notmatch '192\.168')
Check 'no key line' ($txt -notmatch 'AABBCC'); Check 'no token' ($txt -notmatch 'abc123')
Check 'no folder path' ($txt -notmatch [regex]::Escape($rootA))
Check 'useful data kept' (($txt -match 'BCF5E512E7B0DF65') -and ($txt -match '0400486024DEC000') -and ($txt -match 'Forkbay \d'))
Check 'report file saved' (Test-Path "$rootA\Manager\bug-report.txt")
$r = App $rootA 'report_collect' @{ game = ''; log = $false }; Check 'log can be left out' (($r.value.text -notmatch 'BCF5E512') )
"-- data move"
$r = App $rootA 'move_data' @{ location = 'custom'; custom = "$work\Moved\Data" }; Check 'move' $r.ok $r.error; "     $($r.value.message)"
Check 'moved save' ((Get-Content "$work\Moved\Data\nand\user\save\0\P\T1\a.bin") -eq 'B-3')
Check 'config rewritten' ((Get-Content "$work\Moved\Data\config\qt-config.ini" -Raw) -notmatch [regex]::Escape($rootA.Replace('\', '/') + '/Data'))
Check 'old copy kept' (Test-Path "$rootA\Data.old")

[IO.File]::WriteAllText("$rootA\Roms\Pokemon Pokopia [0400A3A01EE4C000].nup", 'x'); [IO.File]::WriteAllText("$rootA\Roms\Fire Emblem Fortunes Weave [04007EF01F8AE000].nup", 'x'); [IO.File]::WriteAllText("$rootA\Roms\The Legend of Zelda Ocarina of Time [0400486024DEC000][v131072].nup", 'x'); [IO.File]::WriteAllText("$work\bad.txt", 'x')
"-- journal"
$r = App $rootA 'install_firmware' @{ path = "$work\empty.zip" }
$logf = "$rootA\Manager\logs\forkbay.log"; Check 'error logged' ((Get-Content $logf -Raw) -match 'install_firmware')
Check 'journal has no path' ((Get-Content $logf -Raw) -notmatch [regex]::Escape($env:USERPROFILE))
$env:FORKBAY_SELFTEST = "$work\self.txt"; $null = App $rootA 'selftest_crash'; Remove-Item Env:\FORKBAY_SELFTEST
$j = Get-Content $logf -Raw; Check 'crash logged' ($j -match 'CRASH test crash'); Check 'crash path cleaned' ($j -notmatch [regex]::Escape($rootA))
$r = App $rootA 'report_collect' @{ game = ''; log = $false }; Check 'journal in report' ($r.value.text -match 'CRASH test crash')

"-- steam tool shortcut"
$vv = "$work\steam\userdata\2\config\shortcuts.vdf"; New-Item -ItemType Directory -Force (Split-Path $vv) | Out-Null
$r = App $rootA 'steam_save' @{ account = '2'; games = @('Test Game [0400000000000000].nup'); tool = $true }; Check 'tool saved' $r.ok $r.error
$r = App $rootA 'steam_load' @{ account = '2' }; Check 'tool found' $r.value.tool; Check 'game kept' (@($r.value.managed).Count -eq 1); Check 'tool not listed as game' (@($r.value.managed) -notcontains 'Forkbay')
$tx = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($vv)); Check 'tool entry name' ($tx -match 'AppName\x00Forkbay'); Check 'fullscreen flag' ($tx -match '--fullscreen'); $lc = "$work\steam\userdata\2\config\forkbay-launcher.exe"; Check 'no powershell in shortcuts' ($tx -notmatch 'powershell'); Check 'launcher next to shortcuts' (Test-Path $lc); Check 'quoted game path' ($tx -match '--launch "Roms/Test Game'); $lv = "$work\launcher-version.txt"; Start-Process -FilePath $lc -ArgumentList '--find', "`"$($Bin.Substring(3))`"", '--version', "`"$lv`"" -Wait; Check 'launcher finds the program' (Test-Path $lv)
$r = App $rootA 'steam_save' @{ account = '2'; games = @(); tool = $false }; $r = App $rootA 'steam_load' @{ account = '2' }; Check 'tool removed' (-not $r.value.tool)

"-- themes"
$good = "$work\good.json"; [IO.File]::WriteAllText($good, '{"forkbayTheme":1,"name":"Lava <b>","a":"#ff0000","b":"#ffaa00","gradient":false,"evil":"x"}')
$r = App $rootA 'theme_import' @{ path = $good }; Check 'theme imported' ($r.value.theme.name -eq 'Lava b') ($r | ConvertTo-Json -Compress); Check 'unknown field dropped' ($null -eq $r.value.theme.evil)
$r = App $rootA 'state'; Check 'theme active' ($r.value.theme -eq 'user:Lava b'); Check 'theme listed' (@($r.value.userThemes).Count -eq 1)
foreach ($bad in '{"forkbayTheme":1,"name":"X","a":"red;}body{","b":"#000000"}', '{"forkbayTheme":1,"name":"X","a":"#000000","b":"url(evil)"}', '{"name":"X","a":"#000000","b":"#000000"}', 'not json', '{"forkbayTheme":1,"name":"","a":"#000000","b":"#000000"}', '{"forkbayTheme":1,"name":"X","a":"#000000","b":"#000000","bg":"expression(1)","card":"#000000","text":"#000000"}') { $b = "$work\bad.json"; [IO.File]::WriteAllText($b, $bad); $r = App $rootA 'theme_import' @{ path = $b }; Check "bad theme refused" (-not $r.ok) $bad }
[IO.File]::WriteAllText("$work\big.json", ('x' * 20000)); $r = App $rootA 'theme_import' @{ path = "$work\big.json" }; Check 'huge file refused' (-not $r.ok)
$out = "$work\out.json"; $r = App $rootA 'theme_export' @{ path = $out; theme = @{ name = 'Sortie'; a = '#112233'; b = '#445566'; gradient = $true; bg = '#000000'; card = '#111111'; text = '#eeeeee' } }; Check 'theme exported' $r.ok $r.error; $j = Get-Content $out -Raw | ConvertFrom-Json; Check 'export content' (($j.a -eq '#112233') -and ($j.text -eq '#eeeeee') -and ($j.forkbayTheme -eq 1))
$r = App $rootA 'theme_import' @{ path = $out }; Check 'export can be imported' ($r.value.theme.name -eq 'Sortie')
$r = App $rootA 'theme_remove' @{ name = 'Sortie' }; $r = App $rootA 'state'; Check 'theme removed' (@($r.value.userThemes).Count -eq 1); Check 'falls back to preset' ($r.value.theme -eq 'aurora')
$cfg = "$rootA\Manager\settings.json"; $c = Get-Content $cfg -Raw | ConvertFrom-Json; $c.theme = 'light'; $c | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding UTF8; $r = App $rootA 'state'; Check 'old setting migrated' (($r.value.mode -eq 'light') -and ($r.value.theme -eq 'aurora')) ($r.value | Select-Object mode, theme | ConvertTo-Json -Compress)

[IO.File]::WriteAllText($cfg, 'not json at all'); $r = App $rootA 'state'; Check 'damaged settings kept aside' (Test-Path "$cfg.damaged".Replace('.json.damaged','.json.damaged')) ; Check 'damaged settings logged' ((Get-Content "$rootA\Manager\logs\forkbay.log" -Raw) -match 'settings.json unreadable')
[IO.File]::WriteAllText($cfg, '{"setupDone":true}')

"-- covers"
$r = App $rootA 'fetch_art'; Check 'no key, clear refusal' ((-not $r.ok) -and $r.error -match 'SteamGridDB') ($r | ConvertTo-Json -Compress)
New-Item -ItemType Directory -Force "$rootA\Covers" | Out-Null; [IO.File]::WriteAllBytes("$rootA\Covers\Test Game-grid.png", [byte[]](137, 80, 78, 71))
$r = App $rootA 'state'; Check 'own image shown' (@($r.value.games | Where-Object cover).Count -eq 1)

"-- languages"
New-Item -ItemType Directory -Force "$rootA\Manager\locales" | Out-Null
[IO.File]::WriteAllText("$rootA\Manager\locales\de.json", '{"name":"Deutsch","ui":{"Jeux":"Spiele"},"uiPatterns":[],"messages":[["^Choisis ton fichier prod\\.keys\\.$","Wahle deine prod.keys."]],"categories":{}}')
function Lang([string]$code, [string]$cmd, $arg = @{}) { $env:FORKBAY_ROOT = $rootA; $env:FORKBAY_LANG = $code; $file = "$work\l.json"; if (Test-Path $file) { Remove-Item $file }; $json = ($arg | ConvertTo-Json -Compress).Replace('"', '\"'); Start-Process -FilePath $Bin -ArgumentList '--cli', $cmd, "`"$json`"", "`"$file`"" -Wait -NoNewWindow; Get-Content -LiteralPath $file -Raw -Encoding UTF8 | ConvertFrom-Json }
$r = Lang 'de-DE' 'state'; Check 'dropped file detected' ($r.value.language -eq 'de'); Check 'listed' (@($r.value.languages | ForEach-Object code) -contains 'de')
$r = Lang 'de-DE' 'install_keys' @{ path = "$work\bad.txt" }; Check 'translated message' ($r.error -match 'Wahle') ($r | ConvertTo-Json -Compress)
$r = Lang 'de-DE' 'install_firmware' @{ path = "$work\empty.zip" }; Check 'missing text falls back to English' ($r.error -match 'not a firmware') ($r | ConvertTo-Json -Compress)
$r = Lang 'ja-JP' 'state'; Check 'unknown language gives English' ($r.value.language -eq 'en')
$r = Lang 'fr-CA' 'state'; Check 'french detected' ($r.value.language -eq 'fr')
$r = Lang 'en-US' 'locale'; Check 'page locale' ($r.value.tables[0].ui.'Jeux' -eq 'Games')
$env:FORKBAY_LANG = 'fr'
"-- wichforks"
$r = App $rootA 'compat'; Check 'guide read' $r.ok $r.error
if ($r.ok) { "     maj $($r.value.updated), carte [$($r.value.gpu)], jeux $(@($r.value.games.PSObject.Properties).Count), catalogue $(@($r.value.catalogue).Count)"; $r.value.games.PSObject.Properties | ForEach-Object { $g = $_.Value; "     $($_.Name) => $($g.guideName): " + (($g.first | ForEach-Object name) -join ', ') + " | rapports $($g.reports)/$($g.reportsSameGpu)" }; $p = $r.value.games.'Pokemon Pokopia [0400A3A01EE4C000].nup'; Check 'pokopia found' ($p.first.Count -ge 1); $z = $r.value.games.'The Legend of Zelda Ocarina of Time [0400486024DEC000][v131072].nup'; Check 'zelda found' ($z.first.Count -ge 1); $f = $r.value.games.'Fire Emblem Fortunes Weave [04007EF01F8AE000].nup'; Check 'fire emblem found' ($f.first.Count -ge 1) }
"-- automatic copies and older versions"
$rootC = "$work\C\Files"; $sc = "$rootC\Data\nand\user\save\0\P\T1\a.bin"; New-Item -ItemType Directory -Force (Split-Path $sc), "$rootC\Roms", "$work\fake" | Out-Null; [IO.File]::WriteAllText($sc, 'one')
$r = App $rootC 'state'; $r = App $rootC 'backup_now'; Check 'copy made' ($r.value.message -match 'enregistr') ($r | ConvertTo-Json -Compress)
$r = App $rootC 'backup_now'; Check 'unchanged data not copied again' ($r.value.message -match 'Rien')
Copy-Item "$env:WINDIR\System32\hostname.exe" "$work\fake\fake.exe"; $r = App $rootC 'emu_add_local' @{ name = 'Fake'; exe = "$work\fake\fake.exe" }; Check 'test emulator added' $r.ok $r.error; $r = App $rootC 'set_default_emu' @{ emu = 'Fake' }; [IO.File]::WriteAllText("$rootC\Roms\Game [0400000000000000].nup", 'x'); $r = App $rootC 'set_game_emu' @{ file = 'Game [0400000000000000].nup'; emu = 'Fake' }
function PlayC { $env:FORKBAY_ROOT = $rootC; Start-Process -FilePath $Bin -ArgumentList '--launch', '"Roms/Game [0400000000000000].nup"' -Wait }
[IO.File]::WriteAllText($sc, 'two'); PlayC; $r = App $rootC 'backup_list'; Check 'copy before playing' (@($r.value.copies).Count -eq 2) ($r | ConvertTo-Json -Compress -Depth 4)
PlayC; $r = App $rootC 'backup_list'; Check 'no copy when nothing changed' (@($r.value.copies).Count -eq 2)
[IO.File]::WriteAllText($sc, 'three'); $first = @($r.value.copies)[-1].name; $r = App $rootC 'backup_restore' @{ name = $first }; Check 'restore' $r.ok $r.error; Check 'restored content' ((Get-Content $sc) -eq 'one')
$r = App $rootC 'backup_list'; $n = @($r.value.copies)[0].name; Check 'state kept before restore' ((Get-Content "$rootC\Save Backups\$n\nand\user\save\0\P\T1\a.bin") -eq 'three')
$r = App $rootC 'backup_restore' @{ name = '..\..\Data' }; Check 'bad copy name refused' (-not $r.ok)
$r = App $rootC 'backup_set' @{ enabled = $false }; [IO.File]::WriteAllText($sc, 'four'); $before = @((App $rootC 'backup_list').value.copies).Count; PlayC; Check 'option can be turned off' (@((App $rootC 'backup_list').value.copies).Count -eq $before)
$of = "$rootC\Old Versions\Fake v1"; New-Item -ItemType Directory -Force $of, "$rootC\Old Versions\Lichii Upstream v9" | Out-Null; foreach ($d in $of, "$rootC\Old Versions\Lichii Upstream v9") { Copy-Item "$work\fake\fake.exe" "$d\fake.exe"; [IO.File]::WriteAllText("$d\exe.txt", 'fake.exe'); [IO.File]::WriteAllText("$d\version.txt", 'v1') }; [IO.File]::WriteAllText("$rootC\Fake\version.txt", 'v2')
$r = App $rootC 'emu_status'; $fk = @($r.value) | Where-Object name -eq 'Fake'; Check 'older version listed' (@($fk.old) -contains 'Fake v1'); $li = @($r.value) | Where-Object name -eq 'Lichii'; Check 'other emulator not mixed in' (@($li.old).Count -eq 0)
$r = App $rootC 'emu_rollback' @{ name = 'Fake'; version = '..\..\Data' }; Check 'bad version refused' (-not $r.ok)
$r = App $rootC 'emu_rollback' @{ name = 'Fake'; version = 'Fake v1' }; Check 'rollback' $r.ok $r.error; Check 'older version active' ((Get-Content "$rootC\Fake\version.txt") -eq 'v1'); Check 'newer version kept' (Test-Path "$rootC\Old Versions\Fake v2\fake.exe"); Check 'data link back' ((Get-Item "$rootC\Fake\user" -Force).Attributes -match 'ReparsePoint'); Check 'no link in the kept version' (-not (Test-Path "$rootC\Old Versions\Fake v2\user"))
"-- window protection"
$env:FORKBAY_ROOT = $rootC; $env:FORKBAY_SELFTEST = "$work\sec.txt"; $env:FORKBAY_SECTEST = '1'; $w = Start-Process -FilePath $Bin -PassThru; $sec = ''; for ($i = 0; $i -lt 40 -and $sec -notmatch 'nav=blocked'; $i++) { Start-Sleep -Milliseconds 750; if (Test-Path "$work\sec.txt") { $sec = Get-Content "$work\sec.txt" -Raw } }; Stop-Process -Id $w.Id -Force -ErrorAction SilentlyContinue; Remove-Item Env:\FORKBAY_SELFTEST, Env:\FORKBAY_SECTEST; Check 'page cannot reach or navigate to the web' ($sec -match 'csp=blocked nav=blocked') $sec
Remove-Item Env:\FORKBAY_ROOT, Env:\FORKBAY_SEARCH, Env:\FORKBAY_STEAM -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($script:fail) { 'RESULT: FAILED' } else { 'RESULT: all passed' }
exit $script:fail
