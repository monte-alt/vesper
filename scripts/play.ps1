# Starts the title with the settings this fork is played with (README.md, "How to run it").
#   scripts\play.ps1 -Exe <path of your own copy's executable>
# -Safe         without the newer, faster ways (the first thing to try when something is wrong)
# -RayTracing   the title's own ray-traced indirect light: lit interiors, about a third slower
# -Fsr4K        PC FidelityFX at 3840x2160 (quality, balanced or performance): FSR 4.1.2 where AMD's SDK offers it for the card, FSR 3.1 otherwise
# -Build        target\diag (the default, the build all measurements are of) or target\release
#               Light DRED is requested by default for diag and requires --features diagnostics.
# -More         other settings: -More @{VESPER_PANEL='1'}
# Run it from a normal window, not as administrator: the driver then keeps its shader cache elsewhere.
param(
    [Parameter(Mandatory)][string]$Exe,
    [switch]$Safe,
    [switch]$RayTracing,
    [ValidateSet('quality', 'balanced', 'performance')][string]$Fsr4K,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [hashtable]$More = @{}
)
$settings = @{
    VESPER_TEXTURE_NO_WAIT = '1'; VESPER_RELAY = '2'; VESPER_PARALLEL_READBACK = '1'; VESPER_CARD_REHASH_RUNS = '1'
    VESPER_JOIN_COPIES = '1'; VESPER_INDIRECT_WINDOW = '1'; VESPER_RELAY_COPY = '1'; VESPER_CARD_STEPS = '2'
    VESPER_INDIRECT_BATCH = '2'; VESPER_MAP_THREADS = '1'; VESPER_TITLE_PRIORITIES = '0'; VESPER_GPU_KEEP_AWAKE = '1'
    VESPER_ASYNC_PRESENT = '1'; VESPER_AMBIENT = '0.8'
}
if ($Build -eq 'diag') { $settings.VESPER_D3D12_DRED = 'light' }
if ($Safe) {
    $settings.VESPER_UNDER_WIDE = '0'; $settings.VESPER_LAND_ECHO = '0'
} else {
    $settings.VESPER_PARALLEL_LISTS = '1'; $settings.VESPER_JOIN_PARTS = '8'; $settings.VESPER_UPLOAD_AHEAD = '1'
}
if ($RayTracing) { $settings.VESPER_BVH_BUILDER = '1' }
if ($Fsr4K) { $settings.VESPER_FSR_4K = $Fsr4K.ToLowerInvariant() }
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }

$vesper = Join-Path $PSScriptRoot "..\target\$Build\vesper.exe"
if (-not (Test-Path $vesper)) { Write-Host "No $vesper. Build it first (README.md)."; exit 1 }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; exit 1 }
# (Other settings left over in this window go; where your files are stays.)
$paths = 'VESPER_CONFIG', 'VESPER_GAME_DIR', 'VESPER_CACHE_DIR', 'VESPER_DATA_DIR', 'VESPER_FIDELITYFX_DIR'
$inherited = @(Get-ChildItem Env:VESPER_* | Where-Object { $_.Name -notin $paths })
foreach ($entry in $inherited) { Remove-Item "Env:$($entry.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$discarded = $inherited | Where-Object {
    $_.Name -notin $More.Keys -and -not $_.Value.Equals([Environment]::GetEnvironmentVariable($_.Name))
} | Sort-Object Name
if ($discarded) {
    Write-Host "Discarded inherited values for $($discarded.Name -join ', '). Use -More to pass them explicitly."
}
$Exe = (Resolve-Path $Exe).Path
Push-Location (Join-Path $PSScriptRoot '..')   # vesper.toml is looked for here
try { & $vesper run $Exe --execute } finally { Pop-Location }
