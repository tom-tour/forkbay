# Starts a game on its chosen emulator, with one shared Data folder and optional cloud sync.
#   Launcher.ps1                         -> menu
#   Launcher.ps1 -Game "Roms\game.nup"   -> emulator chosen in Switch 2 Manager
#   Launcher.ps1 -Emu "Folder" -Game ... -> forced emulator
#   Launcher.ps1 -Setup                  -> repair links and paths, start nothing
#   -Wait stays open until the emulator closes (Steam tracks the game this way)
param([string]$Emu, [string]$Game, [switch]$Setup, [switch]$Wait)

$ErrorActionPreference = 'Stop'
$script:Root = $PSScriptRoot
. (Join-Path $PSScriptRoot 'Manager\Core.ps1')

function Show-Message([string]$text, [string]$icon = 'Warning', [string]$buttons = 'OK') {
    Add-Type -AssemblyName System.Windows.Forms
    return [Windows.Forms.MessageBox]::Show($text, 'Switch 2 Manager', $buttons, $icon)
}

try {
    $data = Repair-Setup
    if ($Setup) { return }
    $settings = Get-Settings

    if (-not $Emu -and -not $Game) {
        $emulators = Get-EmulatorFolders
        if ($emulators.Count -eq 0) { throw 'Aucun émulateur installé : ouvre Switch 2 Manager.' }
        Write-Host ''
        for ($i = 0; $i -lt $emulators.Count; $i++) { Write-Host ("  {0}. {1}" -f ($i + 1), $emulators[$i].Name) }
        Write-Host ''
        $choice = 0
        if (-not [int]::TryParse((Read-Host 'Emulator'), [ref]$choice) -or $choice -lt 1 -or $choice -gt $emulators.Count) { throw 'Invalid choice.' }
        $Emu = $emulators[$choice - 1].Name
    }
    if (-not $Emu) { $Emu = Get-GameEmulator $settings (Split-Path $Game -Leaf) }

    $dir = Join-Path $script:Root $Emu
    $exe = Find-Exe $dir
    if (-not $exe) {
        if ((Show-Message "$Emu n'est pas installé. L'installer maintenant ?" 'Question' 'YesNo') -ne 'Yes') { return }
        [void](Install-Emulator $Emu)
        $exe = Find-Exe $dir
        if (-not $exe) { throw "Installation de $Emu impossible." }
    }

    $arguments = @()
    if ($Game) {
        $gamePath = Join-Path $script:Root $Game
        if (-not (Test-Path -LiteralPath $gamePath)) { throw "Jeu introuvable : $gamePath" }
        $arguments = @('-g', "`"$gamePath`"")
    }

    # Avant de jouer : remplace la version locale si le cloud est plus récent, demande en cas de conflit.
    $sync = [bool]$settings.sync.enabled -and -not (Test-EmulatorRunning)
    if ($sync) {
        try {
            $r = Invoke-CloudSync $settings { param($l, $d, $m) Show-ConflictDialog $l $d $m $true }
            if ($r -eq 'cancelled') { return }
        } catch {
            $ask = "Vérification du cloud impossible :`n$($_.Exception.Message)`n`nJouer quand même ? Les versions seront comparées au prochain échange."
            if ((Show-Message $ask 'Warning' 'YesNo') -ne 'Yes') { return }
        }
    }

    $start = @{ FilePath = $exe; WorkingDirectory = $dir; Wait = ([bool]$Wait -or $sync) }
    if ($arguments.Count) { $start.ArgumentList = $arguments }
    Start-Process @start

    # Après la partie : envoie les changements.
    if ($sync) {
        try { [void](Invoke-CloudSync (Get-Settings) { param($l, $d, $m) Show-ConflictDialog $l $d $m $false }) }
        catch { [void](Show-Message "Envoi vers le cloud impossible :`n$($_.Exception.Message)`n`nTa partie est bien gardée sur ce PC et sera envoyée au prochain échange.") }
    }
} catch {
    [void](Show-Message $_.Exception.Message)
    exit 1
}
