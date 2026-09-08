[CmdletBinding()]
param(
    [string[]]$Command = @(
        'cargo test -p gpu-dialect-macros',
        'cargo test -p gpu-dialect --lib',
        'cargo test -p gpu-dialect-wgpu',
        'cargo test --workspace'
    )
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'Use PowerShell 7 (pwsh).' }
$root = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $root
try {
    foreach ($line in $Command) {
        Write-Host "== $line"
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $parts = $line -split ' '
        $program = $parts[0]
        $arguments = if ($parts.Length -gt 1) { $parts[1..($parts.Length - 1)] } else { @() }
        & $program @arguments
        $exit = $LASTEXITCODE
        $timer.Stop()
        Write-Host ("elapsed_ms={0} exit={1}" -f [int]$timer.Elapsed.TotalMilliseconds, $exit)
        if ($exit -ne 0) { exit $exit }
    }
} finally {
    Pop-Location
}
