param(
    [string]$Dir = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

function Out-Json($obj) { $obj | ConvertTo-Json -Depth 8 }

try {
    if (-not (Test-IsAdmin)) {
        Out-Json ([PSCustomObject]@{
            ok = $false
            action = 'restore'
            error = 'administrator_required'
            hint = 'Run from an administrator PowerShell, or let the agent call this script elevated.'
        })
        exit 2
    }

    $selected = Resolve-WeTypeDir $Dir
    $tool = $script:ToolPath
    if (-not (Test-Path $tool)) { throw "Missing tool: $tool" }

    Write-SkillLog "restore $selected"
    $raw = & $tool restore --dir $selected 2>&1
    $code = $LASTEXITCODE
    $result = $raw | Out-String | ConvertFrom-Json

    Out-Json $result
    if ($code -ne 0 -or -not $result.ok) { exit 1 }
} catch {
    Out-Json ([PSCustomObject]@{
        ok = $false
        action = 'restore'
        error = $_.Exception.Message
    })
    exit 1
}
