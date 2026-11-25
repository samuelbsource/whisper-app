# Run the app in dev mode (hot-reloading UI, debug Rust build).
#   .\scripts\dev.ps1                    # cuda if installed, else vulkan, else cpu
#   .\scripts\dev.ps1 -Backend vulkan
# Whisper is built optimised even in dev builds, so the first build takes ~10 minutes
# with CUDA; later runs take seconds.
param([ValidateSet('auto', 'cpu', 'vulkan', 'cuda')][string]$Backend = 'auto')

. "$PSScriptRoot\env.ps1"
Initialize-BuildEnv -Backend $Backend

Set-Location $script:RepoRoot
if (-not (Test-Path node_modules)) { npm install --no-fund --no-audit }

Write-Host "Starting Win Whisper dev build: $script:Backend"
npx tauri dev -- @script:CargoFeatures
