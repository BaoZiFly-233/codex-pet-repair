param([switch]$Test, [string]$Toolchain = 'stable')
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    if ($Test) { & cargo "+$Toolchain" test --locked; if ($LASTEXITCODE -ne 0) { throw 'Tests failed' } }
    & cargo "+$Toolchain" build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
} finally { Pop-Location }
