[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'Use PowerShell 7 (pwsh).' }
$root = Split-Path -Parent $PSScriptRoot
$examples = @('vector-add', 'polynomial', 'signal-pipeline', 'particle-step', 'typed-pipeline', 'staged-graph', 'component-pool')
Push-Location -LiteralPath $root
try {
    foreach ($example in $examples) {
        Write-Host "Exporting artifacts via $example"
        & cargo run --quiet -p $example
        if ($LASTEXITCODE -ne 0) { throw "cargo run -p $example failed (exit $LASTEXITCODE)" }
    }
} finally {
    Pop-Location
}
