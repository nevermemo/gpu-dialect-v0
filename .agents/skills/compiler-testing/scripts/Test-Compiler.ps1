#requires -Version 7.0
[CmdletBinding()]
param([string]$ProjectRoot = '.', [string]$Package, [string[]]$Features = @())
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
if (!(Test-Path -LiteralPath (Join-Path $root 'Cargo.toml'))) { throw 'ProjectRoot must contain Cargo.toml' }
if (!(Test-Path -LiteralPath (Join-Path $root 'Cargo.lock'))) { throw 'Cargo.lock required for reproducible --locked checks; generate intentionally first' }
if ($env:TRYBUILD -eq 'overwrite') { throw 'Unset TRYBUILD=overwrite before verification' }
Get-Command cargo -ErrorAction Stop | Out-Null
function Invoke-Cargo([string[]]$Arguments) {
    Write-Host ('cargo ' + ($Arguments -join ' '))
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo check failed ($LASTEXITCODE)" }
}
$scope = if ($Package) { @('-p', $Package) } else { @('--workspace') }
$featureArgs = if ($Features.Count) { @('--features', ($Features -join ',')) } else { @() }
Push-Location -LiteralPath $root
try {
    Invoke-Cargo @('fmt','--all','--','--check')
    Invoke-Cargo (@('clippy','--locked') + $scope + @('--all-targets') + $featureArgs + @('--','-D','warnings'))
    Invoke-Cargo (@('test','--locked') + $scope + @('--all-targets') + $featureArgs)
    Invoke-Cargo (@('test','--locked') + $scope + @('--doc') + $featureArgs)
    [pscustomobject]@{ stage='cargo'; status='PASS'; project=$root; note='Workspace checks only; inspect ignored tests and runtime evidence separately' }
} finally { Pop-Location }
