param([int]$Seconds = 30, [string]$Label = 'olcum')
# Zodyvel + tüm alt süreçleri (WebView2) için CPU (sistemin yüzdesi) ve bellek (özel çalışma kümesi).
$all = Get-CimInstance Win32_Process
$ids = @(($all | Where-Object { $_.Name -eq 'zodyvel.exe' }).ProcessId)
do {
  $added = @($all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId })
  $ids += @($added.ProcessId)
} while ($added.Count)
function CpuMs { (Get-Process -Id $ids -ErrorAction SilentlyContinue | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum }
$t0 = CpuMs
Start-Sleep -Seconds $Seconds
$t1 = CpuMs
$cores = [Environment]::ProcessorCount
$pct = ($t1 - $t0) / ($Seconds * 1000 * $cores) * 100
$mem = (Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $ids -contains [int]$_.IDProcess } | Measure-Object -Property WorkingSetPrivate -Sum).Sum / 1MB
"{0}: CPU %{1:N2} (sistemin, {2} mantiksal cekirdek) | ozel bellek {3:N1} MB | {4} surec" -f $Label, $pct, $cores, $mem, $ids.Count
