#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$ProjectRoot = '.',
    [Parameter(Mandatory)][string]$SourcePath,
    [Parameter(Mandatory)][string]$EmitScript,
    [Parameter(Mandatory)][string]$RuntimeScript,
    [Parameter(Mandatory)][string]$EntryPoint,
    [ValidateSet('wgsl','spirv')][string[]]$Targets = @('wgsl'),
    [ValidateSet('wgsl','spirv')][string]$RuntimeTarget = 'wgsl',
    [string]$TargetEnv,
    [string[]]$SlangArgs = @(),
    [switch]$RequireHardware
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
function Resolve-ProjectFile([string]$Path) {
    $candidate = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $root $Path }
    $resolved = (Resolve-Path -LiteralPath $candidate).Path
    if (!(Test-Path -LiteralPath $resolved -PathType Leaf)) { throw "Expected file: $resolved" }
    return $resolved
}
$source = Resolve-ProjectFile $SourcePath
$emit = Resolve-ProjectFile $EmitScript
$runtime = Resolve-ProjectFile $RuntimeScript
if ([IO.Path]::GetExtension($emit) -ne '.ps1' -or [IO.Path]::GetExtension($runtime) -ne '.ps1') { throw 'Adapters must be PowerShell .ps1 files' }
if ($RuntimeTarget -notin $Targets) { throw 'RuntimeTarget must be included in Targets' }
if ('spirv' -in $Targets -and !$TargetEnv) { throw 'SPIR-V requires explicit TargetEnv' }
$skills = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out = Join-Path $root ('target/skill-verification/vertical-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($out) | Out-Null
$report = [ordered]@{ schemaVersion=1; status='FAIL'; source=$source; sourceSha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash; runtimeTarget=$RuntimeTarget; stages=@(); evidence=$out }
$psExe = (Get-Process -Id $PID).Path
function Run_Adapter([string]$Script, [string[]]$Arguments, [string]$Log) {
    & $psExe -NoProfile -NonInteractive -File $Script @Arguments 2>&1 | Tee-Object -FilePath $Log | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Adapter failed ($LASTEXITCODE): $Script" }
}
Push-Location -LiteralPath $root
try {
    $slang = Join-Path $out 'kernel.slang'
    Run_Adapter $emit @('-ProjectRoot',$root,'-SourcePath',$source,'-OutputPath',$slang) (Join-Path $out 'emit.log')
    if (!(Test-Path -LiteralPath $slang -PathType Leaf) -or (Get-Item -LiteralPath $slang).Length -eq 0) { throw 'Emit adapter produced no fresh nonempty Slang file' }
    $report.stages += [pscustomobject]@{ stage='rust-to-slang'; status='PASS'; sha256=(Get-FileHash -LiteralPath $slang -Algorithm SHA256).Hash }
    $artifacts = @{}
    foreach ($target in ($Targets | Select-Object -Unique)) {
        $ext = if ($target -eq 'spirv') { 'spv' } else { 'wgsl' }
        $file = Join-Path $out "kernel.$ext"
        $report.stages += & (Join-Path $skills 'slang-language/scripts/Compile-Slang.ps1') -SourcePath $slang -EntryPoint $EntryPoint -Target $target -OutputPath $file -ExtraArgs $SlangArgs
        if ($target -eq 'spirv') {
            $report.stages += & (Join-Path $skills 'spirv-validation/scripts/Test-Spirv.ps1') -Path $file -TargetEnv $TargetEnv
        }
        $artifacts[$target] = $file
    }
    $shader = $artifacts[$RuntimeTarget]
    $expectedHash = (Get-FileHash -LiteralPath $shader -Algorithm SHA256).Hash
    $resultPath = Join-Path $out 'runtime.json'
    Run_Adapter $runtime @('-ProjectRoot',$root,'-ShaderPath',$shader,'-ResultPath',$resultPath) (Join-Path $out 'runtime.log')
    $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($result.schemaVersion -ne 1 -or $result.status -cne 'PASS' -or $result.execution -cne 'wgpu') { throw 'Runtime did not report successful wgpu execution' }
    if ($result.shaderSha256 -ne $expectedHash -or (Get-FileHash -LiteralPath $shader -Algorithm SHA256).Hash -ne $expectedHash) { throw 'Runtime shader hash mismatch or shader modified during execution' }
    foreach ($field in @('adapter','backend','deviceType')) {
        if ([string]::IsNullOrWhiteSpace($result.$field)) { throw "Missing runtime $field" }
    }
    if ($result.hardware -isnot [bool]) { throw 'hardware must be a JSON boolean' }
    if ($RequireHardware -and !$result.hardware) { throw 'Hardware GPU execution required; software adapter reported' }
    $cases = @($result.cases)
    if ($cases.Count -eq 0) { throw 'No runtime comparison cases' }
    foreach ($case in $cases) {
        if (($case.n -isnot [long] -and $case.n -isnot [int]) -or ($case.compared -isnot [long] -and $case.compared -isnot [int])) { throw 'Runtime case counts must be JSON integers' }
        if ([string]::IsNullOrWhiteSpace($case.name) -or $case.passed -isnot [bool] -or !$case.passed -or $case.n -lt 0 -or $case.compared -ne $case.n) { throw 'Incomplete or failed runtime comparison' }
    }
    if (@($cases | Where-Object { $_.n -gt 1 }).Count -eq 0) { throw 'Need a nontrivial multi-element runtime comparison' }
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $report.sourceSha256) { throw 'Rust source changed during verification' }
    $report.stages += [pscustomobject]@{ stage='wgpu-readback'; status='PASS'; target=$RuntimeTarget; result=$result }
    $report.status = 'PASS'
} catch {
    $report['error'] = $_.Exception.Message
    throw
} finally {
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Pop-Location
    Write-Host "Vertical verification evidence: $out"
}
[pscustomobject]$report
