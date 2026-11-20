param(
    [string]$Dir = '',
    [string]$Expect = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

try {
    $selected = Resolve-WeTypeDir $Dir
    $renderer = Join-Path $selected 'wetype_renderer.exe'
    $bytes = [IO.File]::ReadAllBytes($renderer)
    $sigs = Get-Signatures
    $matches = @(Find-Signature $bytes $sigs)
    $chosen = $null
    foreach ($m in $matches) { if ($m.unique) { $chosen = $m; break } }

    $color = $null
    if ($chosen) { $color = Get-ColorFromBytes $bytes $chosen.start $sigs.offsets }
    $want = $null
    if ($Expect) {
        $want = $Expect.Trim()
        if (-not $want.StartsWith('#')) { $want = '#' + $want }
        $want = $want.ToUpperInvariant()
    }
    $matchesWant = $true
    if ($want) { $matchesWant = ($color -eq $want) }

    [PSCustomObject]@{
        ok = [bool]($chosen -ne $null)
        action = 'verify'
        selected = $selected
        renderer = (Get-RendererInfo $renderer)
        signature = $(if ($chosen) { $chosen.name } else { $null })
        currentColor = $color
        expectedColor = $want
        matchesExpected = $matchesWant
        error = $(if ($chosen) { $null } else { 'No unique signature; cannot verify color.' })
    } | ConvertTo-Json -Depth 8
} catch {
    [PSCustomObject]@{
        ok = $false
        action = 'verify'
        error = $_.Exception.Message
    } | ConvertTo-Json -Depth 8
    exit 1
}
