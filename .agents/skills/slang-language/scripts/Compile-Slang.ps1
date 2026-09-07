#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SourcePath,
    [Parameter(Mandatory)][string]$EntryPoint,
    [Parameter(Mandatory)][ValidateSet('wgsl','spirv')][string]$Target,
    [Parameter(Mandatory)][string]$OutputPath,
    [string]$Slangc = 'slangc',
    [string[]]$ExtraArgs = @()
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = (Resolve-Path -LiteralPath $SourcePath).Path
$output = [IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $output) { throw "Output already exists; use a fresh path: $output" }
Get-Command $Slangc -ErrorAction Stop | Out-Null
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($output)) | Out-Null
$log = "$output.compile.log"
$argsList = @($source, '-entry', $EntryPoint, '-stage', 'compute', '-target', $Target, '-o', $output) + $ExtraArgs
$version = @(& $Slangc -version 2>&1)
if ($LASTEXITCODE -ne 0) { throw 'Could not read slangc version' }
@{ executable=$Slangc; version=($version -join "`n"); arguments=$argsList } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath "$output.command.json"
& $Slangc @argsList 2>&1 | Tee-Object -FilePath $log | Out-Host
if ($LASTEXITCODE -ne 0) { throw "slangc failed ($LASTEXITCODE); see $log" }
if (!(Test-Path -LiteralPath $output -PathType Leaf) -or (Get-Item -LiteralPath $output).Length -eq 0) { throw 'slangc produced no nonempty artifact' }
[pscustomobject]@{ stage='slangc'; status='PASS'; target=$Target; artifact=$output; sha256=(Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash; log=$log }
