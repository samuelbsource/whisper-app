# Shared build environment. Dot-source it, then call Initialize-BuildEnv:
#   . "$PSScriptRoot\env.ps1"
#   Initialize-BuildEnv -Backend auto|cpu|vulkan|cuda
#
# Finds CMake and LLVM (libclang) for building whisper.cpp, then the GPU toolkit for the chosen backend:
#   cuda    CUDA Toolkit (CUDA_PATH, else the newest one under Program Files)
#   vulkan  Vulkan SDK   (VULKAN_SDK, else the newest one under C:\VulkanSDK)
#   cpu     nothing extra
#   auto    cuda if the CUDA Toolkit is installed, else vulkan if the SDK is, else cpu
# Sets $script:Backend (resolved), $script:CargoFeatures and $script:RepoRoot for the caller.

$ErrorActionPreference = 'Stop'
$script:RepoRoot = Split-Path -Parent $PSScriptRoot

function Find-FirstDir([string[]]$Candidates) {
    foreach ($c in $Candidates) { if ($c -and (Test-Path $c)) { return $c } }
    return $null
}

function Find-CudaDir {
    if ($env:CUDA_PATH -and (Test-Path $env:CUDA_PATH)) { return $env:CUDA_PATH }
    $root = "$env:ProgramFiles\NVIDIA GPU Computing Toolkit\CUDA"
    if (-not (Test-Path $root)) { return $null }
    return Get-ChildItem $root -Directory | Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
}

function Find-VulkanDir {
    if ($env:VULKAN_SDK -and (Test-Path $env:VULKAN_SDK)) { return $env:VULKAN_SDK }
    if (-not (Test-Path 'C:\VulkanSDK')) { return $null }
    return Get-ChildItem 'C:\VulkanSDK' -Directory | Sort-Object Name -Descending | Select-Object -First 1 -ExpandProperty FullName
}

function Initialize-BuildEnv([ValidateSet('auto', 'cpu', 'vulkan', 'cuda')][string]$Backend = 'auto') {
    # CMake and LLVM: add the usual install folders when present; otherwise they must already be on PATH.
    $cmake = Find-FirstDir @("$env:ProgramFiles\CMake\bin")
    if ($cmake) { $env:Path = "$cmake;$env:Path" }
    elseif (-not (Get-Command cmake -ErrorAction SilentlyContinue)) { throw 'CMake not found. Install it: winget install Kitware.CMake' }

    if (-not $env:LIBCLANG_PATH) {
        $llvm = Find-FirstDir @("$env:ProgramFiles\LLVM\bin")
        if (-not $llvm) { throw 'LLVM (libclang) not found. Install it: winget install LLVM.LLVM, or set LIBCLANG_PATH.' }
        $env:LIBCLANG_PATH = $llvm
    }

    $cuda = Find-CudaDir
    $vulkan = Find-VulkanDir
    if ($Backend -eq 'auto') {
        $Backend = if ($cuda) { 'cuda' } elseif ($vulkan) { 'vulkan' } else { 'cpu' }
    }

    switch ($Backend) {
        'cuda' {
            if (-not $cuda) { throw 'CUDA Toolkit not found. Install it (winget install Nvidia.CUDA) or pick -Backend vulkan / cpu.' }
            $env:CUDA_PATH = $cuda
            $env:CudaToolkitDir = "$cuda\"      # MSBuild's CUDA targets need this
            $env:CUDAToolkit_ROOT = $cuda
            # 75 = RTX 20 / GTX 16, 86 = RTX 30, 89 = RTX 40, 120 = RTX 50. Override with WW_CUDA_ARCH, e.g. "75".
            if (-not $env:CMAKE_CUDA_ARCHITECTURES) {
                $env:CMAKE_CUDA_ARCHITECTURES = if ($env:WW_CUDA_ARCH) { $env:WW_CUDA_ARCH } else { '75' }
            }
            $env:Path = "$cuda\bin\x64;$cuda\bin;$env:Path"
            $script:CargoFeatures = @('--features', 'cuda')
        }
        'vulkan' {
            if (-not $vulkan) { throw 'Vulkan SDK not found. Install it (winget install KhronosGroup.VulkanSDK) or pick -Backend cpu.' }
            $env:VULKAN_SDK = $vulkan
            $script:CargoFeatures = @('--features', 'vulkan')
        }
        default { $script:CargoFeatures = @() }
    }
    $script:Backend = $Backend
}
