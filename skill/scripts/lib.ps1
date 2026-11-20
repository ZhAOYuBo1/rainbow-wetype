# Shared helpers for rainbow-wetype.
# Requires Windows PowerShell 5.1+. Outputs objects; callers decide JSON formatting.

$script:SkillRoot = Split-Path -Parent $PSScriptRoot
$script:SignaturesPath = Join-Path $script:SkillRoot 'signatures.json'
$script:ToolPath = Join-Path $script:SkillRoot 'tools\wetype-color.exe'
$script:BackupRoot = Join-Path $env:ProgramData 'WeTypeColor\backup'
$script:LogPath = Join-Path $env:ProgramData 'WeTypeColor\skill-log.txt'

if (-not ('WtSig' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;

public static class WtSig {
    public static int[] Find(byte[] data, byte[] pattern, bool[] wildcard) {
        if (data == null || pattern == null || wildcard == null) return new int[0];
        if (pattern.Length == 0 || data.Length < pattern.Length) return new int[0];
        var hits = new List<int>();
        int limit = data.Length - pattern.Length;
        for (int i = 0; i <= limit; i++) {
            bool ok = true;
            for (int j = 0; j < pattern.Length; j++) {
                if (!wildcard[j] && data[i + j] != pattern[j]) { ok = false; break; }
            }
            if (ok) hits.Add(i);
        }
        return hits.ToArray();
    }
}
'@
}

function Write-SkillLog([string]$Message) {
    try {
        $dir = Split-Path -Parent $script:LogPath
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
        Add-Content -LiteralPath $script:LogPath -Value ((Get-Date -Format 'yyyy-MM-dd HH:mm:ss') + ' ' + $Message) -Encoding UTF8
    } catch { }
}

function Test-IsAdmin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $p = New-Object Security.Principal.WindowsPrincipal($id)
    return $p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-RendererPath([string]$Dir) {
    if ([string]::IsNullOrWhiteSpace($Dir)) { return $null }
    $p = Join-Path $Dir 'wetype_renderer.exe'
    if (Test-Path $p) { return $p }
    return $null
}

function Add-DirCandidate($List, [string]$Dir) {
    if ([string]::IsNullOrWhiteSpace($Dir)) { return }
    $Dir = $Dir.Trim().Trim('"')
    if (-not (Test-Path $Dir)) { return }
    if (Test-Path (Join-Path $Dir 'wetype_renderer.exe')) {
        if (-not $List.Contains($Dir)) { $List.Add($Dir) | Out-Null }
        return
    }
    try {
        foreach ($sub in Get-ChildItem -LiteralPath $Dir -Directory -ErrorAction SilentlyContinue) {
            if (Test-Path (Join-Path $sub.FullName 'wetype_renderer.exe')) {
                if (-not $List.Contains($sub.FullName)) { $List.Add($sub.FullName) | Out-Null }
            }
        }
    } catch { }
}

function Get-WeTypeCandidates {
    $dirs = New-Object System.Collections.Generic.List[string]

    # Running processes
    try {
        $procs = Get-CimInstance Win32_Process -Filter "Name='wetype_renderer.exe' OR Name='wetype_server.exe' OR Name='wetype_service.exe'" -ErrorAction Stop
        foreach ($p in $procs) {
            if ($p.ExecutablePath) { Add-DirCandidate $dirs (Split-Path -Parent $p.ExecutablePath) }
        }
    } catch {
        foreach ($n in @('wetype_renderer', 'wetype_server', 'wetype_service')) {
            foreach ($p in Get-Process -Name $n -ErrorAction SilentlyContinue) {
                try { Add-DirCandidate $dirs (Split-Path -Parent $p.MainModule.FileName) } catch { }
            }
        }
    }

    # Uninstall registry entries
    foreach ($hive in @([Microsoft.Win32.Registry]::LocalMachine, [Microsoft.Win32.Registry]::CurrentUser)) {
        foreach ($root in @('SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall', 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall')) {
            try {
                $k = $hive.OpenSubKey($root)
                if ($k -eq $null) { continue }
                foreach ($sub in $k.GetSubKeyNames()) {
                    try {
                        $e = $k.OpenSubKey($sub)
                        if ($e -eq $null) { continue }
                        $name = [string]$e.GetValue('DisplayName')
                        if ($name -notmatch '微信输入法|WeType|WeChat Input') { continue }
                        $loc = [string]$e.GetValue('InstallLocation')
                        if ($loc) { Add-DirCandidate $dirs $loc }
                        $icon = [string]$e.GetValue('DisplayIcon')
                        if ($icon) { Add-DirCandidate $dirs (Split-Path -Parent ($icon.Trim('"').Split(',')[0])) }
                        $un = [string]$e.GetValue('UninstallString')
                        if ($un) { Add-DirCandidate $dirs (Split-Path -Parent ($un.Trim('"').Split(' ')[0])) }
                    } catch { }
                }
            } catch { }
        }
    }

    # Conventional roots
    foreach ($base in @($env:ProgramFiles, ${env:ProgramFiles(x86)}, 'C:\Program Files', 'C:\Program Files (x86)')) {
        if (-not $base) { continue }
        Add-DirCandidate $dirs (Join-Path $base 'Tencent\WeType')
    }

    return @($dirs | Select-Object -Unique)
}

function Get-Sha256([string]$Path) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [IO.File]::ReadAllBytes($Path)
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "")
    } finally {
        $sha.Dispose()
    }
}

function Get-RendererInfo([string]$Renderer) {
    if (-not (Test-Path $Renderer)) { throw "renderer not found: $Renderer" }
    $fi = Get-Item -LiteralPath $Renderer
    $ver = ''
    try { $ver = [Diagnostics.FileVersionInfo]::GetVersionInfo($Renderer).FileVersion } catch { }
    $hash = Get-Sha256 $Renderer
    return [PSCustomObject]@{
        path = $Renderer
        size = $fi.Length
        version = $ver
        sha256 = $hash
    }
}

function Resolve-WeTypeDir([string]$Dir) {
    if ($Dir) {
        $p = Get-RendererPath $Dir
        if ($p) { return (Split-Path -Parent $p) }
        # parent directory: choose a version child with renderer
        $children = @()
        if (Test-Path $Dir) {
            $children = @(Get-ChildItem -LiteralPath $Dir -Directory -ErrorAction SilentlyContinue | Where-Object { Test-Path (Join-Path $_.FullName 'wetype_renderer.exe') })
        }
        if ($children.Count -eq 1) { return $children[0].FullName }
        if ($children.Count -gt 1) {
            $preferred = $children | Sort-Object Name -Descending | Select-Object -First 1
            return $preferred.FullName
        }
        throw "wetype_renderer.exe not found under: $Dir"
    }

    $cands = @(Get-WeTypeCandidates)
    if ($cands.Count -eq 0) { throw 'WeType installation not found.' }
    # Prefer known build, then newest directory name.
    $preferred = $cands | Where-Object {
        try { [Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path $_ 'wetype_renderer.exe')).FileVersion -eq '3.0.2.44' } catch { $false }
    } | Select-Object -First 1
    if ($preferred) { return $preferred }
    return ($cands | Sort-Object -Descending | Select-Object -First 1)
}

function Get-ServiceInfo {
    $svc = @(Get-Service -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -match 'WeType' -or $_.DisplayName -match '微信输入法|WeType'
    } | Select-Object -First 1)
    if ($svc.Count -eq 0) { return $null }
    $s = $svc[0]
    $image = ''
    try {
        $rk = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey("SYSTEM\CurrentControlSet\Services\$($s.Name)")
        if ($rk) { $image = [string]$rk.GetValue('ImagePath') }
    } catch { }
    return [PSCustomObject]@{ name = $s.Name; status = [string]$s.Status; imagePath = $image }
}

function Get-SignalData([string]$SignatureText) {
    $tokens = @($SignatureText.Trim() -split '\s+' | Where-Object { $_ })
    $bytes = New-Object byte[] $tokens.Count
    $wild = New-Object bool[] $tokens.Count
    for ($i = 0; $i -lt $tokens.Count; $i++) {
        if ($tokens[$i] -eq '??') { $wild[$i] = $true; $bytes[$i] = 0 }
        else { $bytes[$i] = [Convert]::ToByte($tokens[$i], 16); $wild[$i] = $false }
    }
    return [PSCustomObject]@{ bytes = $bytes; wildcard = $wild; tokens = $tokens }
}

function Get-Signatures {
    if (-not (Test-Path $script:SignaturesPath)) { throw "signatures.json not found: $script:SignaturesPath" }
    return Get-Content -LiteralPath $script:SignaturesPath -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Find-Signature($Bytes, $Signatures) {
    $results = New-Object System.Collections.Generic.List[object]
    foreach ($pat in @($Signatures.patterns | Sort-Object priority)) {
        $sd = Get-SignalData $pat.pattern
        $hits = [WtSig]::Find($Bytes, $sd.bytes, $sd.wildcard)
        $startVal = if ($hits.Length -eq 1) { $hits[0] } else { $null }
        $results.Add([PSCustomObject]@{
            name = $pat.name
            kind = $pat.kind
            priority = [int]$pat.priority
            hits = @($hits)
            start = $startVal
            unique = ($hits.Length -eq 1)
        }) | Out-Null
    }
    return $results
}

function Get-ColorFromBytes($Bytes, [int]$Start, $Offsets) {
    $r = [BitConverter]::ToSingle($Bytes, $Start + [int]$Offsets.r)
    $g = [int]$Bytes[$Start + [int]$Offsets.g]
    $b = [int]$Bytes[$Start + [int]$Offsets.b]
    $rr = [Math]::Max(0, [Math]::Min(255, [int][Math]::Round($r * 255.0)))
    return ('#{0:X2}{1:X2}{2:X2}' -f $rr, $g, $b)
}

function Get-BackupPath([string]$Renderer) {
    $ver = [Diagnostics.FileVersionInfo]::GetVersionInfo($Renderer).FileVersion
    if (-not $ver) { $ver = Split-Path -Leaf (Split-Path -Parent $Renderer) }
    $safe = ($ver -replace '[^0-9A-Za-z._-]', '_')
    return Join-Path (Join-Path $script:BackupRoot $safe) 'wetype_renderer.exe.orig'
}
