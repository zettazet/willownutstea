param(
    [ValidateSet('idle','messaging','call2','call8')][string]$Scenario='idle',
    [int]$ProcessId=0,
    [string]$Exe='',
    [ValidateRange(10,86400)][int]$Seconds=120
)
$ErrorActionPreference='Stop'
$startupMs=$null
if ($Exe) {
    $clock=[Diagnostics.Stopwatch]::StartNew()
    $process=Start-Process -FilePath (Resolve-Path $Exe).Path -PassThru
    if (!$process.WaitForInputIdle(15000)) { throw 'Client UI did not become input-idle in 15 seconds' }
    $clock.Stop(); $startupMs=$clock.Elapsed.TotalMilliseconds
    $ProcessId=$process.Id
} elseif ($ProcessId -gt 0) { $process=Get-Process -Id $ProcessId }
else { throw 'Supply -ProcessId for a running client, or -Exe to measure startup' }
Write-Host "Sampling process $ProcessId, scenario $Scenario. Prepare this scenario before sampling."
# Only sleep for sampling; this script does not alter the call, log in or synthesize microphone input.
$rows=[System.Collections.Generic.List[object]]::new();$process.Refresh();$lastCpu=$process.TotalProcessorTime.TotalSeconds;$lastTime=[DateTime]::UtcNow
for ($i=0;$i -lt $Seconds;$i++) {
    Start-Sleep -Seconds 1
    $process.Refresh();if ($process.HasExited){throw 'Client exited during measurement'}
    $time=[DateTime]::UtcNow;$cpu=$process.TotalProcessorTime.TotalSeconds
    $elapsed=($time-$lastTime).TotalSeconds
    $rows.Add([pscustomobject]@{
        UTC=$time.ToString('o');Scenario=$Scenario;ProcessId=$ProcessId
        WorkingSetMiB=[math]::Round($process.WorkingSet64/1MB,3)
        PrivateMiB=[math]::Round($process.PrivateMemorySize64/1MB,3)
        CPUOneCorePercent=[math]::Round(100*($cpu-$lastCpu)/$elapsed,3)
        CPUWholeMachinePercent=[math]::Round(100*($cpu-$lastCpu)/$elapsed/[Environment]::ProcessorCount,3)
        StartupInputIdleMs=$startupMs
    })
    $lastCpu=$cpu;$lastTime=$time
}
$file="benchmarks-$Scenario-$([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss')).csv"
$rows|Export-Csv -NoTypeInformation -Path $file
$sorted=@($rows.WorkingSetMiB|Sort-Object)
[pscustomobject]@{
    File=$file;Samples=$rows.Count
    MeanWorkingSetMiB=($rows.WorkingSetMiB|Measure-Object -Average).Average
    P95WorkingSetMiB=$sorted[[math]::Min($sorted.Count-1,[math]::Floor($sorted.Count*0.95))]
    PeakWorkingSetMiB=($rows.WorkingSetMiB|Measure-Object -Maximum).Maximum
    MeanPrivateMiB=($rows.PrivateMiB|Measure-Object -Average).Average
    MeanCPUOneCorePercent=($rows.CPUOneCorePercent|Measure-Object -Average).Average
    StartupInputIdleMs=$startupMs
}|Format-List
