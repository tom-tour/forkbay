# Runs the core commands of a Windows build on throwaway folders.
param([string]$Bin)
# Some checks expect a command to fail: failures are counted, they do not stop the script.
$ErrorActionPreference = 'Continue'
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
New-Item -ItemType Directory -Force "$rootA\Manager" | Out-Null; [IO.File]::WriteAllText("$rootA\Manager\playtime.json", '{"0400000000000000":{"seconds":3700,"last":1700000000}}')
$r = App $rootA 'state'; Check 'play time read by program id' ($r.value.games[0].played -eq 3700 -and $r.value.games[0].last -eq 1700000000) ($r.value.games | ConvertTo-Json -Compress)
Remove-Item "$rootA\Manager\playtime.json"
$r = App $rootA 'game_reports' @{ file = 'Test Game [0400000000000000].nup' }; Check 'report form filled in' ($r.ok -and $r.value.form.StartsWith('https://github.com/tom-tour/forkbay-compat/issues/new?template=report.yml&') -and $r.value.form.Contains('title_id=0400000000000000') -and $r.value.site.Contains('#/find/0400000000000000/')) ($r | ConvertTo-Json -Compress -Depth 4)
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
foreach ($n in 'eglantine', 'eden') { New-Item -ItemType Directory -Force "$work\other\$n\cache\shader\0400" | Out-Null; [IO.File]::WriteAllText("$work\other\$n\cache\shader\0400\vulkan.bin", $n) }
$r = App $rootA 'inspect' @{ path = "$work\other\eden" }; Check 'shaders of an unknown emulator not offered' (-not (@($r.value.categories | ForEach-Object id) -contains 'shaders')) ($r | ConvertTo-Json -Compress -Depth 4)
$r = App $rootA 'inspect' @{ path = "$work\other\eglantine" }; Check 'shaders of a known emulator offered' (@($r.value.categories | ForEach-Object id) -contains 'shaders')
$r = App $rootA 'import' @{ path = "$work\other\eglantine"; cats = @('shaders') }; Check 'shaders go to their own emulator' ($r.ok -and (Test-Path "$rootA\Data\cache\shaders\Eglantine\0400\vulkan.bin") -and -not (Test-Path "$rootA\Data\cache\shader\0400")) $r.error
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
$tx = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($vv)); Check 'tool entry name' ($tx -match 'AppName\x00Forkbay'); Check 'Steam mode flag' ($tx -match '--steam-ui'); $lc = "$work\steam\userdata\2\config\forkbay-launcher.exe"; Check 'no powershell in shortcuts' ($tx -notmatch 'powershell'); Check 'launcher next to shortcuts' (Test-Path $lc); Check 'quoted game path' ($tx -match '--launch "Roms/Test Game'); $lv = "$work\launcher-version.txt"; Start-Process -FilePath $lc -ArgumentList '--find', "`"$($Bin.Substring(3))`"", '--version', "`"$lv`"" -Wait; Check 'launcher finds the program' (Test-Path $lv)
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
$sh = Get-Item -Force "$rootC\Data\cache\shader" -ErrorAction SilentlyContinue; Check 'shader cache of this emulator only' ($sh -and $sh.LinkType -eq 'Junction' -and "$($sh.Target)" -like '*cache\shaders\Fake') "$($sh.LinkType) $($sh.Target)"
[IO.File]::WriteAllText("$rootC\Data\cache\shader\vulkan.bin", 'x'); $r = App $rootC 'emu_status'; Check 'shader cache size shown' ((@($r.value) | Where-Object name -eq 'Fake').shaders -gt 0)
$r = App $rootC 'emu_clear_shaders' @{ name = 'Fake' }; Check 'shader cache cleared' ($r.ok -and -not (Test-Path "$rootC\Data\cache\shader\vulkan.bin") -and (Test-Path "$rootC\Data\cache\shaders\Fake")) $r.error
$r = App $rootC 'emu_clear_shaders' @{ name = '..\..' }; Check 'unknown emulator refused for clearing' (-not $r.ok)
PlayC; $r = App $rootC 'backup_list'; Check 'no copy when nothing changed' (@($r.value.copies).Count -eq 2)
[IO.File]::WriteAllText($sc, 'three'); $first = @($r.value.copies)[-1].name; $r = App $rootC 'backup_restore' @{ name = $first }; Check 'restore' $r.ok $r.error; Check 'restored content' ((Get-Content $sc) -eq 'one')
$r = App $rootC 'backup_list'; $n = @($r.value.copies)[0].name; Check 'state kept before restore' ((Get-Content "$rootC\Save Backups\$n\nand\user\save\0\P\T1\a.bin") -eq 'three')
$cp = @($r.value.copies)[0]; $expected = [DateTimeOffset]::FromUnixTimeSeconds([long]($cp.name -replace '^\D+', '')).ToOffset([TimeZoneInfo]::Local.GetUtcOffset([datetime]::Now)).ToString('HH:mm'); Check 'dates use the local time' ($cp.date -match $expected) "$($cp.date) / $expected"
$r2 = App $rootC 'report_collect' @{ game = ''; log = $false }; Check 'system details without helper programs' (($r2.value.text -match 'Windows') -and ($r2.value.text -match '\d+ GB')) $r2.value.text.Substring(0, 300)
$r = App $rootC 'backup_restore' @{ name = '..\..\Data' }; Check 'bad copy name refused' (-not $r.ok)
$r = App $rootC 'backup_set' @{ enabled = $false }; [IO.File]::WriteAllText($sc, 'four'); $before = @((App $rootC 'backup_list').value.copies).Count; PlayC; Check 'option can be turned off' (@((App $rootC 'backup_list').value.copies).Count -eq $before)
$of = "$rootC\Old Versions\Fake v1"; New-Item -ItemType Directory -Force $of, "$rootC\Old Versions\Lichii Upstream v9" | Out-Null; foreach ($d in $of, "$rootC\Old Versions\Lichii Upstream v9") { Copy-Item "$work\fake\fake.exe" "$d\fake.exe"; [IO.File]::WriteAllText("$d\exe.txt", 'fake.exe'); [IO.File]::WriteAllText("$d\version.txt", 'v1') }; [IO.File]::WriteAllText("$rootC\Fake\version.txt", 'v2')
$r = App $rootC 'emu_status'; $fk = @($r.value) | Where-Object name -eq 'Fake'; Check 'older version listed' (@($fk.old) -contains 'Fake v1'); $li = @($r.value) | Where-Object name -eq 'Lichii Upstream'; Check 'other emulator not mixed in' ((@($li.old) -contains 'Lichii Upstream v9') -and -not (@($fk.old) -contains 'Lichii Upstream v9'))
$r = App $rootC 'emu_versions' @{ name = 'Fake' }; $v = @($r.value.versions) | Where-Object old -eq 'Fake v1'; Check 'kept version offered in the version list' ($r.ok -and $v -and $v.tag -eq 'v1') $r.error
$r = App $rootC 'emu_versions' @{ name = 'Nope' }; Check 'version list of an unknown emulator refused' (-not $r.ok)
$r = App $rootC 'emu_rollback' @{ name = 'Fake'; version = '..\..\Data' }; Check 'bad version refused' (-not $r.ok)
$r = App $rootC 'emu_rollback' @{ name = 'Fake'; version = 'Fake v1' }; Check 'rollback' $r.ok $r.error; Check 'older version active' ((Get-Content "$rootC\Fake\version.txt") -eq 'v1'); Check 'newer version kept' (Test-Path "$rootC\Old Versions\Fake v2\fake.exe"); Check 'data link back' ((Get-Item "$rootC\Fake\user" -Force).Attributes -match 'ReparsePoint'); Check 'no link in the kept version' (-not (Test-Path "$rootC\Old Versions\Fake v2\user"))
"-- window protection"
$env:FORKBAY_ROOT = $rootC; $env:FORKBAY_SELFTEST = "$work\sec.txt"; $env:FORKBAY_SECTEST = '1'; $w = Start-Process -FilePath $Bin -PassThru; $sec = ''; for ($i = 0; $i -lt 40 -and $sec -notmatch 'nav=blocked'; $i++) { Start-Sleep -Milliseconds 750; if (Test-Path "$work\sec.txt") { $sec = Get-Content "$work\sec.txt" -Raw } }; Stop-Process -Id $w.Id -Force -ErrorAction SilentlyContinue; Remove-Item Env:\FORKBAY_SELFTEST, Env:\FORKBAY_SECTEST; Check 'page cannot reach or navigate to the web' ($sec -match 'csp=blocked nav=blocked') $sec
"-- full screen, game folders and images"
$rootF = "$work\F\Files"; New-Item -ItemType Directory -Force "$rootF\Roms", "$rootF\Fritchii", "$work\games\Sub Folder" | Out-Null
[IO.File]::WriteAllText("$work\games\Sub Folder\Other Game [0400000000000001].nsp", 'x'); [IO.File]::WriteAllText("$work\games\ignored.txt", 'x')
[IO.File]::WriteAllText("$rootF\Fritchii\args.bat", "@echo %* > ""$work\args.txt""`r`n"); [IO.File]::WriteAllText("$rootF\Fritchii\exe.txt", 'args.bat')
$r = App $rootF 'state'; $r = App $rootF 'game_dir_add' @{ path = "$work\games" }; Check 'games folder added' ($r.value.message -match '^Dossier ajout.* : 1 ') ($r | ConvertTo-Json -Compress)
$r = App $rootF 'game_dir_add' @{ path = "$work\games" }; Check 'same folder refused' (-not $r.ok)
$r = App $rootF 'game_dir_add' @{ path = "$rootF\Roms" }; Check 'Roms folder refused' (-not $r.ok)
$r = App $rootF 'state'; $ext = @($r.value.games)[0]; Check 'game found in subfolder, not moved' ($ext.name -eq 'Other Game' -and (Test-Path -LiteralPath "$work\games\Sub Folder\Other Game [0400000000000001].nsp") -and @(Get-ChildItem "$rootF\Roms").Count -eq 0) ($r | ConvertTo-Json -Compress -Depth 3)
$r = App $rootF 'set_game_emu' @{ file = $ext.file; emu = 'Fritchii' }
function PlayF { if (Test-Path "$work\args.txt") { Remove-Item "$work\args.txt" }; $env:FORKBAY_ROOT = $rootF; Start-Process -FilePath $Bin -ArgumentList '--launch', ('"' + $ext.file + '"') -Wait; Start-Sleep -Milliseconds 300; if (Test-Path "$work\args.txt") { Get-Content "$work\args.txt" -Raw } else { '' } }
$a = PlayF; Check 'game launched in full screen' ($a -match '^-f -g ' -and $a -match 'Other Game') $a
App $rootF 'set_fullscreen_games' @{ enabled = $false } | Out-Null; $a = PlayF; Check 'full screen can be turned off' ($a -match '^-g ' -and $a -notmatch '-f ') $a; App $rootF 'set_fullscreen_games' @{ enabled = $true } | Out-Null
$vdf = "$work\steam\userdata\1\config\shortcuts.vdf"; if (Test-Path $vdf) { Remove-Item $vdf }
$r = App $rootF 'steam_save' @{ account = '1'; games = @($ext.file); tool = $true }; Check 'steam shortcut for a game elsewhere' $r.ok $r.error
$raw = [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($vdf)); Check 'steam launches it by full path' ($raw.Contains('--launch "' + $ext.file + '"')); Check 'tool shortcut follows the Steam mode' ($raw.Contains('--steam-ui') -and -not $raw.Contains('--fullscreen'))
$r = App $rootF 'steam_load' @{ account = '1' }; Check 'steam page finds them again' ((@($r.value.managed) -contains $ext.file) -and $r.value.tool -and $r.value.stale -eq 0) ($r | ConvertTo-Json -Compress)
$png = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAEBgIApD5fRAAAAABJRU5ErkJggg=='); [IO.File]::WriteAllBytes("$work\any name.png", $png); [IO.File]::WriteAllBytes("$work\fake.jpg", [byte[]](0xFF,0xD8,0xFF,0xE0,0,0,0)); [IO.File]::WriteAllText("$work\not-image.png", 'hello')
$r = App $rootF 'set_image' @{ file = $ext.file; kind = 'grid'; path = "$work\any name.png" }; Check 'image named automatically' ($r.ok -and (Test-Path -LiteralPath "$rootF\Covers\Other Game-grid.png")) $r.error
$r = App $rootF 'state'; Check 'cover shown' ([bool]@($r.value.games)[0].cover)
$r = App $rootF 'set_image' @{ file = $ext.file; kind = 'grid'; path = "$work\fake.jpg" }; Check 'new image replaces the old one' ($r.ok -and (Test-Path -LiteralPath "$rootF\Covers\Other Game-grid.jpg") -and -not (Test-Path -LiteralPath "$rootF\Covers\Other Game-grid.png")) $r.error
$r = App $rootF 'set_image' @{ file = $ext.file; kind = 'grid'; path = "$work\not-image.png" }; Check 'file that is not an image refused' (-not $r.ok)
$r = App $rootF 'set_image' @{ file = '..\..\x'; kind = 'grid'; path = "$work\any name.png" }; Check 'unknown game refused' (-not $r.ok)
$r = App $rootF 'set_image' @{ file = $ext.file; kind = '..\x'; path = "$work\any name.png" }; Check 'unknown image type refused' (-not $r.ok)
$sd = "$rootF\Manager\cover-sources"; New-Item -ItemType Directory -Force $sd | Out-Null; [IO.File]::WriteAllText("$sd\bad.json", '{ nope'); [IO.File]::WriteAllText("$sd\plain.json", '{"name":"Plain","direct":{"wide":"http://example.com/{id}.png","exe":"https://example.com/a.exe"}}')
$r = App $rootF 'state'; Check 'source listed, bad one ignored' (@($r.value.coverSources) -join ',' -eq 'Plain') ($r.value.coverSources -join ',')
$r = App $rootF 'fetch_art'; Check 'sources work without a key, insecure address refused' ($r.ok -and $r.value.message -match 'bad' -and -not (Test-Path -LiteralPath "$rootF\Covers\Other Game-wide.png")) ($r | ConvertTo-Json -Compress)
$r = App $rootF 'game_dir_remove' @{ path = "$work\games" }; $r = App $rootF 'state'; Check 'folder removed, game untouched' (@($r.value.games).Count -eq 0 -and (Test-Path -LiteralPath "$work\games\Sub Folder\Other Game [0400000000000001].nsp"))
$env:SteamGamepadUI = '1'; $r = App $rootF 'steam_ui_detect'; Check 'Steam full-screen mode detected' $r.value.active; Remove-Item Env:\SteamGamepadUI
App $rootF 'game_dir_add' @{ path = "$work\games" } | Out-Null; App $rootF 'finish_setup' | Out-Null
$env:FORKBAY_ROOT = $rootF; $env:FORKBAY_SELFTEST = "$work\ui.txt"; $env:FORKBAY_UITEST = "$work\any name.png"; Remove-Item -LiteralPath "$rootF\Covers\Other Game-grid.jpg" -ErrorAction SilentlyContinue; $w = Start-Process -FilePath $Bin -PassThru; $ui = ''; for ($i = 0; $i -lt 60 -and $ui -notmatch 'drop='; $i++) { Start-Sleep -Milliseconds 500; if (Test-Path "$work\ui.txt") { $ui = Get-Content "$work\ui.txt" -Raw } }; if (-not $w.HasExited) { Stop-Process -Id $w.Id -Force }; Remove-Item Env:\FORKBAY_SELFTEST, Env:\FORKBAY_UITEST
Check 'F11 and dropping an image in the window' ($ui -match 'on=true off=false drop=ok') $ui
"-- emulator settings, running game and image choices"
$rootG = "$work\G\Files"; New-Item -ItemType Directory -Force "$rootG\Roms", "$rootG\Data\config", "$rootG\Fritchii" | Out-Null
[IO.File]::WriteAllText("$rootG\Roms\Run Game [0400000000000002].nup", 'x'); Copy-Item $Bin "$rootG\Fritchii\emu.exe"; [IO.File]::WriteAllText("$rootG\Fritchii\exe.txt", 'emu.exe')
$ini = "$rootG\Data\config\qt-config.ini"; [IO.File]::WriteAllText($ini, "[Core]`r`nuse_speed_limit\default=true`r`nuse_speed_limit=true`r`nspeed_limit\default=true`r`nspeed_limit=100`r`n[System]`r`nlanguage_index\default=true`r`nlanguage_index=1`r`nregion_index=1`r`n[UI]`r`nother=keep`r`n")
$r = App $rootG 'state'; $r = App $rootG 'emu_settings_get'; Check 'settings read' ($r.value.language_index -eq 1 -and $r.value.speed -eq 100 -and $r.value.volume -eq 100) ($r | ConvertTo-Json -Compress)
$r = App $rootG 'emu_settings_set' @{ key = 'language_index'; value = 2 }; $t = [IO.File]::ReadAllText($ini); Check 'game language written for the emulators' ($r.ok -and $t.Contains("language_index\default=false`r`nlanguage_index=2`r`n")) $t
Check 'other settings untouched' ($t.Contains("region_index=1`r`n") -and $t.Contains("other=keep`r`n")); Check 'copy kept before writing' ([IO.File]::ReadAllText("$ini.forkbay-backup".Replace('.ini.forkbay-backup', '.ini.forkbay-backup')).Contains('language_index=1'))
$r = App $rootG 'emu_settings_set' @{ key = 'resolution_setup'; value = 6 }; $t = [IO.File]::ReadAllText($ini); Check 'missing section created' ($r.ok -and $t.Contains("[Renderer]") -and $t.Contains("resolution_setup=6"))
$r = App $rootG 'emu_settings_get'; Check 'value read back' ($r.value.resolution_setup -eq 6 -and $r.value.language_index -eq 2)
$r = App $rootG 'emu_settings_set' @{ key = 'speed'; value = 0 }; $t = [IO.File]::ReadAllText($ini); Check 'no speed limit' ($t.Contains('use_speed_limit=false') -and (App $rootG 'emu_settings_get').value.speed -eq 0)
$r = App $rootG 'emu_settings_set' @{ key = 'speed'; value = 150 }; $t = [IO.File]::ReadAllText($ini); Check 'speed limit' ($t.Contains('use_speed_limit=true') -and $t.Contains('speed_limit=150'))
$r = App $rootG 'emu_settings_set' @{ key = 'language_index'; value = 99 }; Check 'value out of range refused' (-not $r.ok)
$r = App $rootG 'emu_settings_set' @{ key = 'backend'; value = 2 }; Check 'renderer that draws nothing refused' (-not $r.ok)
$r = App $rootG 'emu_settings_set' @{ key = 'nand_directory'; value = 1 }; Check 'unknown setting refused' (-not $r.ok)
function StartG { $env:FORKBAY_ROOT = $rootG; $p = Start-Process -FilePath $Bin -ArgumentList '--launch', '"Roms/Run Game [0400000000000002].nup"' -PassThru; for ($i = 0; $i -lt 60; $i++) { Start-Sleep -Milliseconds 500; $script:s = App $rootG 'game_session'; if ($script:s.value.session.file) { break } }; return $p }
function StopWait($p) { for ($i = 0; $i -lt 40 -and -not $p.HasExited; $i++) { Start-Sleep -Milliseconds 500 }; return $p.HasExited }
"-- controllers"
$idA = '030000005e0400008e02000010017801'
[IO.File]::AppendAllText($ini, "[Controls]`r`nplayer_0_button_a\default=false`r`nplayer_0_button_a=`"engine:sdl,pad:0,button:0,guid:$idA,port:0`"`r`nplayer_0_button_b\default=true`r`nplayer_0_button_b=x`r`n")
$r = App $rootG 'controls_devices'; Check 'controllers read through SDL' ($r.ok -and $null -ne $r.value) $r.error
$r = App $rootG 'controls_get'; $p0 = $r.value.players[0]; Check 'controls read like the emulators' ($r.ok -and @($r.value.players).Count -eq 10 -and $p0.controls.buttons.button_a -eq "engine:sdl,pad:0,button:0,guid:$idA,port:0" -and $p0.controls.buttons.button_b -eq 'engine:keyboard,code:88,toggle:0' -and $p0.connected -and -not $r.value.players[1].connected) ($r | ConvertTo-Json -Compress -Depth 6)
$r = App $rootG 'controls_set' @{ player = 1; name = 'button_a'; value = 'engine:keyboard,code:75,toggle:0' }; $t = [IO.File]::ReadAllText($ini); Check 'control written like the emulators' ($r.ok -and $t.Contains("player_1_button_a\default=false`r`nplayer_1_button_a=`"engine:keyboard,code:75,toggle:0`"`r`n")) $t
$r = App $rootG 'controls_set' @{ player = 1; name = 'nand_directory'; value = 'x' }; Check 'unknown control refused' (-not $r.ok)
$r = App $rootG 'controls_connected' @{ player = 3; on = $true }; $r = App $rootG 'controls_get'; Check 'players before it connected too' ((@($r.value.players[0..3] | Where-Object { $_.connected }).Count -eq 4) -and -not $r.value.players[4].connected)
$r = App $rootG 'controls_connected' @{ player = 2; on = $false }; $r = App $rootG 'controls_get'; Check 'players after it disconnected too' ($r.value.players[1].connected -and -not $r.value.players[2].connected -and -not $r.value.players[3].connected)
$r = App $rootG 'controls_type' @{ player = 0; type = 4 }; $r = App $rootG 'controls_get'; Check 'handheld moves player 1 to the handheld controller' ($r.value.players[0].type -eq 4 -and $r.value.players[8].type -eq 4 -and $r.value.players[8].connected -and -not $r.value.players[0].connected -and -not $r.value.docked)
$r = App $rootG 'controls_type' @{ player = 1; type = 4 }; Check 'handheld only for player 1' (-not $r.ok)
$r = App $rootG 'controls_type' @{ player = 0; type = 0 }; $r = App $rootG 'controls_get'; Check 'back to a controller' ($r.value.players[0].connected -and -not $r.value.players[8].connected -and $r.value.players[0].type -eq 0)
$r = App $rootG 'profile_save' @{ name = 'Salon'; player = 0; new = $true }; $pf = "$rootG\Data\config\input\Salon.ini"; Check 'profile saved like the emulators' ($r.ok -and (Test-Path $pf) -and ([IO.File]::ReadAllText($pf)).Contains("button_a=`"engine:sdl,pad:0,button:0,guid:$idA,port:0`"")) $r.error
$r = App $rootG 'profile_save' @{ name = 'Salon'; player = 0; new = $true }; Check 'profile name taken refused' (-not $r.ok)
$r = App $rootG 'profile_save' @{ name = 'a/b'; player = 0; new = $true }; Check 'profile name the emulators refuse refused' (-not $r.ok)
$r = App $rootG 'profile_load' @{ name = 'Salon'; player = 2 }; $r = App $rootG 'controls_get'; Check 'profile loaded into a player' ($r.value.players[2].controls.buttons.button_a -like "*guid:$idA*" -and @($r.value.profiles) -contains 'Salon')
$gc = "$rootG\Data\config\custom\0400000000000002.ini"
$r = App $rootG 'game_profile_set' @{ file = 'Run Game [0400000000000002].nup'; player = 0; name = 'Salon' }; $t = [IO.File]::ReadAllText($gc); Check 'game gets its own profile' ($r.ok -and $t.Contains('player_0_profile_name=Salon') -and $t.Contains("guid:$idA") -and $t.Contains('player_0_connected=true')) $t
$r = App $rootG 'game_profiles' @{ file = 'Run Game [0400000000000002].nup' }; Check 'game profile read back' ($r.value.players[0] -eq 'Salon' -and $r.value.players[1] -eq '')
$r = App $rootG 'game_profile_set' @{ file = 'Run Game [0400000000000002].nup'; player = 0; name = '' }; $t = [IO.File]::ReadAllText($gc); Check 'game follows the common controls again' ($r.ok -and -not $t.Contains('player_0_')) $t
$r = App $rootG 'profile_delete' @{ name = 'Salon' }; Check 'profile deleted' ($r.ok -and -not (Test-Path $pf))
"-- shortcuts"
$r = App $rootG 'hotkeys_get'; $h = @($r.value.list) | Where-Object { $_.id -eq 'Change Docked Mode' }; Check 'shortcuts read with the emulators defaults' ($r.ok -and $h.key -eq 'F10' -and $h.pad -eq 'Home+X') ($r | ConvertTo-Json -Compress -Depth 4)
$r = App $rootG 'hotkeys_set' @{ id = 'Change Docked Mode'; field = 'pad'; value = 'Home+ZR' }; $t = [IO.File]::ReadAllText($ini); Check 'shortcut written like the emulators' ($r.ok -and $t.Contains("Shortcuts\Main%20Window\Change%20Docked%20Mode\Controller_KeySeq=Home+ZR") -and $t.Contains("Shortcuts\Main%20Window\Change%20Docked%20Mode\Controller_KeySeq\default=false")) $r.error
$r = App $rootG 'hotkeys_set' @{ id = 'Exit Eglantine'; field = 'key'; value = 'Ctrl+=' }; $t = [IO.File]::ReadAllText($ini); Check 'quit written under every emulator name, quoted' ($r.ok -and $t.Contains("Shortcuts\Main%20Window\Exit%20Eglantine\KeySeq=`"Ctrl+=`"") -and $t.Contains("Shortcuts\Main%20Window\Exit%20Orchid\KeySeq=`"Ctrl+=`"")) $r.error
$r = App $rootG 'hotkeys_set' @{ id = 'Fullscreen'; field = 'pad'; value = 'ZR+Home' }; Check 'shortcut already used refused' (-not $r.ok)
$r = App $rootG 'hotkeys_set' @{ id = 'Fullscreen'; field = 'pad'; value = 'Home+Start' }; Check 'unknown button refused' (-not $r.ok)
$r = App $rootG 'hotkeys_reset'; $r = App $rootG 'hotkeys_get'; $h = @($r.value.list) | Where-Object { $_.id -eq 'Change Docked Mode' }; $t = [IO.File]::ReadAllText($ini); Check 'shortcuts back to default' ($r.ok -and $h.pad -eq 'Home+X' -and $t.Contains("Shortcuts\Main%20Window\Change%20Docked%20Mode\Controller_KeySeq\default=true"))
$p = StartG; Check 'running game known' ($s.value.session.file -like 'Run Game*' -and $s.value.session.emu -eq 'Fritchii') ($s | ConvertTo-Json -Compress -Depth 3)
$t = Get-Content $ini -Raw; Check 'emulator update prompt turned off' ($t -match '(?m)^check_for_updates=false' -and $t -match '(?m)^other=keep') $t
$r = App $rootG 'emu_settings_set' @{ key = 'language_index'; value = 3 }; Check 'settings refused while an emulator is open' (-not $r.ok)
$r = App $rootG 'controls_set' @{ player = 0; name = 'button_a'; value = 'engine:keyboard,code:75,toggle:0' }; Check 'controls refused while an emulator is open' (-not $r.ok)
$r = App $rootG 'hotkeys_set' @{ id = 'Fullscreen'; field = 'key'; value = 'F12' }; Check 'shortcuts refused while an emulator is open' (-not $r.ok)
$r = App $rootG 'game_close' @{ force = $false }; Check 'close requested' $r.ok $r.error; Check 'game closed normally' (StopWait $p); $s = App $rootG 'game_session'; Check 'session ended' (-not $s.value.session)
$p = StartG; $r = App $rootG 'game_close' @{ force = $true }; Check 'forced close' ($r.ok -and (StopWait $p)) $r.error
$r = App $rootG 'game_close' @{ force = $true }; Check 'nothing to close' (-not $r.ok)
[IO.File]::WriteAllText("$rootG\Manager\session.json", '{"file":"x","emu":"Fritchii","exe":"emu.exe","pid":4000000}'); $s = App $rootG 'game_session'; Check 'stale session ignored' ((-not $s.value.session) -and -not (Test-Path "$rootG\Manager\session.json"))
[IO.File]::WriteAllText("$rootG\Manager\session.json", ('{"file":"x","emu":"Fritchii","exe":"emu.exe","pid":' + $PID + '}')); $s = App $rootG 'game_session'; Check 'another program with that number is not a game' (-not $s.value.session)
$g = 'Run Game [0400000000000002].nup'; $png = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAEBgIApD5fRAAAAABJRU5ErkJggg=='); [IO.File]::WriteAllBytes("$work\tile.png", $png)
foreach ($k in 'grid', 'logo') { $r = App $rootG 'set_image' @{ file = $g; kind = $k; path = "$work\tile.png" } }
$r = App $rootG 'game_images' @{ file = $g }; Check 'all images visible' ($r.value.grid -like 'data:image/png*' -and $r.value.logo -like 'data:image/png*' -and -not $r.value.hero) ($r | ConvertTo-Json -Compress)
$r = App $rootG 'remove_image' @{ file = $g; kind = 'logo' }; $r2 = App $rootG 'game_images' @{ file = $g }; Check 'image removed' ($r.ok -and -not $r2.value.logo -and $r2.value.grid)
$r = App $rootG 'remove_image' @{ file = '..\..\x'; kind = 'grid' }; Check 'unknown game refused for removal' (-not $r.ok)
$r = App $rootG 'fetch_art' @{ kinds = @() }; Check 'no image type refused' (-not $r.ok)
$sd = "$rootG\Manager\cover-sources"; New-Item -ItemType Directory -Force $sd | Out-Null; [IO.File]::WriteAllText("$sd\a.json", '{"name":"Source A","direct":{"grid":"https://example.com/none.png"}}')
$r = App $rootG 'state'; Check 'sources offered' (@($r.value.artSources) -contains 'Source A') ($r.value.artSources -join ','); Check 'all image types offered by default' (@($r.value.artChoice.kinds).Count -eq 5)
$r = App $rootG 'fetch_art' @{ kinds = @('wide'); source = 'Unknown' }; Check 'unknown source refused' (-not $r.ok)
$r = App $rootG 'fetch_art' @{ kinds = @('wide'); source = 'Source A'; replace = $true; file = $g }; $r2 = App $rootG 'state'; Check 'choices remembered' ($r.ok -and @($r2.value.artChoice.kinds) -join ',' -eq 'wide' -and $r2.value.artChoice.source -eq 'Source A' -and $r2.value.artChoice.replace) ($r | ConvertTo-Json -Compress)
Check 'chosen images only, existing image kept' (Test-Path -LiteralPath "$rootG\Covers\Run Game-grid.png")
"-- settings of one game"
$gi = "$rootG\Data\config\custom\0400000000000002.ini"
$r = App $rootG 'game_settings_get' @{ file = $g }; Check 'game follows the common settings at first' ($r.ok -and $null -eq $r.value.own.use_docked_mode -and $r.value.common.language_index -eq 2) ($r | ConvertTo-Json -Compress -Depth 4)
$r = App $rootG 'game_settings_set' @{ file = $g; key = 'use_docked_mode'; value = 1 }; $t = [IO.File]::ReadAllText($gi); Check 'game setting written where the emulators read it' ($r.ok -and $t.Contains("use_docked_mode\use_global=false`r`nuse_docked_mode\default=false`r`nuse_docked_mode=1`r`n")) $t
$r = App $rootG 'game_settings_set' @{ file = $g; key = 'speed'; value = 0 }; $r = App $rootG 'game_settings_get' @{ file = $g }; Check 'own values read back' ($r.value.own.use_docked_mode -eq 1 -and $r.value.own.speed -eq 0 -and $null -eq $r.value.own.volume) ($r | ConvertTo-Json -Compress -Depth 4)
$r = App $rootG 'state'; Check 'game marked as having its own settings' (@($r.value.games | Where-Object { $_.file -eq $g -and $_.own }).Count -eq 1)
$r = App $rootG 'custom_games'; Check 'listed with its settings' (@($r.value).Count -eq 1 -and (@($r.value[0].own) -join ',') -eq 'use_docked_mode,speed') ($r | ConvertTo-Json -Compress -Depth 4)
$r = App $rootG 'game_settings_set' @{ file = $g; key = 'use_docked_mode'; value = $null }; $t = [IO.File]::ReadAllText($gi); Check 'game follows the common setting again' ($r.ok -and $t.Contains("use_docked_mode\use_global=true") -and -not $t.Contains('use_docked_mode=1')) $t
$r = App $rootG 'game_settings_set' @{ file = $g; key = 'language_index'; value = 99 }; Check 'game value out of range refused' (-not $r.ok)
[IO.File]::WriteAllText("$rootG\Roms\No Id Game.nup", 'x'); $r = App $rootG 'game_settings_get' @{ file = 'No Id Game.nup' }; Check 'game without id explained' ((-not $r.ok) -and $r.error -match '0100000000010000')
$r = App $rootG 'game_settings_set' @{ file = '..\x [0400000000000009].nup'; key = 'volume'; value = 10 }; Check 'unknown game refused for settings' (-not $r.ok)
$mem = "use_multi_core\use_global=true`r`nmemory_layout_mode\use_global=false`r`nmemory_layout_mode\default=true`r`nmemory_layout_mode=2`r`n"; $t = [IO.File]::ReadAllText($gi); $t = if ($t.Contains("[Core]`r`n")) { $t.Replace("[Core]`r`n", "[Core]`r`n$mem") } else { $t + "[Core]`r`n$mem" }; [IO.File]::WriteAllText($gi, $t)
$r = App $rootG 'game_settings_get' @{ file = $g }; $o = @($r.value.otherList); Check 'setting changed in the emulator named' ($r.ok -and $o.Count -eq 1 -and $o[0].key -eq 'memory_layout_mode' -and $o[0].section -eq 'Core' -and $o[0].name -match '^M.moire .mul.e$') ($r | ConvertTo-Json -Compress -Depth 4)
$r = App $rootG 'game_settings_other_common' @{ file = $g; section = 'Core'; key = 'use_multi_core' }; Check 'setting not changed for the game refused' (-not $r.ok)
$r = App $rootG 'game_settings_other_common' @{ file = $g; section = 'Core'; key = 'memory_layout_mode' }; $t = [IO.File]::ReadAllText($gi); Check 'setting changed in the emulator follows the common one again' ($r.ok -and $t.Contains("memory_layout_mode\use_global=true`r`n") -and -not $t.Contains('memory_layout_mode=2') -and $t.Contains('use_multi_core\use_global=true')) $t
$r = App $rootG 'game_settings_get' @{ file = $g }; Check 'nothing left from the emulator' ($r.value.others -eq 0) ($r | ConvertTo-Json -Compress -Depth 4)
"-- image sources"
$r = App $rootG 'art_sources'; Check 'sources listed with SteamGridDB' ((@($r.value).name -join ',') -eq 'Source A,SteamGridDB' -and -not $r.value[1].ok) ($r | ConvertTo-Json -Compress)
$r = App $rootG 'art_source_enable' @{ name = 'Source A'; enabled = $false }; $r2 = App $rootG 'state'; Check 'source turned off' ($r.ok -and @($r2.value.artSources).Count -eq 0)
$r = App $rootG 'fetch_art' @{ kinds = @('wide') }; Check 'no active source, clear refusal' ((-not $r.ok) -and $r.error -match 'SteamGridDB')
App $rootG 'art_source_enable' @{ name = 'Source A'; enabled = $true } | Out-Null; $r = App $rootG 'state'; Check 'source turned on again' (@($r.value.artSources) -contains 'Source A')
[IO.File]::WriteAllText("$work\new-source.json", '{"name":"Source B","direct":{"icon":"https://example.com/{id}.png"}}'); $r = App $rootG 'art_source_add' @{ path = "$work\new-source.json" }; Check 'source added' ($r.ok -and (Test-Path "$sd\Source B.json")) ($r | ConvertTo-Json -Compress)
$r = App $rootG 'art_source_add' @{ path = "$work\new-source.json" }; Check 'same name refused' (-not $r.ok)
[IO.File]::WriteAllText("$work\bad-source.json", '{"name":"Bad"}'); $r = App $rootG 'art_source_add' @{ path = "$work\bad-source.json" }; Check 'unusable source refused' ((-not $r.ok) -and -not (Test-Path "$sd\Bad.json"))
$r = App $rootG 'grid_key_test' @{ key = '' }; Check 'key test without a key refused' (-not $r.ok)
"-- first start"
Remove-Item Env:\FORKBAY_ROOT; $env:FORKBAY_INSTALLTEST = '1'; $env:FORKBAY_SHORTCUT_DIR = "$work\links"
$dl = "$work\Downloads"; New-Item -ItemType Directory -Force $dl | Out-Null; Copy-Item $Bin "$dl\forkbay-windows-x86_64.exe"
function Inst([string]$exe, [string]$cmd, $arg = @{}) { $file = "$work\i.json"; if (Test-Path $file) { Remove-Item $file }; $json = ($arg | ConvertTo-Json -Compress -Depth 5).Replace('"', '\"'); Start-Process -FilePath $exe -ArgumentList '--cli', $cmd, "`"$json`"", "`"$file`"" -Wait -NoNewWindow; return Get-Content -LiteralPath $file -Raw -Encoding UTF8 | ConvertFrom-Json }
$old = "$dl\forkbay-windows-x86_64.exe"
$r = Inst $old 'state'; Check 'new copy asks where to go' $r.value.installPrompt ($r | ConvertTo-Json -Compress -Depth 2); Check 'nothing created before the choice' (-not (Test-Path "$dl\Files\Data"))
$r = Inst $old 'install_options'; Check 'downloads folder recognised' $r.value.temporary
$r = Inst $old 'install_to' @{ kind = 'custom'; custom = ''; startMenu = $false; desktop = $false }; Check 'empty folder refused' (-not $r.ok)
$r = Inst $old 'install_to' @{ kind = 'custom'; custom = "$work\apps"; startMenu = $true; desktop = $false }; $new = "$work\apps\Forkbay\Forkbay.exe"; Check 'program copied to the chosen folder' ($r.ok -and (Test-Path $new)) ($r | ConvertTo-Json -Compress)
$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut("$work\links\menu\Forkbay.lnk"); Check 'shortcut points at it' ($lnk.TargetPath -eq $new) $lnk.TargetPath; Check 'no shortcut where not asked' (-not (Test-Path "$work\links\desktop"))
$r = Inst $new 'state'; Check 'new place does not ask again' (-not $r.value.installPrompt)
[IO.File]::WriteAllText("$dl\other.exe", 'not this program'); Start-Process -FilePath $new -ArgumentList '--cleanup-install', "`"$dl\other.exe`"", '--version', "`"$work\v.txt`"" -Wait; Check 'another file is never removed' (Test-Path "$dl\other.exe")
New-Item -ItemType Directory -Force "$dl\Files\Manager" | Out-Null; Start-Process -FilePath $new -ArgumentList '--cleanup-install', "`"$old`"", '--version', "`"$work\v.txt`"" -Wait; Check 'old copy and its empty folder removed' (-not (Test-Path $old) -and -not (Test-Path "$dl\Files"))
$keep = "$work\Keep"; New-Item -ItemType Directory -Force "$keep\Files\Data" | Out-Null; Copy-Item $Bin "$keep\Forkbay.exe"; $r = Inst "$keep\Forkbay.exe" 'state'; Check 'folder already in use does not ask' (-not $r.value.installPrompt)
Start-Process -FilePath $new -ArgumentList '--cleanup-install', "`"$keep\Forkbay.exe`"", '--version', "`"$work\v.txt`"" -Wait; Check 'data of an old copy is kept' (Test-Path "$keep\Files\Data")
$stay = "$work\Stay"; New-Item -ItemType Directory -Force $stay | Out-Null; Copy-Item $Bin "$stay\Forkbay.exe"; $r = Inst "$stay\Forkbay.exe" 'install_stay'; $r = Inst "$stay\Forkbay.exe" 'state'; Check 'staying here is remembered' ($r.ok -and -not $r.value.installPrompt -and (Test-Path "$stay\Files\Roms")) ($r | ConvertTo-Json -Compress -Depth 2)
Remove-Item Env:\FORKBAY_INSTALLTEST, Env:\FORKBAY_SHORTCUT_DIR
"-- renamed emulators"
$rootH = "$work\H\Files"; New-Item -ItemType Directory -Force "$rootH\Roms", "$rootH\Lichii", "$rootH\Old Versions\Lichii frizti-v2", "$rootH\Data\cache\shaders\Lichii", "$rootH\Manager" | Out-Null
foreach ($d in "$rootH\Lichii", "$rootH\Old Versions\Lichii frizti-v2") { Copy-Item "$work\fake\fake.exe" "$d\fake.exe"; [IO.File]::WriteAllText("$d\exe.txt", 'fake.exe') }
[IO.File]::WriteAllText("$rootH\Lichii\version.txt", 'frizti-v3'); [IO.File]::WriteAllText("$rootH\Data\cache\shaders\Lichii\vulkan.bin", 's')
[IO.File]::WriteAllText("$rootH\Manager\settings.json", '{"setupDone":true,"defaultEmulator":"Lichii","games":{"Roms/A.nsp":"Lichii","Roms/B.nsp":"Eglantine"}}')
$r = App $rootH 'state'
Check 'renamed emulator folder moved' ((Test-Path "$rootH\Fritchii\fake.exe") -and -not (Test-Path "$rootH\Lichii"))
Check 'its kept version and shader cache follow' ((Test-Path "$rootH\Old Versions\Fritchii frizti-v2\fake.exe") -and (Test-Path "$rootH\Data\cache\shaders\Fritchii\vulkan.bin"))
Check 'saved choices use the new name' ($r.value.defaultEmulator -eq 'Fritchii') ($r.value.defaultEmulator)
$r = App $rootH 'emu_status'; $fr = @($r.value) | Where-Object name -eq 'Fritchii'; Check 'renamed emulator installed with its version' ($fr.present -and $fr.installed -eq 'frizti-v3' -and @($fr.old) -contains 'Fritchii frizti-v2')
$r = App $rootH 'emu_add_github' @{ name = 'Lichii'; repo = 'a/b' }; Check 'old name kept free' (-not $r.ok)
$rootI = "$work\I\Files"; New-Item -ItemType Directory -Force "$rootI\Roms", "$rootI\Lichii" | Out-Null; Copy-Item "$work\fake\fake.exe" "$rootI\Lichii\fake.exe"; [IO.File]::WriteAllText("$rootI\Lichii\exe.txt", 'fake.exe'); [IO.File]::WriteAllText("$rootI\Lichii\version.txt", 'build-40')
App $rootI 'state' | Out-Null; Check 'another emulator in that folder is not renamed' ((Test-Path "$rootI\Lichii\fake.exe") -and -not (Test-Path "$rootI\Fritchii"))
Remove-Item Env:\FORKBAY_ROOT, Env:\FORKBAY_SEARCH, Env:\FORKBAY_STEAM -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($script:fail) { 'RESULT: FAILED' } else { 'RESULT: all passed' }
exit $script:fail
