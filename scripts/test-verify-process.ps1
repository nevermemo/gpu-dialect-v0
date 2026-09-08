[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Load only the checked-command function, without running the verification suite
# or overwriting its durable JSON report.
$tokens = $null
$parseErrors = $null
$scriptAst = [System.Management.Automation.Language.Parser]::ParseFile(
    (Join-Path $PSScriptRoot 'verify.ps1'), [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw "verify.ps1 has parse errors: $parseErrors" }
$functionAst = $scriptAst.Find({
    param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
        $node.Name -eq 'Invoke-Checked'
}, $false)
if ($null -eq $functionAst) { throw 'Invoke-Checked was not found' }
. ([scriptblock]::Create($functionAst.Extent.Text))
$taskChecks = [System.Collections.Generic.List[object]]::new()
$taskArtifacts = [System.Collections.Generic.List[object]]::new()
$taskPassed = $false
$taskFailure = $null
$taskStarted = [DateTime]::UtcNow.ToString('o')
$taskMode = 'test-harness'

$shellPath = 'pwsh'
$parentCommand = @"
[Console]::Out.WriteLine('stdout preserved')
[Console]::Error.WriteLine('stderr preserved')
exit 7
"@
$encodedParent = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($parentCommand))
try {
    $failure = $null
    try {
        Invoke-Checked $shellPath @('-NoProfile', '-EncodedCommand', $encodedParent)
    } catch {
        $failure = $_.Exception.Message
    }
    if ($taskChecks.Count -ne 1) { throw "Expected exactly one captured command record; saw $($taskChecks.Count); failure=$failure" }
    $record = $taskChecks[0]
    if ($record.exit_code -ne 7 -or $failure -notlike '*failed (exit 7)*') {
        throw "Nonzero exit was not preserved: $failure"
    }
    if ($record.stdout -notlike '*stdout preserved*' -or $record.stderr -notlike '*stderr preserved*') {
        throw 'A captured diagnostic stream was lost'
    }
    Write-Host 'PASS: verify.ps1 parses; Invoke-Checked preserves exit code and both diagnostic streams.'
} finally {
}
