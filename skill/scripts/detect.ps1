param(
    [string]$Dir = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

try {
    $candidates = @(Get-WeTypeCandidates)
    $selected = $null
    if ($Dir) {
        $selected = Resolve-WeTypeDir $Dir
    } elseif ($candidates.Count -gt 0) {
        $selected = Resolve-WeTypeDir ''
    }

    $err = if ($selected) { $null } else { 'WeType installation not found.' }
    $renderer = $null
    $info = $null
    $backup = $null
    if ($selected) {
        $renderer = Join-Path $selected 'wetype_renderer.exe'
        $info = Get-RendererInfo $renderer
        $backup = Get-BackupPath $renderer
    }

    [PSCustomObject]@{
        ok = [bool]($selected -ne $null)
        action = 'detect'
        isAdmin = Test-IsAdmin
        selected = $selected
        candidates = $candidates
        renderer = $info
        service = Get-ServiceInfo
        backup = $backup
        backupExists = [bool]($backup -and (Test-Path $backup))
        error = $err
    } | ConvertTo-Json -Depth 8
} catch {
    [PSCustomObject]@{
        ok = $false
        action = 'detect'
        error = $_.Exception.Message
    } | ConvertTo-Json -Depth 8
    exit 1
}
