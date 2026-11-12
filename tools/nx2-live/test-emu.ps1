# Live test on an emulator copied into the demo: a profile with A on K is applied by injection.
# Usage: test-emu.ps1 <folder in the demo root> <exe name> <build: msvc|mingw>
param([string]$Folder, [string]$Exe, [string]$Build = 'msvc')
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$demo = 'G:\Claude\Outils\forkbay\target\demo'
$cfg = "$demo\root\Data\config"
$out = "G:\Claude\Outils\forkbay\tools\nx2-live\out\$Build"
$env:FORKBAY_ROOT = "$demo\root"
Copy-Item "$cfg\qt-config.ini" "$cfg\qt-config.ini.live-backup" -Force
$f = "$cfg\out.json"
Start-Process "$demo\nx2-launcher.exe" -ArgumentList '--cli', 'profile_save', '"{\"name\":\"NX2 Live\",\"player\":0,\"new\":false}"', "`"$f`"" -Wait -NoNewWindow
if ((Get-Content $f -Raw) -notmatch '"ok":true') { Start-Process "$demo\nx2-launcher.exe" -ArgumentList '--cli', 'profile_save', '"{\"name\":\"NX2 Live\",\"player\":0,\"new\":true}"', "`"$f`"" -Wait -NoNewWindow }
Remove-Item $f
$p = "$cfg\input\NX2 Live.ini"
(Get-Content $p) -replace '^(button_a=).*$', '$1"engine:keyboard,code:75,toggle:0"' | Set-Content $p -Encoding UTF8
"before: " + (Select-String -Path "$cfg\qt-config.ini" -Pattern '^player_0_button_a=').Line

$emu = Start-Process "$demo\root\$Folder\$Exe" -WorkingDirectory "$demo\root\$Folder" -PassThru
Start-Sleep 9
"window: " + (Get-Process -Id $emu.Id).MainWindowTitle
"Qt: " + (Get-Item "$demo\root\$Folder\Qt6Core.dll").VersionInfo.ProductVersion
# First start questions of the demo (no keys, no settings yet): answered the way that changes nothing.
$root = [Windows.Automation.AutomationElement]::RootElement.FindFirst([Windows.Automation.TreeScope]::Children, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ProcessIdProperty, $emu.Id)))
for ($i = 0; $i -lt 4; $i++) {
  $dlg = $root.FindFirst([Windows.Automation.TreeScope]::Children, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Window)))
  if (-not $dlg) { break }
  "question: " + $dlg.Current.Name
  $btn = $dlg.FindAll([Windows.Automation.TreeScope]::Descendants, (New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::Button))) | Where-Object { $_.Current.Name -match 'Later|30 FPS|^OK$|^No$|Cancel|Close' } | Select-Object -First 1
  if (-not $btn) { 'no known answer'; break }
  $btn.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke(); Start-Sleep 2
}
[IO.File]::WriteAllText("$out\nx2-live.req", "0`tNX2 Live`n", [Text.UTF8Encoding]::new($false))
$t = Get-Date
& "$out\nx2-inject.exe" $emu.Id "$out\nx2-live.dll"
"took " + [int]((Get-Date) - $t).TotalMilliseconds + " ms"
Start-Sleep 2
"after:  " + (Select-String -Path "$cfg\qt-config.ini" -Pattern '^player_0_button_a=').Line
"still running: " + (-not $emu.HasExited) + " / DLL unloaded: " + (-not ((Get-Process -Id $emu.Id).Modules | Where-Object ModuleName -eq 'nx2-live.dll'))
Stop-Process -Id $emu.Id -Force; Start-Sleep 1
Copy-Item "$cfg\qt-config.ini.live-backup" "$cfg\qt-config.ini" -Force; Remove-Item "$cfg\qt-config.ini.live-backup", $p
