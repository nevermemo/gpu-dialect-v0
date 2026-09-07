#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Path,
    [Parameter(Mandatory)][string]$TargetEnv,
    [string]$Validator = 'spirv-val'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$file = (Resolve-Path -LiteralPath $Path).Path
$bytes = [IO.File]::ReadAllBytes($file)
if ($bytes.Length -lt 20 -or $bytes.Length % 4 -ne 0) { throw 'Invalid SPIR-V binary length' }
if ($bytes[0] -ne 3 -or $bytes[1] -ne 2 -or $bytes[2] -ne 35 -or $bytes[3] -ne 7) { throw 'Not a little-endian SPIR-V binary' }
Get-Command $Validator -ErrorAction Stop | Out-Null
$version = @(& $Validator --version 2>&1)
if ($LASTEXITCODE -ne 0) { throw 'Could not read spirv-val version' }
& $Validator --target-env $TargetEnv $file 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) { throw "spirv-val failed ($LASTEXITCODE) for $file" }
[pscustomobject]@{ stage='spirv-val'; status='PASS'; targetEnv=$TargetEnv; artifact=$file; sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash; version=($version -join "`n") }
