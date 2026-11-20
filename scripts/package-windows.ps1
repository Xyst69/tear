<#
.SYNOPSIS
  Builds tear for Windows x64 and assembles dist\tear-windows-x64.zip
  (executables, installer, sample archives and a testing guide) plus
  dist\SHA256SUMS. These are the GitHub release assets the website links to.
#>
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$name = 'tear-windows-x64'
$dist = Join-Path $repo 'dist'
$out = Join-Path $dist $name
$sevenZip = 'C:\Program Files\7-Zip\7z.exe'

& (Join-Path $PSScriptRoot 'cargo.ps1') build --locked --release --bins
if ($LASTEXITCODE) { throw 'Release build failed.' }

if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Force $out, "$out\samples", "$out\licenses" | Out-Null

Copy-Item "$repo\target\release\tear.exe", "$repo\target\release\tearw.exe" $out
Copy-Item "$repo\packaging\windows\*" $out
Copy-Item "$repo\LICENSE-MIT", "$repo\LICENSE-APACHE" "$out\licenses"
Copy-Item "$repo\THIRD_PARTY_LICENSES\*" "$out\licenses"

# Sample archives with known content.
$work = Join-Path $dist 'sample-src'
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Force "$work\holiday-photos\day-1", "$work\loose" | Out-Null
1..3 | ForEach-Object {
    $bytes = New-Object byte[] (256KB * $_)
    (New-Object Random $_).NextBytes($bytes)
    [IO.File]::WriteAllBytes("$work\holiday-photos\day-1\photo-$_.raw", $bytes)
}
Set-Content "$work\holiday-photos\notes.txt" 'Packed list: tent, stove, maps.' -Encoding utf8
Set-Content "$work\loose\report.txt" 'Quarterly numbers go here.' -Encoding utf8
Set-Content "$work\loose\budget.csv" "item,cost`ntent,120`nstove,45" -Encoding utf8

Compress-Archive "$work\holiday-photos" "$out\samples\holiday-photos.zip"
Compress-Archive "$work\loose\*" "$out\samples\loose-files.zip"
& "$env:SystemRoot\System32\tar.exe" -czf "$out\samples\holiday-photos.tar.gz" -C $work holiday-photos
if ($LASTEXITCODE) { throw 'tar failed.' }
if (Test-Path $sevenZip) {
    Push-Location $work
    try {
        & $sevenZip a -bso0 -bsp0 "$out\samples\holiday-photos.7z" holiday-photos
        & $sevenZip a -bso0 -bsp0 -mem=AES256 -ptear "$out\samples\secret-password-is-tear.zip" loose\report.txt
        if ($LASTEXITCODE) { throw '7-Zip failed.' }
    } finally { Pop-Location }
} else {
    Write-Warning '7-Zip not found; skipping the 7z and encrypted samples.'
}
Remove-Item $work -Recurse -Force

$zip = Join-Path $dist "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive "$out\*" $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
# sha256sum format, LF line ending, so `sha256sum -c SHA256SUMS` works too.
[IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS'), "$hash  $name.zip`n")
Write-Host "Package: $zip"
Write-Host "SHA-256: $hash"
