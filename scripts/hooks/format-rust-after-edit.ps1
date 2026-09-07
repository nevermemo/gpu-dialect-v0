# PostToolUse hook: format Rust after an agent edit. Exit 0 always; never blocks.
$ErrorActionPreference = 'Stop'
try {
    $raw = [Console]::In.ReadToEnd()
    if (-not $raw) { exit 0 }
    $event = $raw | ConvertFrom-Json
    if ($event.hook_event_name -ne 'PostToolUse') { exit 0 }
    $editTools = @('create_file', 'replace_string_in_file', 'multi_replace_string_in_file', 'insert_edit_into_file', 'edit_file', 'apply_patch')
    if ($editTools -notcontains $event.tool_name) { exit 0 }
    $paths = @()
    $toolInput = $event.tool_input
    if ($null -ne $toolInput) {
        if ($toolInput.PSObject.Properties['filePath']) { $paths += [string]$toolInput.filePath }
        if ($toolInput.PSObject.Properties['replacements']) {
            foreach ($r in @($toolInput.replacements)) { if ($r.PSObject.Properties['filePath']) { $paths += [string]$r.filePath } }
        }
    }
    if (-not ($paths | Where-Object { $_ -like '*.rs' })) { exit 0 }
    $root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
    $output = & cargo fmt --all --manifest-path (Join-Path $root 'Cargo.toml') 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        $message = 'cargo fmt --all failed after edit: ' + ($output.Trim() -replace '\s+', ' ')
        if ($message.Length -gt 400) { $message = $message.Substring(0, 400) + '...' }
        @{ systemMessage = $message } | ConvertTo-Json -Compress
    }
    exit 0
} catch {
    exit 0
}
