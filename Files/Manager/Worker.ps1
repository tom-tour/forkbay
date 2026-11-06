# Exécute une tâche longue en arrière-plan pour ne pas figer la fenêtre.
param([string]$Action, [string]$Arg, [string]$Result)
$ErrorActionPreference = 'Stop'
$script:Root = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'Core.ps1')

function Save-WorkerResult($object) {
    [IO.File]::WriteAllText($Result, ($object | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
}

try {
    $message = ''; $rows = $null
    switch ($Action) {
        'status' { $rows = @(Get-EmulatorStatus) }
        'install' { $message = Install-Emulator $Arg }
        'prereq' { $message = Install-Prerequisites }
        'migrate' { $a = $Arg -split '\|', 2; $message = Move-DataLocation $a[0] $a[1] }
        'google' { $message = Connect-Google (Get-Settings) }
        'import' { $message = Import-ExistingInstall $Arg }
        'art' { $message = Get-GameArt (Get-Settings) @(Get-RomFiles) }
        'addgithub' { $a = $Arg | ConvertFrom-Json; $message = Add-GitHubEmulator $a.name $a.repo $a.asset '' }
        'addlocal' { $a = $Arg | ConvertFrom-Json; $message = Add-LocalEmulator $a.name $a.exe }
        'sync' {
            $r = Invoke-CloudSync (Get-Settings) { param($l, $d, $m) Show-ConflictDialog $l $d $m $true }
            $message = switch ($r) {
                'none' { 'Déjà synchronisé : les deux versions sont identiques.' }
                'uploaded' { 'Version locale envoyée dans le cloud.' }
                'downloaded' { 'Version distante installée. Une copie de la version locale est dans Save Backups.' }
                default { 'Synchronisation annulée : rien n''a changé.' }
            }
        }
        'steam' {
            $a = $Arg | ConvertFrom-Json
            $steam = Get-SteamPath
            if (-not $steam) { throw 'Steam introuvable sur ce PC.' }
            $restart = $false
            if (Get-Process steam -ErrorAction SilentlyContinue) {
                Start-Process -FilePath (Join-Path $steam 'steam.exe') -ArgumentList '-shutdown'
                $limit = (Get-Date).AddSeconds(45)
                while ((Get-Process steam -ErrorAction SilentlyContinue) -and (Get-Date) -lt $limit) { Start-Sleep -Milliseconds 500 }
                if (Get-Process steam -ErrorAction SilentlyContinue) { throw 'Steam ne s''est pas fermé : rien n''a été modifié.' }
                $restart = $true
            }
            try {
                Save-Shortcuts (Get-SteamVdfPath $steam $a.account) @($a.games)
                $images = Update-SteamArt $steam $a.account
                $message = "Enregistré : $(@($a.games).Count) jeu(x) dans Steam, $images image(s)."
            }
            finally { if ($restart) { Start-Process -FilePath (Join-Path $steam 'steam.exe') } }
        }
        default { throw "Tâche inconnue : $Action" }
    }
    Save-WorkerResult ([pscustomobject]@{ Ok = $true; Message = $message; Rows = $rows })
} catch {
    Save-WorkerResult ([pscustomobject]@{ Ok = $false; Message = $_.Exception.Message; Rows = $null })
}
