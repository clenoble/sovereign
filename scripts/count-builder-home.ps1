# Count the builder's home folder, as text, in a release binary; exit 1 when it is there.
# The Windows counterpart of the count at the end of scripts/release-shell-linux.sh (v0.0.13 scope,
# the builder paths; Celine, 2026-11-17: the builder's path at 0 in each release artifact).
#
# It looks for the home folder (default: $env:USERPROFILE) in both slash forms, in any case, as
# single-byte text and as UTF-16LE text at either byte alignment. It prints the count and, when it is
# not 0, the paths found, grouped by their first components.
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\count-builder-home.ps1 <binary> [<home>]
param(
    [Parameter(Mandatory = $true)][string]$Binary,
    [string]$HomeFolder = $env:USERPROFILE
)

$ErrorActionPreference = 'Stop'
$bytes = [System.IO.File]::ReadAllBytes((Get-Item -LiteralPath $Binary).FullName)

# Latin-1 maps each byte to one char, so a single-byte match is a match of the same bytes.
$latin = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes)
$utf16Even = [System.Text.Encoding]::Unicode.GetString($bytes)
$utf16Odd = ''
if ($bytes.Length -gt 1) {
    $utf16Odd = [System.Text.Encoding]::Unicode.GetString($bytes, 1, $bytes.Length - 1)
}

$trimmed = $HomeFolder.TrimEnd('\', '/')
$slashPattern = [regex]::Escape($trimmed) -replace '\\\\', '[\\/]'
# The count matches the folder alone: paths packed side by side are each counted. The few
# components after it, read from each match's position, only group what is printed.
$finder = New-Object System.Text.RegularExpressions.Regex($slashPattern, 'IgnoreCase')
$tailPattern = New-Object System.Text.RegularExpressions.Regex('\G[^\x00-\x1f"<>|]{0,120}')

$total = 0
$groups = @{}
foreach ($pair in @(@('text', $latin), @('utf-16', $utf16Even), @('utf-16', $utf16Odd))) {
    $kind = $pair[0]
    foreach ($hit in $finder.Matches($pair[1])) {
        $total++
        $tail = $tailPattern.Match($pair[1], $hit.Index + $hit.Length).Value
        $parts = (($hit.Value + $tail) -split '[\\/]') | Select-Object -First 7
        $key = "[$kind] " + ($parts -join '/')
        if ($groups.ContainsKey($key)) { $groups[$key]++ } else { $groups[$key] = 1 }
    }
}

Write-Output "=== the builder's home ($trimmed) in $Binary : $total"
if ($total -ne 0) {
    $groups.GetEnumerator() | Sort-Object -Property Value -Descending | Select-Object -First 20 |
        ForEach-Object { Write-Output ("{0,6}  {1}" -f $_.Value, $_.Key) }
    exit 1
}
exit 0
