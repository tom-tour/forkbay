# Raccourcis Steam (fichier binaire shortcuts.vdf : 0 = groupe, 1 = texte, 2 = entier, 8 = fin).
Add-Type @'
public static class SteamCrc {
    public static uint Compute(byte[] data) {
        uint crc = 0xFFFFFFFF;
        foreach (byte b in data) {
            crc ^= b;
            for (int i = 0; i < 8; i++) crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xEDB88320 : crc >> 1;
        }
        return ~crc;
    }
}
'@

function Read-CString([byte[]]$b, [ref]$pos) {
    $start = $pos.Value
    while ($true) {
        if ($pos.Value -ge $b.Length) { throw 'Fichier tronqué.' }
        if ($b[$pos.Value] -eq 0) { break }
        $pos.Value++
    }
    $text = [Text.Encoding]::UTF8.GetString($b, $start, $pos.Value - $start)
    $pos.Value++
    return $text
}

function Read-Map([byte[]]$b, [ref]$pos) {
    $nodes = New-Object Collections.ArrayList
    while ($true) {
        if ($pos.Value -ge $b.Length) { throw 'Fichier tronqué.' }
        $type = $b[$pos.Value]; $pos.Value++
        if ($type -eq 8) { return , $nodes }
        $key = Read-CString $b $pos
        switch ($type) {
            0 { $value = Read-Map $b $pos }
            1 { $value = Read-CString $b $pos }
            2 { $value = [BitConverter]::ToInt32($b, $pos.Value); $pos.Value += 4 }
            default { throw "Type inconnu : $type" }
        }
        [void]$nodes.Add(@{ T = [int]$type; K = $key; V = $value })
    }
}

function Write-CString($stream, [string]$text) {
    $t = [Text.Encoding]::UTF8.GetBytes($text)
    $stream.Write($t, 0, $t.Length)
    $stream.WriteByte(0)
}

function Write-Map($stream, $nodes) {
    foreach ($n in $nodes) {
        $stream.WriteByte([byte]$n.T)
        Write-CString $stream $n.K
        switch ($n.T) {
            0 { Write-Map $stream $n.V }
            1 { Write-CString $stream $n.V }
            2 { $i = [BitConverter]::GetBytes([int]$n.V); $stream.Write($i, 0, 4) }
        }
    }
    $stream.WriteByte(8)
}

function Convert-ToBytes($rootNodes) {
    $ms = New-Object IO.MemoryStream
    Write-Map $ms $rootNodes
    return , $ms.ToArray()
}

# Refuse tout fichier qu'il ne sait pas réécrire à l'identique.
function Read-Shortcuts([string]$path) {
    if (-not (Test-Path -LiteralPath $path) -or (Get-Item -LiteralPath $path).Length -eq 0) { return , (New-Object Collections.ArrayList) }
    $bytes = [IO.File]::ReadAllBytes($path)
    $pos = 0
    $nodes = Read-Map $bytes ([ref]$pos)
    if ($pos -ne $bytes.Length -or $nodes.Count -ne 1 -or $nodes[0].K -ine 'shortcuts') { throw 'Format de shortcuts.vdf inattendu.' }
    $copy = Convert-ToBytes $nodes
    if ([Convert]::ToBase64String($copy) -ne [Convert]::ToBase64String($bytes)) { throw 'Relecture différente de l''original : fichier non modifié.' }
    return , $nodes[0].V
}

function Get-Field($entry, [string]$name) {
    foreach ($n in $entry.V) { if ($n.K -ieq $name) { return $n.V } }
    return $null
}
function Set-Field($entry, [string]$name, $value) {
    foreach ($n in $entry.V) { if ($n.K -ieq $name) { $n.V = $value; return } }
}

function Get-ManagedGame($entry) {
    $m = [regex]::Match([string](Get-Field $entry 'LaunchOptions'), "Launcher\.ps1'.*?-Game '((?:[^']|'')+)'")
    if ($m.Success) { return $m.Groups[1].Value.Replace("''", "'") }
    return $null
}

# La lettre du disque n'est pas stockée : la commande la cherche au lancement.
function Get-LaunchOptions([string]$file) {
    $full = Join-Path $script:Root 'Launcher.ps1'
    $relative = $full.Substring([IO.Path]::GetPathRoot($full).Length).Replace("'", "''")
    $game = "Roms\$file".Replace("'", "''")
    $message = 'Dossier Switch 2 introuvable. Branche le disque, ou ouvre Switch 2 Manager depuis son nouvel emplacement et enregistre Steam.'
    return "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command `"foreach(`$d in Get-PSDrive -PSProvider FileSystem){`$s=`$d.Root+'$relative';if(Test-Path -LiteralPath `$s){& `$s -Game '$game' -Wait;exit}};Add-Type -AssemblyName System.Windows.Forms;[void][Windows.Forms.MessageBox]::Show('$message')`""
}

function Get-SteamMarker {
    $full = Join-Path $script:Root 'Launcher.ps1'
    return "'" + $full.Substring([IO.Path]::GetPathRoot($full).Length).Replace("'", "''") + "'"
}

function Get-SteamIcon([string]$file) {
    $art = Find-Art $file 'icon'
    if ($art) { return $art }
    $exe = Find-Exe (Join-Path $script:Root (Get-GameEmulator (Get-Settings) $file))
    if ($exe) { return $exe }
    return ''
}

function New-SteamEntry([string]$file) {
    $psExe = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $exe = "`"$psExe`""
    $name = Get-DisplayName $file
    $crc = [SteamCrc]::Compute([Text.Encoding]::UTF8.GetBytes($exe + $name)) -bor [uint32]2147483648
    $appid = [BitConverter]::ToInt32([BitConverter]::GetBytes([uint32]$crc), 0)
    $fields = New-Object Collections.ArrayList
    foreach ($f in @(
            @(2, 'appid', $appid), @(1, 'AppName', $name), @(1, 'Exe', $exe), @(1, 'StartDir', ((Split-Path $psExe) + '\')),
            @(1, 'icon', (Get-SteamIcon $file)), @(1, 'ShortcutPath', ''), @(1, 'LaunchOptions', (Get-LaunchOptions $file)),
            @(2, 'IsHidden', 0), @(2, 'AllowDesktopConfig', 1), @(2, 'AllowOverlay', 1), @(2, 'OpenVR', 0), @(2, 'Devkit', 0),
            @(1, 'DevkitGameID', ''), @(2, 'DevkitOverrideAppID', 0), @(2, 'LastPlayTime', 0), @(1, 'FlatpakAppID', ''))) {
        [void]$fields.Add(@{ T = [int]$f[0]; K = [string]$f[1]; V = $f[2] })
    }
    [void]$fields.Add(@{ T = 0; K = 'tags'; V = (New-Object Collections.ArrayList) })
    return @{ T = 0; K = '0'; V = $fields }
}

# Ajoute, retire et met à jour les jeux de l'outil. Les autres raccourcis ne changent pas.
function Save-Shortcuts([string]$path, [string[]]$wanted) {
    $entries = Read-Shortcuts $path
    $result = New-Object Collections.ArrayList
    $done = @{}
    foreach ($e in $entries) {
        $game = Get-ManagedGame $e
        if (-not $game) { [void]$result.Add($e); continue }
        $file = $game -replace '^Roms\\', ''
        if ($wanted -notcontains $file) { continue }
        Set-Field $e 'LaunchOptions' (Get-LaunchOptions $file)
        Set-Field $e 'icon' (Get-SteamIcon $file)
        [void]$result.Add($e)
        $done[$file] = $true
    }
    foreach ($file in $wanted) { if (-not $done[$file]) { [void]$result.Add((New-SteamEntry $file)) } }
    for ($i = 0; $i -lt $result.Count; $i++) { $result[$i].K = [string]$i }

    $bytes = Convert-ToBytes (New-Object Collections.ArrayList (, @(@{ T = 0; K = 'shortcuts'; V = $result })))
    if (Test-Path -LiteralPath $path) {
        $backup = "$path.before-switch2"
        if (-not (Test-Path -LiteralPath $backup)) { Copy-Item -LiteralPath $path $backup }
    } else {
        New-Item -ItemType Directory -Force (Split-Path $path) | Out-Null
    }
    $tmp = "$path.tmp"
    [IO.File]::WriteAllBytes($tmp, $bytes)
    [void](Read-Shortcuts $tmp)
    Move-Item -LiteralPath $tmp -Destination $path -Force
}

function Get-SteamPath {
    $key = Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue
    if (-not $key -or -not $key.SteamPath) { return '' }
    return $key.SteamPath.Replace('/', '\')
}

function Get-SteamVdfPath([string]$steam, [string]$id) { return Join-Path $steam "userdata\$id\config\shortcuts.vdf" }

function Get-SteamAccounts([string]$steam) {
    $names = @{}
    $login = Join-Path $steam 'config\loginusers.vdf'
    if (Test-Path -LiteralPath $login) {
        $text = [IO.File]::ReadAllText($login)
        foreach ($m in [regex]::Matches($text, '"(\d{17})"\s*\{[^}]*?"PersonaName"\s*"([^"]*)"', 'Singleline')) {
            $names[[string]([uint64]$m.Groups[1].Value - 76561197960265728)] = $m.Groups[2].Value
        }
    }
    $active = [string](Get-ItemProperty 'HKCU:\Software\Valve\Steam\ActiveProcess' -ErrorAction SilentlyContinue).ActiveUser
    $dir = Join-Path $steam 'userdata'
    if (-not (Test-Path -LiteralPath $dir)) { return @() }
    $list = foreach ($d in Get-ChildItem -LiteralPath $dir -Directory | Where-Object { $_.Name -match '^\d+$' -and $_.Name -ne '0' }) {
        $label = if ($names[$d.Name]) { $names[$d.Name] } else { $d.Name }
        [pscustomobject]@{ Id = $d.Name; Label = $label; Active = ($d.Name -eq $active) }
    }
    return @($list | Sort-Object @{ Expression = 'Active'; Descending = $true }, Label)
}
