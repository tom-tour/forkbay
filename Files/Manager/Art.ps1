# Images des jeux depuis SteamGridDB (clé API gratuite), pour l'outil et pour Steam.
$script:ArtKinds = @(
    @{ Kind = 'grid'; Path = 'grids'; Query = '?dimensions=600x900&types=static'; Steam = 'p' }
    @{ Kind = 'wide'; Path = 'grids'; Query = '?dimensions=920x430,460x215&types=static'; Steam = '' }
    @{ Kind = 'hero'; Path = 'heroes'; Query = '?types=static'; Steam = '_hero' }
    @{ Kind = 'logo'; Path = 'logos'; Query = '?types=static'; Steam = '_logo' }
    @{ Kind = 'icon'; Path = 'icons'; Query = '?types=static'; Steam = $null }
)

function Get-CoverFolder { return Join-Path $script:Root 'Covers' }

function Get-SafeName([string]$file) { return ((Get-DisplayName $file) -replace '[\\/:*?"<>|]', '').Trim() }

# Une image déposée à la main ("Nom du jeu-grid.png") fonctionne comme une image téléchargée.
function Find-Art([string]$file, [string]$kind) {
    $dir = Get-CoverFolder
    if (-not (Test-Path -LiteralPath $dir)) { return $null }
    $base = "$(Get-SafeName $file)-$kind"
    foreach ($ext in '.png', '.jpg', '.jpeg', '.webp', '.ico') {
        $p = Join-Path $dir "$base$ext"
        if (Test-Path -LiteralPath $p) { return $p }
    }
    return $null
}

function Invoke-SteamGridDb([string]$key, [string]$path) {
    return (Invoke-RestMethod -Uri "https://www.steamgriddb.com/api/v2/$path" -Headers @{ Authorization = "Bearer $key"; 'User-Agent' = 'Switch2-Manager' } -UseBasicParsing -TimeoutSec 30).data
}

# Télécharge ce qui manque. Ne remplace jamais une image déjà présente.
function Get-GameArt($settings, [string[]]$files) {
    $key = Unprotect-Text $settings.steamGridKey
    if (-not $key) { throw 'Renseigne d''abord ta clé SteamGridDB dans Réglages.' }
    $dir = Get-CoverFolder
    New-Item -ItemType Directory -Force $dir | Out-Null
    $got = 0; $missing = @()
    foreach ($file in $files) {
        $name = Get-DisplayName $file
        $games = @(Invoke-SteamGridDb $key "search/autocomplete/$([uri]::EscapeDataString($name))")
        if (-not $games) { $missing += $name; continue }
        $id = $games[0].id
        foreach ($k in $script:ArtKinds) {
            if (Find-Art $file $k.Kind) { continue }
            $items = @(Invoke-SteamGridDb $key "$($k.Path)/game/$id$($k.Query)")
            if (-not $items) { continue }
            $url = [string]$items[0].url
            if ($url -notmatch '^https://') { continue }
            $ext = [IO.Path]::GetExtension(([uri]$url).AbsolutePath).ToLower()
            if (@('.png', '.jpg', '.jpeg', '.webp', '.ico') -notcontains $ext) { $ext = '.png' }
            Invoke-WebRequest -Uri $url -OutFile (Join-Path $dir "$(Get-SafeName $file)-$($k.Kind)$ext") -UseBasicParsing -TimeoutSec 120
            $got++
        }
    }
    $text = "$got image(s) téléchargée(s)."
    if ($missing) { $text += " Introuvable : $($missing -join ', ')." }
    return $text
}

# Copie les images vers Steam. Le nom du fichier dépend de l'identifiant du raccourci.
function Update-SteamArt([string]$steam, [string]$account) {
    $vdf = Get-SteamVdfPath $steam $account
    $grid = Join-Path (Split-Path $vdf) 'grid'
    $count = 0
    foreach ($e in (Read-Shortcuts $vdf)) {
        $game = Get-ManagedGame $e
        if (-not $game) { continue }
        $file = $game -replace '^Roms\\', ''
        $appid = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int](Get-Field $e 'appid')), 0)
        foreach ($k in $script:ArtKinds) {
            if ($null -eq $k.Steam) { continue }
            $art = Find-Art $file $k.Kind
            if (-not $art) { continue }
            New-Item -ItemType Directory -Force $grid | Out-Null
            Copy-Item -LiteralPath $art (Join-Path $grid "$appid$($k.Steam)$([IO.Path]::GetExtension($art))") -Force
            $count++
        }
    }
    return $count
}
