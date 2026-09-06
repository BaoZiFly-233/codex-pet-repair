param([string]$CoreBinary, [string]$UiBinary, [string]$Toolchain = 'stable')
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'experiments/native-ui/package.ps1') -CoreBinary $CoreBinary -UiBinary $UiBinary -Toolchain $Toolchain
