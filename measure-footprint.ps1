param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$Report,
    [int]$Samples = 6,
    [switch]$RequireUI
)
$ErrorActionPreference = 'Stop'
if ($Samples -lt 2 -or $Samples -gt 181) { throw 'Samples must be between 2 and 181.' }
$root = (Resolve-Path -LiteralPath $Directory).Path.TrimEnd('\')
$executables = @((Join-Path $root 'PetRepair.exe'), (Join-Path $root 'ui\PetRepair.UI.exe'), (Join-Path $root 'PetRepair.UI.exe'))
$files = @(Get-ChildItem -LiteralPath $root -Recurse -File | Where-Object { $_.Extension -ne '.pdb' -and !$_.FullName.StartsWith($root+'\data\',[StringComparison]::OrdinalIgnoreCase) })
$rows = @()
$initialIds = $null
for ($sample = 0; $sample -lt $Samples; $sample++) {
    $processes = @(Get-Process -Name PetRepair,PetRepair.UI -ErrorAction SilentlyContinue | Where-Object { $_.Path -in $executables })
    if (!$processes.Count) { throw 'No target app process is running; open the UI or tray before measuring.' }
    if ($RequireUI -and !($processes | Where-Object ProcessName -eq 'PetRepair.UI')) { throw 'The UI is not running; this is not a valid GUI memory sample.' }
    $ids = ($processes.Id | Sort-Object) -join ','
    if ($null -eq $initialIds) { $initialIds=$ids } elseif ($initialIds -ne $ids) { throw 'Target processes changed during sampling; discard this run and measure again.' }
    foreach ($process in $processes) {
        $rows += [pscustomobject]@{
            Sample=$sample; Timestamp=(Get-Date -Format o); Name=$process.ProcessName; Id=$process.Id
            WorkingSetBytes=$process.WorkingSet64; PrivateBytes=$process.PrivateMemorySize64
            CpuMs=$process.TotalProcessorTime.TotalMilliseconds; Handles=$process.HandleCount
        }
    }
    if ($sample -lt $Samples-1) { Start-Sleep -Seconds 2 }
}
$summary = @($rows | Group-Object Id | ForEach-Object {
    [pscustomobject]@{
        Name=$_.Group[0].Name; Id=$_.Group[0].Id
        WorkingSetBytesMean=[math]::Round(($_.Group | Measure-Object WorkingSetBytes -Average).Average)
        PrivateBytesMean=[math]::Round(($_.Group | Measure-Object PrivateBytes -Average).Average)
        CpuDeltaMs=$_.Group[-1].CpuMs-$_.Group[0].CpuMs
        HandleDelta=$_.Group[-1].Handles-$_.Group[0].Handles
    }
})
$result = [pscustomobject]@{
    Directory=$root; FileBytes=($files | Measure-Object Length -Sum).Sum; Files=$files.Count
    Note='FileBytes excludes developer PDBs and user data; it is not ZIP size or filesystem allocated bytes. Memory is an idle sample, not a peak or long-term guarantee.'
    Processes=$summary
    TotalWorkingSetBytesMean=($summary | Measure-Object WorkingSetBytesMean -Sum).Sum
    TotalPrivateBytesMean=($summary | Measure-Object PrivateBytesMean -Sum).Sum
    Samples=$rows
}
$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Report -Encoding utf8
$result | Select-Object FileBytes,Files,Processes,TotalWorkingSetBytesMean,TotalPrivateBytesMean | ConvertTo-Json -Depth 5
