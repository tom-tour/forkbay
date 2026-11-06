# Release build without any path of this computer inside the program, and checks the result.
$repo = Split-Path $PSScriptRoot -Parent
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
# The last matching rule wins, so the most general one comes first.
$env:RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=/home --remap-path-prefix=$cargoHome=/cargo --remap-path-prefix=$rustupHome=/rustup --remap-path-prefix=$repo=/src"
Push-Location -LiteralPath $repo
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }

$exe = Join-Path $repo 'target\release\switch-2-manager.exe'
$text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes($exe))
$found = 0
foreach ($p in @([regex]::Escape($env:USERPROFILE), [regex]::Escape($repo), [regex]::Escape('\' + $env:USERNAME + '\'), '[A-Za-z]:\\Users\\', [regex]::Escape($env:COMPUTERNAME))) {
    $n = ([regex]::Matches($text, "(?i)$p")).Count
    if ($n) { "FOUND $n x $p"; $found += $n }
}
if ($found) { 'Personal paths are still inside the program.'; exit 1 }
'No personal path in the program.'
