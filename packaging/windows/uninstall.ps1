<#
.SYNOPSIS
  Removes the tear preview: right-click entries, PATH entry and installed files.
#>
$ErrorActionPreference = 'Stop'

$target = Join-Path $env:LOCALAPPDATA 'Programs\tear'
$extensions = '.zip', '.7z', '.rar', '.tar', '.gz', '.tgz', '.xz', '.txz', '.bz2', '.tbz2', '.zst', '.tzst'

foreach ($ext in $extensions) {
    $menu = "HKCU:\Software\Classes\SystemFileAssociations\$ext\shell\tear"
    if (Test-Path $menu) { Remove-Item $menu -Recurse -Force }
}

# Edit the raw registry value so entries like %USERPROFILE%\bin stay unexpanded.
$envKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
$raw = [string]$envKey.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$parts = @($raw -split ';' | Where-Object { $_ })
if ($parts -contains $target) {
    $kept = @($parts | Where-Object { $_ -ne $target })
    $envKey.SetValue('Path', ($kept -join ';'), $envKey.GetValueKind('Path'))
    # Tell Explorer and new terminals that the environment changed.
    [Environment]::SetEnvironmentVariable('TEAR_INSTALL_REFRESH', '1', 'User')
    [Environment]::SetEnvironmentVariable('TEAR_INSTALL_REFRESH', $null, 'User')
}
$envKey.Close()

if (Test-Path $target) {
    # This script may be running from the install folder; remove it after exit.
    if ($PSScriptRoot -eq $target) {
        Start-Process cmd.exe -WindowStyle Hidden -ArgumentList "/c timeout /t 2 >nul & rmdir /s /q `"$target`""
    } else {
        Remove-Item $target -Recurse -Force
    }
}

Write-Host 'Tear has been removed.'
