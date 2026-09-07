param([string]$CoreBinary, [string]$UiBinary, [string]$Toolchain = 'stable', [string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'experiments/native-ui/package.ps1') -CoreBinary $CoreBinary -UiBinary $UiBinary -Toolchain $Toolchain -OutputDirectory $OutputDirectory
