[CmdletBinding(DefaultParameterSetName = 'Mode')]
param(
    [Parameter(ParameterSetName = 'Mode')]
    [ValidateSet('Smoke', 'Fast', 'Gpu', 'Examples', 'Artifacts', 'Full')]
    [string]$Mode = 'Smoke',
    [Parameter(ParameterSetName = 'Full')]
    [switch]$Full
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Decode native command output as UTF-8 so captured diagnostics (e.g. the "µs"
# timings) are not mangled by the console code page before they reach the record.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskChecks = [System.Collections.Generic.List[object]]::new()
$taskArtifacts = [System.Collections.Generic.List[object]]::new()
$taskPassed = $false
$taskFailure = $null
$taskStarted = [DateTime]::UtcNow.ToString('o')
$taskMode = if ($Full) { 'Full' } else { $Mode }

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    # Capture stdout and stderr to separate temp files via Start-Process (which
    # redirects at the process level, avoiding PowerShell's error stream so a
    # failing command's stderr is recorded instead of becoming a terminating
    # error under $ErrorActionPreference='Stop'). A failing command's diagnostic
    # (cargo/clippy emit on stderr) is thus preserved in the JSON record.
    $outFile = [System.IO.Path]::GetTempFileName()
    $errFile = [System.IO.Path]::GetTempFileName()
    $taskExit = $null
    $taskStdout = ''
    $taskStderr = ''
    try {
        $proc = Start-Process -FilePath $Program -ArgumentList $Arguments -NoNewWindow -Wait -PassThru `
            -RedirectStandardOutput $outFile -RedirectStandardError $errFile
        $taskExit = $proc.ExitCode
        $taskStdout = [string](Get-Content -LiteralPath $outFile -Raw -ErrorAction SilentlyContinue)
        $taskStderr = [string](Get-Content -LiteralPath $errFile -Raw -ErrorAction SilentlyContinue)
    } finally {
        Remove-Item -LiteralPath $outFile, $errFile -ErrorAction SilentlyContinue
    }
    if ($taskStdout) { Write-Host $taskStdout }
    if ($taskStderr) { Write-Host $taskStderr }
    $taskChecks.Add([ordered]@{
        program = $Program
        arguments = $Arguments
        exit_code = $taskExit
        stdout = $taskStdout
        stderr = $taskStderr
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
    if ($taskMode -in @('Artifacts', 'Full')) { Get-Command spirv-val -ErrorAction Stop | Out-Null }
    Invoke-Checked rustc @('--version')
    Invoke-Checked slangc @('-version')
    Invoke-Checked pwsh @('-NoProfile', '-NonInteractive', '-File', (Join-Path $PSScriptRoot 'build-slang-reflect.ps1'))

    $examples = @('vector-add', 'polynomial', 'signal-pipeline', 'particle-step', 'typed-pipeline', 'staged-graph', 'component-pool')
    $smokeExamples = @('vector-add', 'typed-pipeline')

    if ($taskMode -in @('Smoke', 'Fast', 'Full')) {
        Invoke-Checked cargo @('fmt', '--all', '--', '--check')
    }
    if ($taskMode -in @('Smoke', 'Full')) {
        Invoke-Checked cargo @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
    }

    if ($taskMode -eq 'Fast') {
        Invoke-Checked cargo @('test', '-p', 'gpu-dialect-macros')
        Invoke-Checked cargo @('test', '-p', 'gpu-dialect', '--lib')
        Invoke-Checked cargo @('test', '-p', 'gpu-dialect-wgpu')
    }
    if ($taskMode -eq 'Gpu') {
        Invoke-Checked cargo @('test', '-p', 'gpu-dialect-wgpu')
    }
    if ($taskMode -in @('Smoke', 'Full')) {
        Invoke-Checked cargo @('test', '--workspace')
    }

    if ($taskMode -in @('Examples', 'Full')) {
        foreach ($example in $examples) {
            Invoke-Checked cargo @('test', '-p', $example, '--', '--ignored')
        }
    }
    $examplesToRun = if ($taskMode -eq 'Smoke') { $smokeExamples } elseif ($taskMode -in @('Examples', 'Artifacts', 'Full')) { $examples } else { @() }
    foreach ($example in $examplesToRun) {
        Invoke-Checked cargo @('run', '--quiet', '-p', $example)
    }

    if ($taskMode -in @('Artifacts', 'Full')) {
        Invoke-Checked spirv-val @('--version')
        $artifacts = @(Get-ChildItem -LiteralPath (Join-Path $taskRoot 'generated-wgpu') -Filter '*.spv' -File)
        if ($artifacts.Count -ne 12) { throw "Expected 12 exported kernels, found $($artifacts.Count); update this check deliberately for new examples." }
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
            mode = $taskMode.ToLowerInvariant()
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
