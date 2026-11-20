<#
.SYNOPSIS
  Installs the tear preview for the current user and adds it to File Explorer's
  right-click menu for archives. No administrator rights are needed.

.DESCRIPTION
  - Copies tear.exe and tearw.exe to %LOCALAPPDATA%\Programs\tear
  - Adds "tear" to the right-click menu of supported archive types (current user only)
  - Adds the install folder to your user PATH so `tear` works in a new terminal

  Run uninstall.ps1 (or Uninstall.cmd) to remove everything again.
  On Windows 11 the entry is under "Show more options" (or press Shift+F10).
#>
$ErrorActionPreference = 'Stop'

$source = $PSScriptRoot
$target = Join-Path $env:LOCALAPPDATA 'Programs\tear'
$extensions = '.zip', '.7z', '.rar', '.tar', '.gz', '.tgz', '.xz', '.txz', '.bz2', '.tbz2', '.zst', '.tzst'

foreach ($exe in 'tear.exe', 'tearw.exe') {
    if (-not (Test-Path (Join-Path $source $exe))) { throw "$exe is missing next to install.ps1. Extract the whole download first." }
}

New-Item -ItemType Directory -Force $target | Out-Null
foreach ($file in 'tear.exe', 'tearw.exe', 'uninstall.ps1', 'Uninstall.cmd') {
    $from = Join-Path $source $file
    if (Test-Path $from) { Copy-Item $from $target -Force }
}
# Copies of downloaded files keep their "from the internet" marking; these are ours.
Get-ChildItem $target -File | Unblock-File

$tearw = Join-Path $target 'tearw.exe'
$items = @(
    @{ Key = '1-extract-here'; Label = 'Extract here'; Args = 'x "%1"' },
    @{ Key = '2-extract-folder'; Label = 'Extract to new folder'; Args = 'x "%1" --folder' },
    @{ Key = '3-test'; Label = 'Test archive'; Args = 't "%1"' }
)

foreach ($ext in $extensions) {
    $menu = "HKCU:\Software\Classes\SystemFileAssociations\$ext\shell\tear"
    if (Test-Path $menu) { Remove-Item $menu -Recurse -Force }
    New-Item -Force $menu | Out-Null
    Set-ItemProperty $menu -Name 'MUIVerb' -Value 'Tear'
    Set-ItemProperty $menu -Name 'SubCommands' -Value ''
    foreach ($item in $items) {
        $verb = "$menu\shell\$($item.Key)"
        New-Item -Force "$verb\command" | Out-Null
        Set-ItemProperty $verb -Name 'MUIVerb' -Value $item.Label
        Set-ItemProperty "$verb\command" -Name '(default)' -Value "`"$tearw`" $($item.Args)"
    }
}

# Edit the raw registry value so entries like %USERPROFILE%\bin stay unexpanded.
$envKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
$raw = [string]$envKey.GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$parts = @($raw -split ';' | Where-Object { $_ })
if ($parts -notcontains $target) {
    $kind = if ($raw) { $envKey.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
    $envKey.SetValue('Path', (($parts + $target) -join ';'), $kind)
    # Tell Explorer and new terminals that the environment changed.
    [Environment]::SetEnvironmentVariable('TEAR_INSTALL_REFRESH', '1', 'User')
    [Environment]::SetEnvironmentVariable('TEAR_INSTALL_REFRESH', $null, 'User')
}
$envKey.Close()

Write-Host ''
Write-Host "Tear is installed in $target"
Write-Host 'Right-click any .zip, .7z, .rar or .tar.* file and choose Tear.'
Write-Host 'On Windows 11, choose "Show more options" first (or press Shift+F10).'
Write-Host 'Open a new terminal to use the tear command.'
