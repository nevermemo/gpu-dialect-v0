[CmdletBinding()]
param(
    [string]$SdkRoot = $env:SLANG_SDK,
    [string]$OutputDirectory = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target/slang-reflect'),
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'Use PowerShell 7 (pwsh).' }
if (-not $SdkRoot) { $SdkRoot = $env:VULKAN_SDK }
if (-not $SdkRoot) { throw 'Set SLANG_SDK (standalone Slang SDK) or VULKAN_SDK, or pass -SdkRoot.' }
$SdkRoot = (Resolve-Path -LiteralPath $SdkRoot).Path
$include = @('include/slang', 'include') | ForEach-Object { Join-Path $SdkRoot $_ } |
    Where-Object { Test-Path -LiteralPath (Join-Path $_ 'slang.h') } | Select-Object -First 1
if (-not $include) { throw "slang.h was not found under $SdkRoot/include[/slang]." }
$libraryDirectory = Join-Path $SdkRoot 'lib'
$binaryDirectory = Join-Path $SdkRoot 'bin'
$source = Join-Path $PSScriptRoot 'probes/slang-layout.cpp'
$null = New-Item -ItemType Directory -Path $OutputDirectory -Force
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path

if ($IsWindows) {
    if (-not (Get-Command cl -ErrorAction SilentlyContinue)) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Use an x64 MSVC developer shell or install the MSVC C++ build tools.' }
        $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'An x64 MSVC C++ toolchain is required.' }
        Import-Module (Join-Path $installation 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
        Enter-VsDevShell -VsInstallPath $installation -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64' | Out-Null
    }
    $library = @('slang-compiler.lib', 'slang.lib') | ForEach-Object { Join-Path $libraryDirectory $_ } |
        Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $library) { throw "No Slang import library in $libraryDirectory." }
    $dllName = [IO.Path]::GetFileNameWithoutExtension($library) + '.dll'
    $dll = Join-Path $binaryDirectory $dllName
    if (-not (Test-Path -LiteralPath $dll)) { throw "Missing matching compiler library: $dll" }
    $executable = Join-Path $OutputDirectory 'gust-slang-reflect.exe'
    if (-not $Force -and (Test-Path -LiteralPath $executable)) {
        $exeTime = (Get-Item -LiteralPath $executable).LastWriteTimeUtc
        $sourceTime = (Get-Item -LiteralPath $source).LastWriteTimeUtc
        $libraryTime = (Get-Item -LiteralPath $library).LastWriteTimeUtc
        $dllTime = (Get-Item -LiteralPath $dll).LastWriteTimeUtc
        if ($exeTime -ge $sourceTime -and $exeTime -ge $libraryTime -and $exeTime -ge $dllTime) {
            & $executable --version
            if ($LASTEXITCODE -ne 0) { throw "Native Slang reflection compiler could not load (exit $LASTEXITCODE)." }
            Write-Host "Reflection compiler: $executable (up to date)"
            return
        }
    }
    $arguments = @('/nologo', '/std:c++17', '/EHsc', '/W4', '/WX', "/I$include", $source,
        "/Fo$(Join-Path $OutputDirectory 'gust-slang-reflect.obj')", "/Fe$executable", '/link', $library)
    & cl @arguments
    if ($LASTEXITCODE -ne 0) { throw "Native Slang reflection build failed (exit $LASTEXITCODE)." }
    Copy-Item -LiteralPath $dll -Destination $OutputDirectory -Force
    foreach ($name in @('slang-glsl-module.dll', 'slang-glslang.dll', 'slang-rt.dll')) {
        $dependency = Join-Path $binaryDirectory $name
        if (Test-Path -LiteralPath $dependency) { Copy-Item -LiteralPath $dependency -Destination $OutputDirectory -Force }
    }
} else {
    $extension = if ($IsMacOS) { 'dylib' } else { 'so' }
    $library = @("libslang-compiler.$extension", "libslang.$extension") |
        ForEach-Object { Join-Path $libraryDirectory $_ } |
        Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $library) { throw "No Slang shared library in $libraryDirectory." }
    $compiler = if ($env:CXX) { $env:CXX } else { 'c++' }
    $executable = Join-Path $OutputDirectory 'gust-slang-reflect'
    if (-not $Force -and (Test-Path -LiteralPath $executable)) {
        $exeTime = (Get-Item -LiteralPath $executable).LastWriteTimeUtc
        $sourceTime = (Get-Item -LiteralPath $source).LastWriteTimeUtc
        $libraryTime = (Get-Item -LiteralPath $library).LastWriteTimeUtc
        if ($exeTime -ge $sourceTime -and $exeTime -ge $libraryTime) {
            & $executable --version
            if ($LASTEXITCODE -ne 0) { throw "Native Slang reflection compiler could not load (exit $LASTEXITCODE)." }
            Write-Host "Reflection compiler: $executable (up to date)"
            return
        }
    }
    & $compiler '-std=c++17' '-Wall' '-Wextra' '-Werror' "-I$include" $source $library "-Wl,-rpath,$libraryDirectory" '-o' $executable
    if ($LASTEXITCODE -ne 0) { throw "Native Slang reflection build failed (exit $LASTEXITCODE)." }
}
& $executable --version
if ($LASTEXITCODE -ne 0) { throw "Native Slang reflection compiler could not load (exit $LASTEXITCODE)." }
Write-Host "Reflection compiler: $executable"