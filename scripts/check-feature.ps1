[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('macro', 'core', 'wgpu', 'reflection', 'loops', 'examples', 'artifacts', 'full')]
    [string]$Area
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'Use PowerShell 7 (pwsh).' }
$root = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $root
try {
    switch ($Area) {
        'macro' { & cargo test -p gpu-dialect-macros }
        'core' { & cargo test -p gpu-dialect --lib }
        'wgpu' { & (Join-Path $PSScriptRoot 'verify.ps1') -Mode Gpu }
        'reflection' {
            & (Join-Path $PSScriptRoot 'build-slang-reflect.ps1')
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            & cargo test -p gpu-dialect --test reflection
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            & cargo test -p gpu-dialect-wgpu --test reflection
        }
        'loops' {
            & cargo test -p gpu-dialect-macros loops
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            & cargo test -p gpu-dialect-wgpu --test loops
        }
        'examples' { & (Join-Path $PSScriptRoot 'verify.ps1') -Mode Examples }
        'artifacts' { & (Join-Path $PSScriptRoot 'verify.ps1') -Mode Artifacts }
        'full' { & (Join-Path $PSScriptRoot 'verify.ps1') -Full }
    }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
