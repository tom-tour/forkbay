# Synchronisation des sauvegardes et de la configuration. Clés et firmware ne sont jamais envoyés.
$script:SyncFile = 'switch2-sync.zip'
$script:GoogleScope = 'https://www.googleapis.com/auth/drive.file'

# Fichiers synchronisés, relatifs à Data.
function Get-SyncFiles([string]$data) {
    $list = New-Object Collections.ArrayList
    foreach ($dir in 'nand\user\save', 'nand\system\save\8000000000000010') {
        $full = Join-Path $data $dir
        if (Test-Path -LiteralPath $full) { Get-ChildItem -LiteralPath $full -Recurse -File -Force | ForEach-Object { [void]$list.Add($_.FullName.Substring($data.Length + 1)) } }
    }
    foreach ($f in 'config\qt-config.ini', 'config\sdl2-config.ini') { if (Test-Path -LiteralPath (Join-Path $data $f)) { [void]$list.Add($f) } }
    $custom = Join-Path $data 'config\custom'
    if (Test-Path -LiteralPath $custom) { Get-ChildItem -LiteralPath $custom -Filter '*.ini' -File | ForEach-Object { [void]$list.Add("config\custom\$($_.Name)") } }
    return , ($list | Sort-Object)
}

# Retire ce qui dépend du PC ou change à chaque lancement : la comparaison ne voit que les vrais changements.
function Get-NormalizedConfig([string]$path) {
    $t = [IO.File]::ReadAllText($path)
    $t = [regex]::Replace($t, '(?m)^(nand|sdmc|load|dump|tas)_directory=[^\r\n]*', { param($m) "$($m.Groups[1].Value)_directory=__LOCAL__" })
    $t = [regex]::Replace($t, '(?m)^(Screenshots\\screenshot_path)=[^\r\n]*', { param($m) "$($m.Groups[1].Value)=__LOCAL__" })
    $t = [regex]::Replace($t, '(?m)^(Paths\\gamedirs\\\d+\\path)=([^\r\n]*)', { param($m) if ($m.Groups[2].Value -match ':/') { "$($m.Groups[1].Value)=__LOCAL__" } else { $m.Value } })
    $t = [regex]::Replace($t, '(?m)^(Paths\\recentFiles)=[^\r\n]*', { param($m) "$($m.Groups[1].Value)=" })
    return $t
}

function Get-TextHash([string]$text) {
    $sha = [Security.Cryptography.SHA256]::Create()
    return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($text))) -replace '-', '')
}

function Get-LocalSyncHash([string]$data) {
    $parts = foreach ($rel in (Get-SyncFiles $data)) {
        $full = Join-Path $data $rel
        if ($rel -ieq 'config\qt-config.ini') { $h = Get-TextHash (Get-NormalizedConfig $full) }
        else { $h = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash }
        "$($rel.ToLower())|$h"
    }
    return Get-TextHash (($parts | Sort-Object) -join "`n")
}

# Date de la dernière vraie partie : la sauvegarde la plus récente.
function Get-LocalSyncDate([string]$data) {
    $save = Join-Path $data 'nand\user\save'
    $f = Get-ChildItem -LiteralPath $save -Recurse -File -Force -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if ($f) { return $f.LastWriteTimeUtc }
    return [datetime]::MinValue
}

function Get-SyncState([string]$data) {
    $p = Join-Path $data 'sync-state.json'
    if (Test-Path -LiteralPath $p) { return (Get-Content -LiteralPath $p -Raw -Encoding UTF8 | ConvertFrom-Json).lastHash }
    return ''
}
function Set-SyncState([string]$data, [string]$hash) {
    [IO.File]::WriteAllText((Join-Path $data 'sync-state.json'), (@{ lastHash = $hash; at = (Get-Date).ToUniversalTime().ToString('o') } | ConvertTo-Json), (New-Object Text.UTF8Encoding $false))
}

function Get-WorkDir {
    $w = Join-Path $script:Root 'Manager\_work'
    if (Test-Path -LiteralPath $w) { Remove-Item -LiteralPath $w -Recurse -Force }
    New-Item -ItemType Directory -Force $w | Out-Null
    return $w
}

function New-SyncZip([string]$data, [string]$work) {
    $snap = Join-Path $work 'snap'
    foreach ($rel in (Get-SyncFiles $data)) {
        $to = Join-Path $snap $rel
        New-Item -ItemType Directory -Force (Split-Path $to) | Out-Null
        if ($rel -ieq 'config\qt-config.ini') { [IO.File]::WriteAllText($to, (Get-NormalizedConfig (Join-Path $data $rel)), (New-Object Text.UTF8Encoding $false)) }
        else { Copy-Item -LiteralPath (Join-Path $data $rel) $to -Force }
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = Join-Path $work $script:SyncFile
    [IO.Compression.ZipFile]::CreateFromDirectory($snap, $zip)
    return $zip
}

function Backup-LocalSync([string]$data, [int]$keep) {
    $root = Join-Path $script:Root 'Save Backups'
    $dest = Join-Path $root ('auto-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    foreach ($rel in (Get-SyncFiles $data)) {
        $to = Join-Path $dest $rel
        New-Item -ItemType Directory -Force (Split-Path $to) | Out-Null
        Copy-Item -LiteralPath (Join-Path $data $rel) $to -Force
    }
    if ($keep -lt 1) { $keep = 1 }
    Get-ChildItem -LiteralPath $root -Directory -Filter 'auto-*' | Sort-Object Name -Descending | Select-Object -Skip $keep | ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force }
    return $dest
}

# Retire les fichiers synchronisés, pour que le résultat soit exactement la version installée.
function Clear-SyncFiles([string]$data) {
    foreach ($rel in (Get-SyncFiles $data)) { Remove-Item -LiteralPath (Join-Path $data $rel) -Force }
    $save = Join-Path $data 'nand\user\save'
    if (Test-Path -LiteralPath $save) { Remove-Item -LiteralPath $save -Recurse -Force }
}

function Restore-FromBackup([string]$backup, [string]$data) {
    Clear-SyncFiles $data
    robocopy $backup $data /E /NFL /NDL /NJH /NJS /NP | Out-Null
}

# Remplace la version locale. Copie de secours d'abord, retour arrière si le résultat ne correspond pas.
function Install-SyncZip([string]$zip, [string]$data, [string]$expected, $settings, [string]$work) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $x = Join-Path $work 'restore'
    [IO.Compression.ZipFile]::ExtractToDirectory($zip, $x)
    $backup = Backup-LocalSync $data ([int]$settings.sync.keepBackups)
    try {
        Clear-SyncFiles $data
        robocopy $x $data /E /NFL /NDL /NJH /NJS /NP | Out-Null
        if ($LASTEXITCODE -ge 8) { throw 'copie impossible' }
        Set-ConfigPaths $data
        $now = Get-LocalSyncHash $data
        if ($expected -and $now -ne $expected) { throw 'contenu différent de la version distante' }
    } catch {
        Restore-FromBackup $backup $data
        Set-ConfigPaths $data
        throw "Remplacement annulé, version locale restaurée : $($_.Exception.Message)"
    }
}

# --- Fournisseur : dossier (Google Drive pour ordinateur, OneDrive, NAS...) ---
function Get-FolderSyncDir($settings) {
    if (-not $settings.sync.folderPath) { throw 'Aucun dossier de synchronisation choisi.' }
    if (-not (Test-Path -LiteralPath $settings.sync.folderPath)) { throw "Dossier de synchronisation introuvable : $($settings.sync.folderPath)" }
    return Join-Path $settings.sync.folderPath 'Switch2Sync'
}

# --- Fournisseur : API Google Drive ---
function Get-GoogleToken($settings) {
    $refresh = Unprotect-Text $settings.sync.googleRefreshToken
    if (-not $refresh) { throw 'Compte Google non connecté sur ce PC : clique sur Connecter dans Cloud.' }
    $body = @{ client_id = $settings.sync.googleClientId; client_secret = (Unprotect-Text $settings.sync.googleClientSecret); refresh_token = $refresh; grant_type = 'refresh_token' }
    return (Invoke-RestMethod -Method Post -Uri 'https://oauth2.googleapis.com/token' -Body $body -UseBasicParsing -TimeoutSec 30).access_token
}

function Find-GoogleFile([string]$token) {
    $q = [uri]::EscapeDataString("name='$($script:SyncFile)' and trashed=false")
    $r = Invoke-RestMethod -Uri "https://www.googleapis.com/drive/v3/files?q=$q&fields=files(id,appProperties,modifiedTime)&spaces=drive" -Headers @{ Authorization = "Bearer $token" } -UseBasicParsing -TimeoutSec 30
    return @($r.files) | Select-Object -First 1
}

function Connect-Google($settings) {
    if (-not $settings.sync.googleClientId -or -not $settings.sync.googleClientSecret) { throw 'Renseigne d''abord l''identifiant et le secret du client OAuth.' }
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    $bytes = New-Object byte[] 48; $rng.GetBytes($bytes)
    $verifier = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
    $sha = [Security.Cryptography.SHA256]::Create()
    $challenge = [Convert]::ToBase64String($sha.ComputeHash([Text.Encoding]::ASCII.GetBytes($verifier))).TrimEnd('=').Replace('+', '-').Replace('/', '_')
    $state = [guid]::NewGuid().ToString('N')
    $probe = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 0); $probe.Start(); $port = $probe.LocalEndpoint.Port; $probe.Stop()
    $redirect = "http://127.0.0.1:$port/"
    $listener = New-Object Net.HttpListener; $listener.Prefixes.Add($redirect); $listener.Start()
    try {
        $url = 'https://accounts.google.com/o/oauth2/v2/auth?response_type=code&access_type=offline&prompt=consent' +
            "&client_id=$([uri]::EscapeDataString($settings.sync.googleClientId))&redirect_uri=$([uri]::EscapeDataString($redirect))" +
            "&scope=$([uri]::EscapeDataString($script:GoogleScope))&state=$state&code_challenge=$challenge&code_challenge_method=S256"
        Start-Process $url
        $wait = $listener.BeginGetContext($null, $null)
        if (-not $wait.AsyncWaitHandle.WaitOne([TimeSpan]::FromMinutes(5))) { throw 'Connexion Google non terminée après 5 minutes.' }
        $ctx = $listener.EndGetContext($wait)
        $query = $ctx.Request.QueryString
        $page = [Text.Encoding]::UTF8.GetBytes('<html><body style="font-family:sans-serif"><h2>Switch 2 Manager</h2><p>Tu peux fermer cet onglet et revenir à l''outil.</p></body></html>')
        $ctx.Response.ContentType = 'text/html; charset=utf-8'; $ctx.Response.OutputStream.Write($page, 0, $page.Length); $ctx.Response.Close()
        if ($query['state'] -ne $state) { throw 'Réponse Google non reconnue : connexion refusée.' }
        if (-not $query['code']) { throw "Connexion refusée par Google : $($query['error'])" }
        $body = @{ client_id = $settings.sync.googleClientId; client_secret = (Unprotect-Text $settings.sync.googleClientSecret); code = $query['code']; code_verifier = $verifier; grant_type = 'authorization_code'; redirect_uri = $redirect }
        $t = Invoke-RestMethod -Method Post -Uri 'https://oauth2.googleapis.com/token' -Body $body -UseBasicParsing -TimeoutSec 30
        if (-not $t.refresh_token) { throw 'Google n''a pas renvoyé de jeton durable.' }
        $settings.sync.googleRefreshToken = Protect-Text $t.refresh_token
        Save-Settings $settings
        return 'Compte Google connecté.'
    } finally { $listener.Stop(); $listener.Close() }
}

# --- Opérations communes ---
# Renvoie $null si rien n'existe encore à distance.
function Get-RemoteMeta($settings) {
    if ($settings.sync.provider -eq 'google') {
        $token = Get-GoogleToken $settings
        $f = Find-GoogleFile $token
        if (-not $f) { return $null }
        return [pscustomobject]@{ hash = $f.appProperties.hash; savedAt = $f.appProperties.savedAt; machine = $f.appProperties.machine; id = $f.id; token = $token }
    }
    $meta = Join-Path (Get-FolderSyncDir $settings) 'meta.json'
    if (-not (Test-Path -LiteralPath $meta)) { return $null }
    return Get-Content -LiteralPath $meta -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Send-Remote($settings, [string]$zip, [string]$hash, [datetime]$date) {
    $saved = $date.ToUniversalTime().ToString('o')
    if ($settings.sync.provider -eq 'google') {
        $token = Get-GoogleToken $settings
        $f = Find-GoogleFile $token
        $metadata = @{ appProperties = @{ hash = $hash; savedAt = $saved; machine = $env:COMPUTERNAME } }
        if (-not $f) { $metadata.name = $script:SyncFile }
        $boundary = 'switch2' + [guid]::NewGuid().ToString('N')
        $head = [Text.Encoding]::UTF8.GetBytes("--$boundary`r`nContent-Type: application/json; charset=UTF-8`r`n`r`n$($metadata | ConvertTo-Json -Depth 4)`r`n--$boundary`r`nContent-Type: application/zip`r`n`r`n")
        $tail = [Text.Encoding]::UTF8.GetBytes("`r`n--$boundary--`r`n")
        $ms = New-Object IO.MemoryStream
        $ms.Write($head, 0, $head.Length); $file = [IO.File]::ReadAllBytes($zip); $ms.Write($file, 0, $file.Length); $ms.Write($tail, 0, $tail.Length)
        $url = 'https://www.googleapis.com/upload/drive/v3/files'; $method = 'Post'
        if ($f) { $url += "/$($f.id)"; $method = 'Patch' }
        [void](Invoke-RestMethod -Method $method -Uri "$($url)?uploadType=multipart" -Headers @{ Authorization = "Bearer $token" } -ContentType "multipart/related; boundary=$boundary" -Body $ms.ToArray() -UseBasicParsing -TimeoutSec 600)
        return
    }
    $dir = Get-FolderSyncDir $settings
    New-Item -ItemType Directory -Force $dir | Out-Null
    $tmp = Join-Path $dir "$($script:SyncFile).tmp"
    Copy-Item -LiteralPath $zip $tmp -Force
    Move-Item -LiteralPath $tmp -Destination (Join-Path $dir $script:SyncFile) -Force
    [IO.File]::WriteAllText((Join-Path $dir 'meta.json'), (@{ hash = $hash; savedAt = $saved; machine = $env:COMPUTERNAME } | ConvertTo-Json), (New-Object Text.UTF8Encoding $false))
}

function Receive-Remote($settings, $meta, [string]$out) {
    if ($settings.sync.provider -eq 'google') {
        Invoke-WebRequest -Uri "https://www.googleapis.com/drive/v3/files/$($meta.id)?alt=media" -Headers @{ Authorization = "Bearer $($meta.token)" } -OutFile $out -UseBasicParsing -TimeoutSec 600
        return
    }
    Copy-Item -LiteralPath (Join-Path (Get-FolderSyncDir $settings) $script:SyncFile) $out -Force
}

function Send-Local($settings, [string]$data, [string]$hash) {
    $work = Get-WorkDir
    try {
        $zip = New-SyncZip $data $work
        Send-Remote $settings $zip $hash (Get-LocalSyncDate $data)
        Set-SyncState $data $hash
    } finally { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}

function Get-Local($settings, [string]$data, $meta) {
    $work = Get-WorkDir
    try {
        $zip = Join-Path $work 'remote.zip'
        Receive-Remote $settings $meta $zip
        Install-SyncZip $zip $data $meta.hash $settings $work
        Set-SyncState $data $meta.hash
    } finally { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}

# Compare local, distant et l'état connu au dernier échange.
#   -Resolve reçoit (dateLocale, dateDistante, machine) et renvoie 'local', 'remote' ou 'cancel'.
# Renvoie : none, uploaded, downloaded, cancelled.
function Invoke-CloudSync($settings, [scriptblock]$Resolve) {
    if (Test-EmulatorRunning) { throw 'Un émulateur est ouvert : synchronisation impossible pour l''instant.' }
    $data = Get-DataPath
    $meta = Get-RemoteMeta $settings
    $local = Get-LocalSyncHash $data
    $base = Get-SyncState $data
    if (-not $meta) { Send-Local $settings $data $local; return 'uploaded' }
    if ($meta.hash -eq $local) { if ($base -ne $local) { Set-SyncState $data $local }; return 'none' }
    if ($base -eq $local) { Get-Local $settings $data $meta; return 'downloaded' }
    if ($base -eq $meta.hash) { Send-Local $settings $data $local; return 'uploaded' }
    $choice = & $Resolve (Get-LocalSyncDate $data) ([datetime]$meta.savedAt) $meta.machine
    switch ($choice) {
        'local' { Send-Local $settings $data $local; return 'uploaded' }
        'remote' { Get-Local $settings $data $meta; return 'downloaded' }
    }
    return 'cancelled'
}

# Fenêtre de conflit : affiche les deux dates et laisse choisir.
function Show-ConflictDialog([datetime]$localDate, [datetime]$remoteDate, [string]$machine, [bool]$canCancel = $true) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $dark = [Drawing.Color]::FromArgb(24, 26, 32); $panel = [Drawing.Color]::FromArgb(36, 39, 48); $light = [Drawing.Color]::FromArgb(232, 234, 240)
    $form = New-Object Windows.Forms.Form
    $form.Text = 'Switch 2 Manager : conflit de sauvegarde'; $form.ClientSize = New-Object Drawing.Size(540, 262); $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedSingle'; $form.MaximizeBox = $false; $form.MinimizeBox = $false; $form.TopMost = $true
    $form.BackColor = $dark; $form.ForeColor = $light; $form.Font = New-Object Drawing.Font('Segoe UI', 10)
    $info = New-Object Windows.Forms.Label
    $info.Text = "Les deux versions ont changé depuis le dernier échange.`nCelle que tu ne gardes pas est remplacée, avec une copie de secours dans Save Backups si c'est la version locale."
    $info.Location = New-Object Drawing.Point(18, 16); $info.Size = New-Object Drawing.Size(506, 66)
    $form.Controls.Add($info)
    $fmt = 'dddd dd/MM/yyyy à HH:mm'
    $lt = if ($localDate -eq [datetime]::MinValue) { 'aucune sauvegarde' } else { $localDate.ToLocalTime().ToString($fmt, [Globalization.CultureInfo]'fr-FR') }
    $rt = $remoteDate.ToLocalTime().ToString($fmt, [Globalization.CultureInfo]'fr-FR')
    $script:ConflictChoice = 'cancel'
    $i = 0
    foreach ($v in @(@('Version locale (ce PC)', $lt, 'local', 'Garder la version locale'), @("Version distante ($machine)", $rt, 'remote', 'Garder la version distante'))) {
        $x = 18 + 258 * $i
        $box = New-Object Windows.Forms.Panel
        $box.Location = New-Object Drawing.Point($x, 88); $box.Size = New-Object Drawing.Size(248, 112); $box.BackColor = $panel
        $title = New-Object Windows.Forms.Label; $title.Text = $v[0]; $title.Location = New-Object Drawing.Point(10, 8); $title.Size = New-Object Drawing.Size(230, 22); $title.Font = New-Object Drawing.Font('Segoe UI', 10, [Drawing.FontStyle]::Bold)
        $when = New-Object Windows.Forms.Label; $when.Text = $v[1]; $when.Location = New-Object Drawing.Point(10, 32); $when.Size = New-Object Drawing.Size(230, 40)
        $btn = New-Object Windows.Forms.Button; $btn.Text = $v[3]; $btn.Tag = $v[2]; $btn.Location = New-Object Drawing.Point(10, 76); $btn.Size = New-Object Drawing.Size(228, 28); $btn.FlatStyle = 'Flat'
        $btn.BackColor = [Drawing.Color]::FromArgb(70, 110, 220); $btn.ForeColor = [Drawing.Color]::White
        $btn.Add_Click({ $script:ConflictChoice = $this.Tag; $this.FindForm().Close() })
        $box.Controls.AddRange(@($title, $when, $btn)); $form.Controls.Add($box); $i++
    }
    if ($canCancel) {
        $cancel = New-Object Windows.Forms.Button; $cancel.Text = 'Annuler le lancement'; $cancel.Location = New-Object Drawing.Point(18, 214); $cancel.Size = New-Object Drawing.Size(506, 30); $cancel.FlatStyle = 'Flat'
        $cancel.Add_Click({ $script:ConflictChoice = 'cancel'; $this.FindForm().Close() })
        $form.Controls.Add($cancel)
    }
    # Depuis un lanceur caché, Windows masquerait cette fenêtre : on la force visible et au premier plan.
    Add-Type -Namespace Switch2 -Name Win -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c); [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);' -ErrorAction SilentlyContinue
    $form.Add_Load({ [void][Switch2.Win]::ShowWindow($this.Handle, 5); [void][Switch2.Win]::ShowWindow($this.Handle, 5); [void][Switch2.Win]::SetForegroundWindow($this.Handle) })
    [void]$form.ShowDialog()
    return $script:ConflictChoice
}
