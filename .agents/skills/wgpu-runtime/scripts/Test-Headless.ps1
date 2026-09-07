#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$ProjectRoot = '.',
    [Parameter(Mandatory)][string]$Package,
    [Parameter(Mandatory)][string]$TestTarget,
    [Parameter(Mandatory)][string]$TestName,
    [string[]]$Features = @()
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
if (!(Test-Path -LiteralPath (Join-Path $root 'Cargo.lock'))) { throw 'Cargo.lock required' }
if ($env:TRYBUILD -eq 'overwrite') { throw 'Unset TRYBUILD=overwrite before verification' }
Get-Command cargo -ErrorAction Stop | Out-Null
$baseArgs = @('test','--locked','-p',$Package,'--test',$TestTarget)
if ($Features.Count) { $baseArgs += @('--features',($Features -join ',')) }
Push-Location -LiteralPath $root
try {
    $listing = @(& cargo @baseArgs -- --list 2>&1)
    if ($LASTEXITCODE -ne 0) { $listing | Out-Host; throw 'Could not list integration tests' }
    if (@($listing | Where-Object { "$_" -eq "${TestName}: test" }).Count -ne 1) { throw "Exact test not found once: $TestName" }
    $result = @(& cargo @baseArgs -- $TestName --exact --include-ignored --nocapture 2>&1)
    $code = $LASTEXITCODE
    $result | Out-Host
    if ($code -ne 0) { throw "Headless test failed ($code)" }
    if (($result -join "`n") -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;') { throw 'Expected one executed standard libtest test, not zero or ignored tests' }
    [pscustomobject]@{ stage='headless-test'; status='PASS'; test=$TestName; note='Test executed; inspect its assertions for actual GPU evidence' }
} finally { Pop-Location }
