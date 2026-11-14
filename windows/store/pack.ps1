# Packs a folder laid out by stage.sh into Snowhush.msix for the Microsoft Store, which signs it on upload.
# Needs the Windows SDK's makeappx.exe.
#   pwsh windows/store/pack.ps1 [-Folder dist\store] [-Output dist\Snowhush.msix]
param(
    [string]$Folder = "$PSScriptRoot\..\..\dist\store",
    [string]$Output = "$PSScriptRoot\..\..\dist\Snowhush.msix"
)
$ErrorActionPreference = 'Stop'

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'makeappx.exe not found: install the Windows SDK' }

& $makeappx.FullName pack /o /d $Folder /p $Output
if ($LASTEXITCODE -ne 0) { throw "makeappx failed: $LASTEXITCODE" }
Write-Output (Resolve-Path $Output)
