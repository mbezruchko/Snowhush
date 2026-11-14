# Packs the built app into Snowhush.msix for the Microsoft Store, which signs it on upload: the app, the manifest with
# this version and the logos. Needs the Windows SDK's makeappx.exe.
#   pwsh windows/store/pack.ps1 [-Exe windows\target\release\Snowhush.exe] [-Output dist\Snowhush.msix]
param(
    [string]$Exe = "$PSScriptRoot\..\target\release\Snowhush.exe",
    [string]$Output = "$PSScriptRoot\..\..\dist\Snowhush.msix"
)
$ErrorActionPreference = 'Stop'

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'makeappx.exe not found: install the Windows SDK' }

$version = (Select-String -Path "$PSScriptRoot\..\Cargo.toml" -Pattern '^version = "(.*)"$' | Select-Object -First 1).Matches[0].Groups[1].Value
$stage = Join-Path ([System.IO.Path]::GetTempPath()) "snowhush-msix-$PID"
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $Exe "$stage\Snowhush.exe"
Copy-Item -Recurse "$PSScriptRoot\Assets" "$stage\Assets"
# The Store wants four parts, the last one 0.
(Get-Content -Raw "$PSScriptRoot\AppxManifest.xml").Replace('$VERSION', "$version.0") | Set-Content -NoNewline "$stage\AppxManifest.xml"

New-Item -ItemType Directory -Force (Split-Path $Output) | Out-Null
& $makeappx.FullName pack /o /d $stage /p $Output
if ($LASTEXITCODE -ne 0) { throw "makeappx failed: $LASTEXITCODE" }
Remove-Item -Recurse $stage
Write-Output (Resolve-Path $Output)
