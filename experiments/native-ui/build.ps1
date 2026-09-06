param([switch]$Locked, [string]$Toolchain = 'stable')
$ErrorActionPreference = 'Stop'
$argsList = @('build','--release','--manifest-path',(Join-Path $PSScriptRoot 'Cargo.toml'))
if ($Locked) { $argsList += '--locked' }
& cargo "+$Toolchain" @argsList
if ($LASTEXITCODE -ne 0) { throw 'UI build failed' }
