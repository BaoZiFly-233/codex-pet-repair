param([Parameter(Mandatory)][string]$Executable, [string]$OutputDirectory)
$ErrorActionPreference='Stop'
if (!$OutputDirectory) { $OutputDirectory=Join-Path $PSScriptRoot '..\..\test-results\layout-1.1.0' }
$output=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
$cases=@()
foreach ($scale in @('1','1.5','2')) {
    foreach ($state in @('default','dark','busy','startup')) {
        $cases+=@{Name="$state-$scale";Scale=$scale;Args=$(if($state -eq 'default'){@()}else{@("--$state")});Width=432;Height=0}
    }
}
$cases+=@{Name='minimum';Scale='1';Args=@('--size','320x260');Width=320;Height=260}
$cases+=@{Name='wide';Scale='1';Args=@('--size','1200x560');Width=1200;Height=560}
foreach ($case in $cases) {
    $path=Join-Path $output ($case.Name+'.png')
    $start=[Diagnostics.ProcessStartInfo]::new([IO.Path]::GetFullPath($Executable))
    $start.UseShellExecute=$false
    $start.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    $start.Environment['SLINT_SCALE_FACTOR']=$case.Scale
    foreach($arg in (@('--preview')+$case.Args+@('--capture',$path))) {$start.ArgumentList.Add($arg)}
    $process=[Diagnostics.Process]::Start($start)
    if (!$process.WaitForExit(15000)) {throw "Preview timeout: $($case.Name)"}
    if ($process.ExitCode -ne 0) {throw "Preview failed: $($case.Name)"}
    $process.Dispose()
    $layout=Get-Content -LiteralPath ($path+'.json') -Raw | ConvertFrom-Json
    if ([math]::Abs($layout.width-$case.Width) -gt 1) {throw "Incorrect width: $($case.Name)"}
    if ($case.Height -and [math]::Abs($layout.height-$case.Height) -gt 1) {throw "Explicit size changed: $($case.Name)"}
    if ($case.Name -ne 'minimum' -and $layout.scroll_overflow -gt 0.5) {throw "Unexpected scrolling: $($case.Name): $($layout.scroll_overflow) DIP"}
    if ($case.Name -eq 'minimum' -and $layout.scroll_overflow -le 0) {throw 'Small windows must still support scrolling'}
    Write-Output "$($case.Name): $($layout.width)x$($layout.height), overflow=$($layout.scroll_overflow)"
}
