[CmdletBinding()]
param([switch]$Full)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskChecks = [System.Collections.Generic.List[object]]::new()
$taskArtifacts = [System.Collections.Generic.List[object]]::new()
$taskPassed = $false
$taskFailure = $null
$taskStarted = [DateTime]::UtcNow.ToString('o')

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    $taskOutput = @(& $Program @Arguments)
    $taskExit = $LASTEXITCODE
    foreach ($line in $taskOutput) { Write-Host $line }
    $taskChecks.Add([ordered]@{
        program = $Program
        arguments = $Arguments
        exit_code = $taskExit
        stdout = ($taskOutput -join "`n")
    })
    if ($taskExit -ne 0) {
        throw "$Program $($Arguments -join ' ') failed (exit $taskExit)"
    }
}

Push-Location -LiteralPath $taskRoot
try {
    foreach ($program in @('cargo', 'rustc', 'slangc')) {
        Get-Command $program -ErrorAction Stop | Out-Null
    }
    if ($Full) { Get-Command spirv-val -ErrorAction Stop | Out-Null }
    Invoke-Checked rustc @('--version')
    Invoke-Checked slangc @('-version')
    Invoke-Checked cargo @('fmt', '--all', '--', '--check')
    Invoke-Checked cargo @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
    Invoke-Checked cargo @('test', '--workspace')
    $examples = if ($Full) {
        @('vector-add', 'polynomial', 'signal-pipeline', 'particle-step', 'typed-pipeline')
    } else { @('vector-add', 'typed-pipeline') }
    foreach ($example in $examples) {
        Invoke-Checked cargo @('run', '--quiet', '-p', $example)
    }
    if ($Full) {
        Invoke-Checked spirv-val @('--version')
        $artifacts = @(Get-ChildItem -LiteralPath (Join-Path $taskRoot 'generated-wgpu') -Filter '*.spv' -File)
        if ($artifacts.Count -ne 8) { throw "Expected 8 exported kernels, found $($artifacts.Count); update this check deliberately for new examples." }
        foreach ($artifact in $artifacts) {
            Invoke-Checked spirv-val @('--target-env', 'vulkan1.2', $artifact.FullName)
            $taskArtifacts.Add([ordered]@{
                file = $artifact.Name
                sha256 = (Get-FileHash -LiteralPath $artifact.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                validation_target = 'vulkan1.2'
                status = 'passed'
            })
            Write-Host "Validated $($artifact.Name)"
        }
    }
    $taskPassed = $true
    Write-Host 'GUST verification passed.'
} catch {
    $taskFailure = $_.Exception.Message
    throw
} finally {
    try {
        $report = [ordered]@{
            schema_version = 1
            started_utc = $taskStarted
            finished_utc = [DateTime]::UtcNow.ToString('o')
            mode = $(if ($Full) { 'full' } else { 'smoke' })
            passed = $taskPassed
            failure = $taskFailure
            scope = 'Debug native Vulkan wgpu execution of Slang-generated WGSL; SPIR-V export validation is separate.'
            untested = @('browser WebGPU', 'Metal', 'DXIL', 'other GPU vendors', 'declared minimum Rust version')
            checks = @($taskChecks.ToArray())
            spirv_artifacts = @($taskArtifacts.ToArray())
        }
        $reportDirectory = Join-Path $taskRoot '.ai'
        New-Item -ItemType Directory -Path $reportDirectory -Force | Out-Null
        $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $reportDirectory 'VALIDATION.json') -Encoding utf8
    } finally { Pop-Location }
}
