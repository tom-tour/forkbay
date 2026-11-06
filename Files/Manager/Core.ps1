# Noyau de Switch 2 Manager : réglages, chemins, installation et émulateurs.
# Tout est relatif au dossier Files : l'outil fonctionne depuis n'importe quel disque.
if (-not $script:Root) { $script:Root = Split-Path $PSScriptRoot -Parent }
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$script:GameExtensions = '.nup', '.nsp', '.xci'
$script:ToolExes = '(?i)(cli|redist|room|tests?|setup|unins|crash)'

# Émulateurs fournis (emulators.json) et ajoutés par l'utilisateur (user-emulators.json).
function Get-UserEmulatorPath { return Join-Path $script:Root 'Manager\user-emulators.json' }

function Read-EmulatorFile([string]$path) {
    if (-not (Test-Path -LiteralPath $path)) { return @() }
    $items = Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
    return @($items | ForEach-Object { $_ })
}

function Import-EmulatorList {
    $seen = @{}
    $script:Emus = @(foreach ($e in (@(Read-EmulatorFile (Join-Path $PSScriptRoot 'emulators.json')) + @(Read-EmulatorFile (Get-UserEmulatorPath)))) {
        if (-not $e.Name -or $seen[$e.Name]) { continue }
        $seen[$e.Name] = $true
        $source = [string]$e.Source
        $api = ''; $page = ''
        if ($source -eq 'github') { $api = "https://api.github.com/repos/$($e.Repo)/releases?per_page=40"; $page = "https://github.com/$($e.Repo)/releases" }
        elseif ($source -eq 'forgejo') { $api = "$($e.Base)/api/v1/repos/$($e.Repo)/releases?limit=10"; $page = "$($e.Base)/$($e.Repo)/releases" }
        @{ Name = [string]$e.Name; Exe = [string]$e.Exe; Source = $source; Api = $api; Page = $page; Tag = [string]$e.Tag; Asset = [string]$e.Asset; About = [string]$e.About; User = [bool]$e.User }
    })
}
Import-EmulatorList

function Get-EmulatorNames { return @($script:Emus | ForEach-Object { $_.Name }) }

function Save-UserEmulators($items) {
    [IO.File]::WriteAllText((Get-UserEmulatorPath), (ConvertTo-Json -InputObject @($items) -Depth 4), (New-Object Text.UTF8Encoding $false))
    Import-EmulatorList
}

function Test-EmulatorName([string]$name) {
    if ($name -notmatch '^[A-Za-z0-9][A-Za-z0-9 ._-]{0,39}$' -or $name -match '\.$') { throw 'Nom invalide : lettres, chiffres, espaces, points et tirets seulement.' }
    if ((Get-EmulatorNames) -contains $name) { throw "Un émulateur s'appelle déjà $name." }
    if (@('Data', 'Roms', 'Roms Dumps', 'Manager', 'Save Backups', 'Old Versions', 'Covers', 'Unverified') -contains $name) { throw "Ce nom est réservé : $name." }
}

# Ajoute un émulateur publié sur GitHub. La source est vérifiée avant d'être retenue.
function Add-GitHubEmulator([string]$name, [string]$repo, [string]$asset, [string]$about) {
    Test-EmulatorName $name
    $repo = ($repo.Trim() -replace '^https://github\.com/', '').Trim('/')
    if ($repo -notmatch '^[\w.-]+/[\w.-]+$') { throw 'Dépôt invalide : écris propriétaire/projet, ou colle son adresse GitHub.' }
    if (-not $asset) { $asset = '(?i)^(?!.*(symbols|debug|source|pgo)).*win.*\.zip$' }
    [void][regex]::new($asset)
    $entry = [pscustomobject]@{ Name = $name; Exe = ''; Source = 'github'; Repo = $repo; Tag = '.'; Asset = $asset; About = $(if ($about) { $about } else { "Ajouté depuis $repo" }); User = $true }
    $check = @{ Name = $name; Api = "https://api.github.com/repos/$repo/releases?per_page=40"; Tag = '.'; Asset = $asset }
    try { $l = Get-Latest $check } catch { throw "Aucune version Windows trouvée sur $repo : $($_.Exception.Message)" }
    Save-UserEmulators (@(Read-EmulatorFile (Get-UserEmulatorPath)) + $entry)
    return "$name ajouté : dernière version $($l.Tag). Clique sur Installer."
}

# Ajoute une copie locale, choisie par son .exe. Sans source en ligne : pas de vérification de version.
function Add-LocalEmulator([string]$name, [string]$exePath) {
    Test-EmulatorName $name
    if (-not (Test-Path -LiteralPath $exePath) -or [IO.Path]::GetExtension($exePath) -ine '.exe') { throw 'Choisis le fichier .exe de l''émulateur.' }
    $folder = Join-Path $script:Root $name
    if (Test-Path -LiteralPath $folder) { throw "Le dossier existe déjà : $folder" }
    robocopy (Split-Path $exePath) $folder /E /XJ /XD user /R:2 /W:2 /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw 'Copie impossible.' }
    [IO.File]::WriteAllText((Join-Path $folder 'exe.txt'), [IO.Path]::GetFileName($exePath))
    Set-EmulatorLink $folder (Get-DataPath)
    Save-UserEmulators (@(Read-EmulatorFile (Get-UserEmulatorPath)) + [pscustomobject]@{ Name = $name; Exe = [IO.Path]::GetFileName($exePath); Source = 'local'; About = 'Copie locale'; User = $true })
    return "$name ajouté. Ses mises à jour restent à faire à la main."
}

# Retire seulement l'entrée de la liste : le dossier de l'émulateur reste en place.
function Remove-UserEmulator([string]$name) {
    Save-UserEmulators @(Read-EmulatorFile (Get-UserEmulatorPath) | Where-Object { $_.Name -ne $name })
    return "$name retiré de la liste. Son dossier est conservé."
}

# --- Réglages ---
function Get-SettingsPath { return Join-Path $script:Root 'Manager\settings.json' }

function Get-Settings {
    $defaults = [pscustomobject]@{
        setupDone = $false; steamGridKey = ''
        dataLocation = 'portable'; customDataPath = ''; defaultEmulator = 'Lichii'; games = [pscustomobject]@{}
        sync = [pscustomobject]@{ enabled = $false; provider = 'folder'; folderPath = ''; googleClientId = ''; googleClientSecret = ''; googleRefreshToken = ''; keepBackups = 10 }
    }
    $path = Get-SettingsPath
    if (-not (Test-Path -LiteralPath $path)) { return $defaults }
    $loaded = Get-Content -LiteralPath $path -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($p in $defaults.PSObject.Properties) { if ($null -eq $loaded.PSObject.Properties[$p.Name]) { $loaded | Add-Member $p.Name $p.Value } }
    foreach ($p in $defaults.sync.PSObject.Properties) { if ($null -eq $loaded.sync.PSObject.Properties[$p.Name]) { $loaded.sync | Add-Member $p.Name $p.Value } }
    return $loaded
}

function Save-Settings($settings) {
    $tmp = (Get-SettingsPath) + '.tmp'
    [IO.File]::WriteAllText($tmp, ($settings | ConvertTo-Json -Depth 6), (New-Object Text.UTF8Encoding $false))
    Move-Item -LiteralPath $tmp -Destination (Get-SettingsPath) -Force
}

# Chiffré pour ce compte Windows : un autre PC devra se reconnecter, sans jamais lire le secret.
function Protect-Text([string]$text) { if (-not $text) { return '' }; return ConvertTo-SecureString $text -AsPlainText -Force | ConvertFrom-SecureString }
function Unprotect-Text([string]$text) {
    if (-not $text) { return '' }
    try { return [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR((ConvertTo-SecureString $text))) }
    catch { return '' }
}

# --- Chemins ---
function Get-DataPathFor($settings) {
    switch ($settings.dataLocation) {
        'local' { return Join-Path $env:LOCALAPPDATA 'Switch2Manager\Data' }
        'custom' { if ($settings.customDataPath) { return $settings.customDataPath } }
    }
    return Join-Path $script:Root 'Data'
}
function Get-DataPath { return Get-DataPathFor (Get-Settings) }

function Find-Exe([string]$folder) {
    $marker = Join-Path $folder 'exe.txt'
    if (Test-Path -LiteralPath $marker) {
        $path = Join-Path $folder ([IO.File]::ReadAllText($marker)).Trim()
        if (Test-Path -LiteralPath $path) { return $path }
    }
    foreach ($name in 'lichii.exe', 'eglantine.exe') {
        $path = Join-Path $folder $name
        if (Test-Path -LiteralPath $path) { return $path }
    }
}
function Get-EmulatorFolders { return @(Get-ChildItem -LiteralPath $script:Root -Directory | Where-Object { Find-Exe $_.FullName }) }

function Get-RunningEmulators {
    $paths = @(Get-EmulatorFolders | ForEach-Object { Find-Exe $_.FullName })
    $names = @($paths | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_) }) + 'lichii', 'eglantine', 'eden-cli' | Select-Object -Unique
    return @(Get-Process -Name $names -ErrorAction SilentlyContinue)
}
function Test-EmulatorRunning { return @(Get-RunningEmulators).Count -gt 0 }

function Get-RomFiles {
    $dir = Join-Path $script:Root 'Roms'
    if (-not (Test-Path -LiteralPath $dir)) { return @() }
    return @(Get-ChildItem -LiteralPath $dir -File | Where-Object { $script:GameExtensions -contains $_.Extension.ToLower() } | Sort-Object Name | ForEach-Object { $_.Name })
}

function Get-DisplayName([string]$fileName) {
    $name = [regex]::Replace([IO.Path]::GetFileNameWithoutExtension($fileName), '\s*[\[(][^\])]*[\])]', '').Trim()
    if ($name) { return $name }
    return [IO.Path]::GetFileNameWithoutExtension($fileName)
}

function Get-GameEmulator($settings, [string]$file) {
    $p = $settings.games.PSObject.Properties[$file]
    if ($p -and $p.Value) { return $p.Value }
    return $settings.defaultEmulator
}

# Relie chaque émulateur à Data et remet les chemins de la configuration à jour.
function Repair-Setup {
    $data = Get-DataPath
    foreach ($sub in 'config', 'keys', 'log', 'load', 'sdmc', 'dump', 'tas', 'screenshots', 'nand\user\save', 'nand\system\Contents\registered') {
        New-Item -ItemType Directory -Force (Join-Path $data $sub) | Out-Null
    }
    foreach ($e in Get-EmulatorFolders) { Set-EmulatorLink $e.FullName $data }
    Set-ConfigPaths $data
    return $data
}

function Set-EmulatorLink([string]$folder, [string]$data) {
    $link = Join-Path $folder 'user'
    $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
    if ($item) {
        if (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "$link est un vrai dossier : déplace son contenu dans Data, puis supprime-le." }
        if (@($item.Target)[0] -eq $data) { return }
        [IO.Directory]::Delete($link)   # retire le lien seulement, jamais sa cible
    }
    cmd /c mklink /J "$link" "$data" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Impossible de créer le lien : $link" }
}

function Set-ConfigPaths([string]$data) {
    $ini = Join-Path $data 'config\qt-config.ini'
    if (-not (Test-Path -LiteralPath $ini)) { return }
    $old = [IO.File]::ReadAllText($ini)
    $d = $data.Replace('\', '/'); $roms = (Join-Path $script:Root 'Roms').Replace('\', '/')
    $new = [regex]::Replace($old, '(?m)^(nand|sdmc|load|dump|tas)_directory=[^\r\n]*', { param($m) "$($m.Groups[1].Value)_directory=$d/$($m.Groups[1].Value)" })
    $new = [regex]::Replace($new, '(?m)^(Screenshots\\screenshot_path)=[^\r\n]*', { param($m) "$($m.Groups[1].Value)=$d/screenshots" })
    $new = [regex]::Replace($new, '(?m)^(Paths\\gamedirs\\\d+\\path)=([^\r\n]*)', { param($m)
        if ($m.Groups[2].Value -match ':/|^__LOCAL__$') { "$($m.Groups[1].Value)=$roms" } else { $m.Value } })
    if ($new -cne $old) { [IO.File]::WriteAllText($ini, $new, (New-Object Text.UTF8Encoding $false)) }
}

# --- État de l'installation ---
function Get-SetupStatus {
    $data = Get-DataPath
    $keys = Test-Path -LiteralPath (Join-Path $data 'keys\prod.keys')
    $firmware = @(Get-ChildItem -LiteralPath (Join-Path $data 'nand\system\Contents\registered') -Filter '*.nca' -ErrorAction SilentlyContinue).Count
    return [pscustomobject]@{
        Data = $data; Keys = $keys; Firmware = ($firmware -gt 0); Games = @(Get-RomFiles).Count
        Emulators = @($script:Emus | Where-Object { Find-Exe (Join-Path $script:Root $_.Name) }).Count
        KeysSource = (Find-KeysSource); FirmwareSource = (Find-FirmwareSource)
    }
}

function Find-KeysSource {
    $k = Get-ChildItem -LiteralPath $script:Root -Recurse -Depth 1 -Filter 'prod.keys' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -notmatch '\\(Data|Old Versions|Save Backups|Roms)\\' } | Select-Object -First 1
    if ($k) { return $k.DirectoryName }
    return ''
}
function Find-FirmwareSource {
    $z = Get-ChildItem -LiteralPath $script:Root -Filter 'Firmware*.zip' -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
    if ($z) { return $z.FullName }
    return ''
}

# Installe clés et firmware s'ils manquent. Ne remplace jamais ce qui existe.
function Install-Prerequisites {
    $data = Repair-Setup
    $done = @()
    if (-not (Test-Path -LiteralPath (Join-Path $data 'keys\prod.keys'))) {
        $src = Find-KeysSource
        if ($src) { foreach ($f in 'prod.keys', 'title.keys') { $p = Join-Path $src $f; if (Test-Path -LiteralPath $p) { Copy-Item -LiteralPath $p (Join-Path $data 'keys') } }; $done += 'clés' }
    }
    $reg = Join-Path $data 'nand\system\Contents\registered'
    if (-not @(Get-ChildItem -LiteralPath $reg -Filter '*.nca' -ErrorAction SilentlyContinue).Count) {
        $zip = Find-FirmwareSource
        if ($zip) { Expand-Archive -LiteralPath $zip -DestinationPath $reg -Force; $done += 'firmware' }
    }
    if ($done) { return 'Installé : ' + ($done -join ', ') + '.' }
    return 'Rien à installer.'
}

# Cherche les données d'émulateurs déjà présentes sur ce PC. Lecture seule.
function Find-ExistingInstalls {
    $current = (Get-DataPath).TrimEnd('\')
    foreach ($base in $env:APPDATA, $env:LOCALAPPDATA) {
        foreach ($name in 'eden', 'eglantine', 'lichii', 'orchid', 'yuzu', 'sudachi', 'citron', 'suyu', 'torzu', 'ryujinx') {
            $path = Join-Path $base $name
            if (-not (Test-Path -LiteralPath $path) -or $path -ieq $current) { continue }
            $keys = Test-Path -LiteralPath (Join-Path $path 'keys\prod.keys')
            $config = Test-Path -LiteralPath (Join-Path $path 'config\qt-config.ini')
            $saves = @(Get-ChildItem -LiteralPath (Join-Path $path 'nand\user\save') -Recurse -File -Force -ErrorAction SilentlyContinue).Count
            if (-not ($keys -or $config -or $saves)) { continue }
            [pscustomobject]@{ Name = $name; Path = $path; Keys = $keys; Config = $config; Saves = $saves; Compatible = (@('eden', 'eglantine', 'lichii', 'orchid') -contains $name) }
        }
    }
}

# Copie depuis une installation existante. La source n'est jamais modifiée, et rien de présent n'est écrasé.
function Import-ExistingInstall([string]$source) {
    $found = @(Find-ExistingInstalls | Where-Object { $_.Path -ieq $source })
    if (-not $found) { throw "Installation introuvable : $source" }
    if (Test-EmulatorRunning) { throw 'Ferme les émulateurs avant d''importer.' }
    $data = Repair-Setup
    $done = @()
    $keys = Join-Path $source 'keys'
    if ($found[0].Keys -and -not (Test-Path -LiteralPath (Join-Path $data 'keys\prod.keys'))) {
        robocopy $keys (Join-Path $data 'keys') /E /XC /XN /XO /R:2 /W:2 /NFL /NDL /NJH /NJS /NP | Out-Null
        $done += 'clés'
    }
    if ($found[0].Saves) {
        $saveDir = Join-Path $data 'nand\user\save'
        $countBefore = @(Get-ChildItem -LiteralPath $saveDir -Recurse -File -Force).Count
        robocopy (Join-Path $source 'nand\user\save') $saveDir /E /XC /XN /XO /R:2 /W:2 /NFL /NDL /NJH /NJS /NP | Out-Null
        if ($LASTEXITCODE -ge 8) { throw 'Copie des sauvegardes impossible.' }
        $profile = Join-Path $source 'nand\system\save\8000000000000010'
        if (Test-Path -LiteralPath $profile) { robocopy $profile (Join-Path $data 'nand\system\save\8000000000000010') /E /XC /XN /XO /R:2 /W:2 /NFL /NDL /NJH /NJS /NP | Out-Null }
        $added = @(Get-ChildItem -LiteralPath $saveDir -Recurse -File -Force).Count - $countBefore
        if ($added -gt 0) { $done += "$added fichier(s) de sauvegarde" }
    }
    if ($found[0].Compatible -and $found[0].Config -and -not (Test-Path -LiteralPath (Join-Path $data 'config\qt-config.ini'))) {
        robocopy (Join-Path $source 'config') (Join-Path $data 'config') /E /XC /XN /XO /R:2 /W:2 /NFL /NDL /NJH /NJS /NP | Out-Null
        Set-ConfigPaths $data
        $done += 'configuration'
    }
    if ($done) { return "Importé depuis $($found[0].Name) : " + ($done -join ', ') + '.' }
    return "Rien à importer depuis $($found[0].Name) : ce qui existe déjà n'est jamais écrasé."
}

# Déplace Data vers un autre emplacement. L'ancienne copie est gardée, jamais effacée.
function Move-DataLocation([string]$location, [string]$custom) {
    if (Test-EmulatorRunning) { throw 'Ferme les émulateurs avant de déplacer les données.' }
    $settings = Get-Settings
    $from = Get-DataPathFor $settings
    $settings.dataLocation = $location; $settings.customDataPath = $custom
    $to = Get-DataPathFor $settings
    if ($from -ieq $to) { return 'Les données sont déjà à cet endroit.' }
    if ((Test-Path -LiteralPath $to) -and @(Get-ChildItem -LiteralPath $to -Force).Count) { throw "Le dossier cible n'est pas vide : $to" }
    New-Item -ItemType Directory -Force $to | Out-Null
    robocopy $from $to /E /COPY:DAT /DCOPY:T /R:2 /W:2 /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw 'La copie a échoué : rien n''a changé.' }
    $a = Get-ChildItem -LiteralPath $from -Recurse -File -Force | Measure-Object Length -Sum
    $b = Get-ChildItem -LiteralPath $to -Recurse -File -Force | Measure-Object Length -Sum
    if ($a.Count -ne $b.Count -or $a.Sum -ne $b.Sum) { throw 'La copie est incomplète : rien n''a changé.' }
    Save-Settings $settings
    [void](Repair-Setup)
    $old = "$from.old-" + (Get-Date -Format 'yyyyMMdd-HHmmss')
    Rename-Item -LiteralPath $from -NewName (Split-Path $old -Leaf)
    return "Données déplacées vers $to. L'ancienne copie reste dans $old : supprime-la quand tout fonctionne."
}

# --- Émulateurs ---
function Get-Installed($emu) {
    $file = Join-Path (Join-Path $script:Root $emu.Name) 'version.txt'
    if (Test-Path -LiteralPath $file) { return ([IO.File]::ReadAllText($file)).Trim() }
    return ''
}

function Get-Latest($emu) {
    $releases = Invoke-RestMethod -Uri $emu.Api -UseBasicParsing -TimeoutSec 30 -Headers @{ 'User-Agent' = 'Switch2-Manager' }
    $best = $null
    foreach ($r in @($releases)) {
        if ($r.draft -or $r.tag_name -notmatch $emu.Tag) { continue }
        $asset = @($r.assets) | Where-Object { $_.name -match $emu.Asset } | Select-Object -First 1
        if (-not $asset) { continue }
        $date = [datetime]$r.published_at
        if (-not $best -or $date -gt $best.Date) {
            $best = @{ Tag = $r.tag_name; Date = $date; Asset = $asset.name; Url = $asset.browser_download_url
                       Size = [long]$asset.size; Digest = [string]$asset.digest; Page = [string]$r.html_url }
        }
    }
    if (-not $best) { throw 'Aucune version Windows trouvée.' }
    return $best
}

function Get-EmulatorStatus {
    foreach ($emu in $script:Emus) {
        $folder = Join-Path $script:Root $emu.Name
        $row = [ordered]@{ Name = $emu.Name; About = $emu.About; Present = [bool](Find-Exe $folder); Installed = (Get-Installed $emu); Latest = ''; Date = ''; Size = 0; Page = $emu.Page; UpToDate = $false; Error = ''; User = $emu.User; Local = ($emu.Source -eq 'local') }
        if ($row.Local) { [pscustomobject]$row; continue }
        try {
            $l = Get-Latest $emu
            $row.Latest = $l.Tag; $row.Date = $l.Date.ToString('dd/MM/yyyy'); $row.Size = $l.Size
            if ($l.Page) { $row.Page = $l.Page }
            $row.UpToDate = ($row.Present -and $row.Installed -eq $l.Tag)
        } catch { $row.Error = $_.Exception.Message }
        [pscustomobject]$row
    }
}

function Install-Emulator([string]$name) {
    $emu = $script:Emus | Where-Object { $_.Name -eq $name }
    if (-not $emu) { throw "Émulateur inconnu : $name" }
    if ($emu.Source -eq 'local') { throw "$name est une copie locale : elle se met à jour à la main." }
    $folder = Join-Path $script:Root $emu.Name
    $running = Get-RunningEmulators | Where-Object { $_.Path -and $_.Path.StartsWith("$folder\", [StringComparison]::OrdinalIgnoreCase) }
    if ($running) { throw "$name est ouvert : ferme-le d'abord." }

    $l = Get-Latest $emu
    $present = [bool](Find-Exe $folder)
    $installed = Get-Installed $emu
    if ($present -and $installed -eq $l.Tag) { return "$name est déjà à jour ($installed)." }

    $work = Join-Path $script:Root "_update\$($emu.Name)"
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
    New-Item -ItemType Directory -Force $work | Out-Null
    $zip = Join-Path $work 'download.zip'
    Invoke-WebRequest -Uri $l.Url -OutFile $zip -UseBasicParsing -TimeoutSec 1800 -Headers @{ 'User-Agent' = 'Switch2-Manager' }

    if ((Get-Item -LiteralPath $zip).Length -ne $l.Size) { throw 'Téléchargement incomplet : taille différente.' }
    $checked = 'taille'
    if ($l.Digest -match '^sha256:(.+)$') {
        if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne $Matches[1]) { throw 'Condensat différent : fichier refusé.' }
        $checked = 'condensat SHA-256'
    }

    $extract = Join-Path $work 'x'
    Expand-Archive -LiteralPath $zip -DestinationPath $extract
    # Exécutable inconnu (émulateur ajouté par l'utilisateur) : le plus gros qui n'est pas un outil annexe.
    if ($emu.Exe) { $exe = Get-ChildItem -LiteralPath $extract -Recurse -Filter $emu.Exe | Select-Object -First 1 }
    else { $exe = Get-ChildItem -LiteralPath $extract -Recurse -Filter '*.exe' | Where-Object { $_.BaseName -notmatch $script:ToolExes } | Sort-Object Length -Descending | Select-Object -First 1 }
    if (-not $exe) { throw "Aucun exécutable d'émulateur dans l'archive." }
    $new = $exe.DirectoryName
    $exeName = $exe.Name

    $defender = Join-Path $env:ProgramFiles 'Windows Defender\MpCmdRun.exe'
    if (Test-Path -LiteralPath $defender) {
        & $defender -Scan -ScanType 3 -File $new -DisableRemediation | Out-Null
        if ($LASTEXITCODE -eq 2) { throw 'Defender signale une menace : installation annulée.' }
    }

    $data = Get-DataPath
    $label = if ($installed) { $installed } else { 'inconnue' }
    if (Test-Path -LiteralPath $folder) {
        # Le lien est retiré avant le déplacement : une ancienne version ne doit jamais pointer vers les données.
        $link = Join-Path $folder 'user'
        $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
        if ($item) {
            if (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "$link est un vrai dossier : installation annulée." }
            [IO.Directory]::Delete($link)
        }
        $oldRoot = Join-Path $script:Root 'Old Versions'
        New-Item -ItemType Directory -Force $oldRoot | Out-Null
        $old = Join-Path $oldRoot "$($emu.Name) $label"
        if (Test-Path -LiteralPath $old) { $old += ' ' + (Get-Date -Format 'yyyyMMdd-HHmmss') }
        Move-Item -LiteralPath $folder -Destination $old
        try { Move-Item -LiteralPath $new -Destination $folder }
        catch { Move-Item -LiteralPath $old -Destination $folder; Set-EmulatorLink $folder $data; throw }
    } else {
        Move-Item -LiteralPath $new -Destination $folder
    }
    [IO.File]::WriteAllText((Join-Path $folder 'version.txt'), $l.Tag)
    [IO.File]::WriteAllText((Join-Path $folder 'exe.txt'), $exeName)
    Set-EmulatorLink $folder $data
    Remove-Item -LiteralPath $work -Recurse -Force
    $parent = Join-Path $script:Root '_update'
    if (@(Get-ChildItem -LiteralPath $parent -Force).Count -eq 0) { [IO.Directory]::Delete($parent) }
    if ($present) { return "$name : $label -> $($l.Tag) (vérifié : $checked). Ancienne version gardée dans Old Versions." }
    return "$name $($l.Tag) installé (vérifié : $checked)."
}

. (Join-Path $PSScriptRoot 'Steam.ps1')
. (Join-Path $PSScriptRoot 'Cloud.ps1')
. (Join-Path $PSScriptRoot 'Art.ps1')
