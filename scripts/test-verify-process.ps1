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

$shellPath = (Get-Command pwsh -CommandType Application).Source
$childCommand = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('Start-Sleep -Seconds 20'))
$escapedShellPath = $shellPath.Replace("'", "''")
$parentCommand = @"
`$child = Start-Process -FilePath '$escapedShellPath' -ArgumentList '-NoProfile', '-EncodedCommand', '$childCommand' -WindowStyle Hidden -PassThru
[Console]::Out.WriteLine('owned-child=' + `$child.Id)
[Console]::Out.WriteLine('stdout preserved')
[Console]::Error.WriteLine('stderr preserved')
exit 7
"@
$encodedParent = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($parentCommand))
$ownedChild = $null
$timer = [Diagnostics.Stopwatch]::StartNew()
try {
    $failure = $null
    try {
        Invoke-Checked $shellPath @('-NoProfile', '-EncodedCommand', $encodedParent)
    } catch {
        $failure = $_.Exception.Message
    }
    $timer.Stop()
    if ($taskChecks.Count -ne 1) { throw 'Expected exactly one captured command record' }
    $record = $taskChecks[0]
    if ($record.stdout -match 'owned-child=(\d+)') {
        $ownedChild = Get-Process -Id ([int]$Matches[1]) -ErrorAction SilentlyContinue
    }
    if ($record.exit_code -ne 7 -or $failure -notlike '*failed (exit 7)*') {
        throw "Nonzero exit was not preserved: $failure"
    }
    if ($record.stdout -notlike '*stdout preserved*' -or $record.stderr -notlike '*stderr preserved*') {
        throw 'A captured diagnostic stream was lost'
    }
    if ($timer.Elapsed.TotalSeconds -ge 15 -or $null -eq $ownedChild -or $ownedChild.HasExited) {
        throw "Waited for the descendant instead of the launched process ($($timer.Elapsed.TotalSeconds) seconds)"
    }
    Write-Host 'PASS: direct-process wait, surviving descendant, exit code, and both diagnostic streams.'
} finally {
    # Only terminate the short-lived process created and identified by this test.
    if ($null -ne $ownedChild) {
        if (-not $ownedChild.HasExited) { $ownedChild.Kill(); $ownedChild.WaitForExit() }
        $ownedChild.Dispose()
    }
}
