# Build the Windows installer (NSIS) for one backend and copy it to installers\.
#   .\scripts\release.ps1                    # cuda if installed, else vulkan, else cpu
#   .\scripts\release.ps1 -Backend vulkan
#   .\scripts\release.ps1 -Backend cpu
# The CUDA build bundles NVIDIA's cuBLAS DLLs so it runs on a PC without the CUDA Toolkit.
# Vulkan and CPU builds need no extra libraries (Vulkan comes with the GPU driver).
param([ValidateSet('auto', 'cpu', 'vulkan', 'cuda')][string]$Backend = 'auto')

. "$PSScriptRoot\env.ps1"
Initialize-BuildEnv -Backend $Backend

Set-Location $script:RepoRoot
if (-not (Test-Path node_modules)) { npm ci --no-fund --no-audit }

$bundleDir = Join-Path $script:RepoRoot 'src-tauri\bundle-libs'
$overrideFile = Join-Path $script:RepoRoot 'src-tauri\tauri.release.conf.json'

# Start from a clean folder so a CPU or Vulkan build never ships stale CUDA files.
if (Test-Path $bundleDir) { Remove-Item $bundleDir -Recurse -Force }
New-Item -ItemType Directory $bundleDir | Out-Null

if ($script:Backend -eq 'cuda') {
    $bin = Join-Path $env:CUDA_PATH 'bin\x64'
    # cublas is linked by the exe; cublasLt is loaded by cublas at runtime.
    foreach ($pattern in 'cublas64_*.dll', 'cublasLt64_*.dll') {
        $files = Get-ChildItem $bin -Filter $pattern
        if (-not $files) { throw "$pattern not found in $bin" }
        $files | Copy-Item -Destination $bundleDir
    }
    '{ "bundle": { "resources": { "bundle-libs/*.dll": "./" } } }' | Set-Content $overrideFile -Encoding utf8
    Write-Host "Building with CUDA, bundling: $((Get-ChildItem $bundleDir).Name -join ', ')"
} else {
    '{}' | Set-Content $overrideFile -Encoding utf8
    Write-Host "Building $script:Backend"
}

try {
    # Build the argument list as an array: PowerShell does not expand @splats that come after a literal "--",
    # and npx.ps1 would try to evaluate them as an expression. npx.cmd avoids that shim.
    $tauriArgs = @('tauri', 'build', '--config', $overrideFile)
    if ($script:CargoFeatures.Count -gt 0) { $tauriArgs += @('--') + $script:CargoFeatures }
    & npx.cmd @tauriArgs
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
} finally {
    Remove-Item $overrideFile -ErrorAction SilentlyContinue
}

# Tauri always names the installer the same; add the backend so the three builds can sit side by side.
$version = (Get-Content (Join-Path $script:RepoRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
# CARGO_TARGET_DIR relocates the build output (CI uses a short path for the Vulkan build).
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $script:RepoRoot 'src-tauri\target' }
$built = Get-ChildItem (Join-Path $targetDir 'release\bundle\nsis') -Filter *-setup.exe |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $built) { throw 'No installer was produced.' }
$outDir = Join-Path $script:RepoRoot 'installers'
New-Item -ItemType Directory -Force $outDir | Out-Null
$target = Join-Path $outDir "WinWhisper_${version}_x64-$($script:Backend)-setup.exe"
Copy-Item $built.FullName $target -Force
Write-Host ("`nInstaller: {0} ({1} MB)" -f $target, [int]((Get-Item $target).Length / 1MB))
