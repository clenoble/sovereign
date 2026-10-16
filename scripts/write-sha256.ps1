# Write <file>.sha256 next to <file>, in the format `sha256sum` writes and
# `sha256sum -c` reads: "<lowercase hex>  <basename>" + LF (no CRLF, no BOM).
# Concatenating the .sha256 files of all release assets gives SHA256SUMS.
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\write-sha256.ps1 <file>
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$item = Get-Item -LiteralPath $Path
$hash = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$line = "$hash  $($item.Name)`n"
$out = "$($item.FullName).sha256"
[System.IO.File]::WriteAllText($out, $line, (New-Object System.Text.UTF8Encoding($false)))
Write-Output "$hash  $($item.Name)"
Write-Output "written: $out"
