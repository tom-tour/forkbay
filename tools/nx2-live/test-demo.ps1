# Live test on the demo's Fritchii: a profile with A on K is applied by injection, then the emulator's own settings file is read.
$demo = 'G:\Claude\Outils\forkbay\target\demo'
$cfg = "$demo\root\Data\config"
$out = 'G:\Claude\Outils\forkbay\tools\nx2-live\out\msvc'
$env:FORKBAY_ROOT = "$demo\root"
Get-Process lichii -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*target\demo*' } | Stop-Process -Force
Copy-Item "$cfg\qt-config.ini" "$cfg\qt-config.ini.live-backup" -Force

# The profile, written by NX2 Launcher from the current controls, then A set to K (code 75).
$f = "$cfg\out.json"
Start-Process "$demo\nx2-launcher.exe" -ArgumentList '--cli', 'profile_save', '"{\"name\":\"NX2 Live\",\"player\":0,\"new\":true}"', "`"$f`"" -Wait -NoNewWindow
"profile_save: " + (Get-Content $f -Raw); Remove-Item $f
$p = "$cfg\input\NX2 Live.ini"
(Get-Content $p) -replace '^(button_a=).*$', '$1"engine:keyboard,code:75,toggle:0"' | Set-Content $p -Encoding UTF8
"profile A: " + (Select-String -Path $p -Pattern '^button_a=').Line
"before:    " + (Select-String -Path "$cfg\qt-config.ini" -Pattern '^player_0_button_a=').Line

$emu = Start-Process "$demo\root\Fritchii\lichii.exe" -WorkingDirectory "$demo\root\Fritchii" -PassThru
Start-Sleep 8
"windows: " + ((Get-Process -Id $emu.Id).MainWindowTitle)
[IO.File]::WriteAllText("$out\nx2-live.req", "0`tNX2 Live`n", [Text.UTF8Encoding]::new($false))
$t = Get-Date
& "$out\nx2-inject.exe" $emu.Id "$out\nx2-live.dll"
"took " + [int]((Get-Date) - $t).TotalMilliseconds + " ms"
Start-Sleep 1
"after:     " + (Select-String -Path "$cfg\qt-config.ini" -Pattern '^player_0_button_a=').Line
"still running: " + (-not $emu.HasExited)
"DLL still loaded: " + [bool]((Get-Process -Id $emu.Id).Modules | Where-Object ModuleName -eq 'nx2-live.dll')
