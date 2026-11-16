# Ada açıkken (seviye 2) A/B: A = imleç ada üstünde kıpırdıyor (animasyonlar çalışır),
# B = imleç durgun, 12 sn sonra "dinlenme" tüm animasyonları duraklatır.
$sp = Split-Path $MyInvocation.MyCommand.Path
Add-Type @'
using System.Runtime.InteropServices;
public struct PT { public int X; public int Y; }
public static class Cur {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out PT p);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
}
'@
[Cur]::SetProcessDPIAware() | Out-Null
$orig = New-Object PT
[Cur]::GetCursorPos([ref]$orig) | Out-Null
$cx = [int]([Cur]::GetSystemMetrics(0) / 2)
$cores = [Environment]::ProcessorCount

$all = Get-CimInstance Win32_Process
$ids = @(($all | Where-Object { $_.Name -eq 'zodyvel.exe' }).ProcessId)
do {
  $added = @($all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId })
  $ids += @($added.ProcessId)
} while ($added.Count)
function CpuMs { (Get-Process -Id $ids -ErrorAction SilentlyContinue | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum }

# Başlık satırının sağ boş kısmı (düğme yok): 488 px genişliğin sağ çeyreği, fiziksel ~%125.
$x = $cx + 150; $y = 30
[Cur]::SetCursorPos($x, $y) | Out-Null
Start-Sleep -Seconds 1
& "$sp\level.ps1"

$t0 = CpuMs
for ($i = 0; $i -lt 60; $i++) { [Cur]::SetCursorPos($x + ($i % 2) * 3, $y) | Out-Null; Start-Sleep -Milliseconds 500 }
$t1 = CpuMs
"A (animasyonlar calisiyor): CPU %{0:N2}" -f (($t1 - $t0) / (30000 * $cores) * 100)
& "$sp\level.ps1"

Start-Sleep -Seconds 14
$t0 = CpuMs
Start-Sleep -Seconds 30
$t1 = CpuMs
"B (dinlenme, animasyonlar durdu): CPU %{0:N2}" -f (($t1 - $t0) / (30000 * $cores) * 100)
& "$sp\level.ps1"

[Cur]::SetCursorPos($orig.X, $orig.Y) | Out-Null
Start-Sleep -Seconds 3
& "$sp\level.ps1"
