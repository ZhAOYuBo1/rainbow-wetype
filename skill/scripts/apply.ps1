param(
    [Parameter(Mandatory = $true)]
    [string]$Color,
    [string]$Dir = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

function Out-Json($obj) { $obj | ConvertTo-Json -Depth 8 }

try {
    $c = $Color.Trim()
    if (-not $c.StartsWith('#')) { $c = '#' + $c }
    if ($c -notmatch '^#[0-9a-fA-F]{6}$') { throw 'Color must be #RRGGBB.' }

    if (-not (Test-IsAdmin)) {
        Out-Json ([PSCustomObject]@{
            ok = $false
            action = 'apply'
            error = 'administrator_required'
            hint = 'Run from an administrator PowerShell, or let the agent call this script elevated.'
        })
        exit 2
    }

    $selected = Resolve-WeTypeDir $Dir
    $tool = $script:ToolPath
    if (-not (Test-Path $tool)) { throw "Missing tool: $tool" }
    $sig = Join-Path $script:SkillRoot 'signatures.json'

    Write-SkillLog "apply $c to $selected"
    $raw = & $tool apply --color $c --dir $selected --signatures $sig 2>&1
    $code = $LASTEXITCODE
    $result = $raw | Out-String | ConvertFrom-Json

    Out-Json $result
    if ($code -ne 0 -or -not $result.ok) { exit 1 }
} catch {
    Out-Json ([PSCustomObject]@{
        ok = $false
        action = 'apply'
        error = $_.Exception.Message
    })
    exit 1
}
