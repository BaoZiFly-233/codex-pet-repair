param([string]$CoreBinary, [string]$UiBinary, [string]$Toolchain = 'stable', [string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$version = (Select-String -LiteralPath (Join-Path $project 'Cargo.toml') -Pattern '^version = "([^"]+)"$').Matches[0].Groups[1].Value
$uiVersion = (Select-String -LiteralPath (Join-Path $PSScriptRoot 'Cargo.toml') -Pattern '^version = "([^"]+)"$').Matches[0].Groups[1].Value
if ($version -ne $uiVersion) { throw 'Core and UI versions must match.' }
$release = if ($OutputDirectory) { [IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $project "dist\PetRepair-v$version-windows-x64" }
if (Test-Path -LiteralPath $release) { throw 'Use a new output directory; existing packages and user data are preserved.' }
if (!$UiBinary) { $UiBinary = Join-Path $PSScriptRoot 'target\release\pet-repair-native-ui.exe' }
if (!$CoreBinary) { $CoreBinary = Join-Path $project 'target\release\pet-repair.exe' }
$binary = $UiBinary
if (!(Test-Path -LiteralPath $binary)) { throw 'Build the native UI first.' }
if (!(Test-Path -LiteralPath $CoreBinary)) { throw 'Build the core first.' }
$dependencies = & cargo "+$Toolchain" tree --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') --locked --target x86_64-pc-windows-msvc -e normal,no-proc-macro --prefix none --no-dedupe --format '{p}' | Sort-Object -Unique
if ($LASTEXITCODE -ne 0) { throw 'Dependency inventory failed.' }
$metadata = & cargo "+$Toolchain" metadata --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed.' }
$notices = @{}
$missing = @()
foreach ($dependency in $dependencies) {
    if ($dependency -notmatch '^([\w-]+) v([\w.+-]+)(?: \(.*\))?$') { continue }
    if ($Matches[1] -eq 'pet-repair-native-ui') { continue }
    $crateName = $Matches[1]+'-'+$Matches[2]
    $dependency = $Matches[1]+' v'+$Matches[2]
    $package = $metadata.packages | Where-Object { $_.name+'-'+$_.version -eq $crateName } | Select-Object -First 1
    if (!$package) { throw "Dependency metadata not found: $crateName" }
    $crate = Split-Path -Parent $package.manifest_path
    $files = @(Get-ChildItem -LiteralPath $crate -File | Where-Object Name -match '^(LICENSE|COPYING|NOTICE|UNLICENSE)')
    if (Test-Path -LiteralPath (Join-Path $crate 'LICENSES')) {
        $files += Get-ChildItem -LiteralPath (Join-Path $crate 'LICENSES') -File | Where-Object Name -notmatch 'LicenseRef-Slint-Software|GPL-3'
    }
    if (!$files.Count) {
        $override = Join-Path $PSScriptRoot ('license-overrides\'+$crateName+'\LICENSE.txt')
        if (Test-Path -LiteralPath $override) { $files = @(Get-Item -LiteralPath $override) }
    }
    if (!$files.Count) { $missing += $dependency; continue }
    foreach ($file in $files) {
        $hash=(Get-FileHash -LiteralPath $file.FullName).Hash
        if (!$notices.ContainsKey($hash)) { $notices[$hash]=@{Packages=@();Text=[IO.File]::ReadAllText($file.FullName)} }
        $notices[$hash].Packages += $dependency
    }
}
$report = @('Third-party notices for Codex Pet Repair.','Slint is used under LicenseRef-Slint-Royalty-free-2.0; the in-app component license dialog provides attribution.','')
foreach ($hash in $notices.Keys | Sort-Object) {
    $report += ($notices[$hash].Packages | Sort-Object -Unique) -join ', '
    $report += $notices[$hash].Text
    $report += ''
}
New-Item -ItemType Directory -Path (Join-Path $release 'ui') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $project 'test-results') -Force | Out-Null
$missing | Set-Content -LiteralPath (Join-Path $project 'test-results\native-missing-notices.txt') -Encoding utf8
if ($missing.Count) { throw "Missing license files: $($missing -join ', ')" }
Copy-Item -LiteralPath $binary -Destination (Join-Path $release 'ui\PetRepair.UI.exe') -Force
$coreSource = $CoreBinary
$coreTarget = Join-Path $release 'PetRepair.exe'
if (!(Test-Path -LiteralPath $coreTarget) -or (Get-FileHash -LiteralPath $coreSource).Hash -ne (Get-FileHash -LiteralPath $coreTarget).Hash) {
    Copy-Item -LiteralPath $coreSource -Destination $coreTarget -Force
}
Copy-Item -LiteralPath (Join-Path $project 'CREDITS.md') -Destination $release -Force
Copy-Item -LiteralPath (Join-Path $project 'LICENSE') -Destination $release -Force
$coreLicenses=Join-Path $project 'licenses'
foreach ($file in Get-ChildItem -LiteralPath $coreLicenses -File -Recurse | Where-Object { !$_.FullName.StartsWith($coreLicenses+'\ui\',[StringComparison]::OrdinalIgnoreCase) }) {
    $destination=Join-Path $release ([IO.Path]::GetRelativePath($project,$file.FullName))
    New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($destination)) -Force | Out-Null
    Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
}
[IO.File]::WriteAllLines((Join-Path $release 'THIRD-PARTY-NOTICES.txt'),$report)
$zipPath=$release+'.zip'
$stream=[IO.File]::Open($zipPath,[IO.FileMode]::Create)
try {
    $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$true)
    try {
        foreach ($file in Get-ChildItem -LiteralPath $release -File -Recurse | Where-Object { !$_.FullName.StartsWith($release+'\data\',[StringComparison]::OrdinalIgnoreCase) -and $_.Extension -ne '.pdb' }) {
            $relative=[IO.Path]::GetRelativePath($release,$file.FullName).Replace('\','/')
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$file.FullName,$relative,[IO.Compression.CompressionLevel]::Optimal) | Out-Null
        }
    } finally {$zip.Dispose()}
} finally {$stream.Dispose()}
Get-Item -LiteralPath $zipPath | Select-Object Name,Length
