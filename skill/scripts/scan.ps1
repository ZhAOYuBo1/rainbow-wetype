param(
    [string]$Dir = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

try {
    $selected = Resolve-WeTypeDir $Dir
    $renderer = Join-Path $selected 'wetype_renderer.exe'
    $bytes = [IO.File]::ReadAllBytes($renderer)
    $sigs = Get-Signatures
    $matches = @(Find-Signature $bytes $sigs)

    $outMatches = @()
    $chosen = $null
    foreach ($m in $matches) {
        $color = $null
        if ($m.unique) {
            if ($chosen -eq $null) { $chosen = $m }
            try { $color = Get-ColorFromBytes $bytes $m.start $sigs.offsets } catch { }
        }
        $outMatches += [PSCustomObject]@{
            name = $m.name
            kind = $m.kind
            priority = $m.priority
            matchCount = $m.hits.Count
            start = $m.start
            currentColor = $color
        }
    }

    $sigName = if ($chosen) { $chosen.name } else { $null }
    $startVal = if ($chosen) { $chosen.start } else { $null }
    $colorVal = if ($chosen) { Get-ColorFromBytes $bytes $chosen.start $sigs.offsets } else { $null }
    $err = if ($chosen) { $null } else { 'No unique candidate-brush signature; read references/reverse-engineering.md before changing anything.' }

    [PSCustomObject]@{
        ok = [bool]($chosen -ne $null)
        action = 'scan'
        selected = $selected
        renderer = (Get-RendererInfo $renderer)
        matches = $outMatches
        signature = $sigName
        start = $startVal
        currentColor = $colorVal
        supported = [bool]($chosen -ne $null)
        error = $err
    } | ConvertTo-Json -Depth 8
} catch {
    [PSCustomObject]@{
        ok = $false
        action = 'scan'
        error = $_.Exception.Message
    } | ConvertTo-Json -Depth 8
    exit 1
}
