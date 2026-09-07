#requires -Version 7.0
[CmdletBinding()]
param([string]$ProjectRoot = '.', [string]$TargetEnv = 'vulkan1.2')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
$skills = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out = Join-Path $root ('target/skill-verification/toolchain-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($out) | Out-Null
$results = @()
foreach ($tool in @(@('slangc','-version'), @('spirv-val','--version'))) {
    Get-Command $tool[0] -ErrorAction Stop | Out-Null
    $version = @(& $tool[0] $tool[1] 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "Could not read version of $($tool[0])" }
    $version | Set-Content -LiteralPath (Join-Path $out ($tool[0] + '-version.txt'))
}
foreach ($target in @('wgsl','spirv')) {
    $ext = if ($target -eq 'spirv') { 'spv' } else { 'wgsl' }
    $results += & (Join-Path $skills 'slang-language/scripts/Compile-Slang.ps1') -SourcePath (Join-Path $skills 'slang-language/assets/vector-add.slang') -EntryPoint computeMain -Target $target -OutputPath (Join-Path $out "vector-add.$ext")
}
$results += & (Join-Path $skills 'spirv-validation/scripts/Test-Spirv.ps1') -Path (Join-Path $out 'vector-add.spv') -TargetEnv $TargetEnv
$report = [pscustomobject]@{ status='PASS'; scope='Hand-authored Slang compile and SPIR-V validation only; Rust frontend and GPU execution NOT RUN'; results=$results }
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $out 'report.json')
Write-Host "Toolchain smoke PASS. Rust frontend and GPU execution NOT RUN. Evidence: $out"
$report
