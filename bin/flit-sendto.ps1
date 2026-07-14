param([switch]$Remove)

$SendTo = [Environment]::GetFolderPath('SendTo')
$Link   = Join-Path $SendTo 'Flit.lnk'

if ($Remove) {
    if (Test-Path $Link) { Remove-Item $Link -Force; Write-Host 'removed: Send to -> Flit' }
    else { Write-Host 'nothing to remove' }
    return
}

$Flit = Join-Path $PSScriptRoot 'flit.ps1'
if (-not (Test-Path $Flit)) { throw "flit.ps1 not found beside this script ($Flit)" }

$Pwsh = Join-Path $PSHOME 'powershell.exe'
$Shell = New-Object -ComObject WScript.Shell
$Shortcut = $Shell.CreateShortcut($Link)
$Shortcut.TargetPath       = $Pwsh
$Shortcut.Arguments        = "-NoProfile -ExecutionPolicy Bypass -File `"$Flit`""
$Shortcut.WorkingDirectory = $PSScriptRoot
$Shortcut.IconLocation     = "$Pwsh,0"
$Shortcut.Description       = 'Send to Flit'
$Shortcut.Save()
Write-Host 'installed: right-click a file -> Send to -> Flit'