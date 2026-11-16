param([int]$Seconds = 60, [int]$IntervalMs = 1000)
# Zodyvel süreç ağacının özel belleğini süreç türüne göre (ana, WebView2 tarayıcı/gpu/renderer/utility)
# aralıklarla yazar; yanında ada penceresinin bölge boyutu (hangi seviyede olduğu).
Add-Type @'
using System;
using System.Runtime.InteropServices;
public struct RCm { public int L, T, R, B; }
public static class Wm {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("gdi32.dll")] public static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
  [DllImport("user32.dll")] public static extern int GetWindowRgn(IntPtr h, IntPtr r);
  [DllImport("gdi32.dll")] public static extern int GetRgnBox(IntPtr r, out RCm b);
  [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr o);
}
'@
$all = Get-CimInstance Win32_Process
$root = @($all | Where-Object { $_.Name -eq 'zodyvel.exe' })
$ids = @($root.ProcessId)
do {
  $added = @($all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId })
  $ids += @($added.ProcessId)
} while ($added.Count)
$kind = @{}
foreach ($p in $all | Where-Object { $ids -contains $_.ProcessId }) {
  $kind[[int]$p.ProcessId] = if ($p.Name -eq 'zodyvel.exe') { 'ana' } elseif ($p.CommandLine -match '--type=([\w-]+)') { $matches[1] } else { 'tarayici' }
}
$zid = [int]$root[0].ProcessId
function Region {
  $script:box = ''
  [Wm]::EnumWindows({ param($h, $l)
    $p = 0; [Wm]::GetWindowThreadProcessId($h, [ref]$p) | Out-Null
    if ($p -eq $zid -and [Wm]::IsWindowVisible($h)) {
      $r = [Wm]::CreateRectRgn(0, 0, 0, 0)
      if ([Wm]::GetWindowRgn($h, $r) -gt 1) { $b = New-Object RCm; [Wm]::GetRgnBox($r, [ref]$b) | Out-Null; $script:box = '{0}x{1}' -f ($b.R - $b.L), ($b.B - $b.T) }
      [Wm]::DeleteObject($r) | Out-Null
    }
    $true }, [IntPtr]::Zero) | Out-Null
  $script:box
}
$order = 'ana', 'tarayici', 'gpu-process', 'renderer', 'utility', 'crashpad-handler'
$start = Get-Date
while (((Get-Date) - $start).TotalSeconds -lt $Seconds) {
  $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $ids -contains [int]$_.IDProcess }
  $sum = @{}
  foreach ($p in $perf) { $k = $kind[[int]$p.IDProcess]; $sum[$k] = $sum[$k] + $p.WorkingSetPrivate / 1MB }
  $total = ($sum.Values | Measure-Object -Sum).Sum
  $parts = ($order | Where-Object { $sum.ContainsKey($_) } | ForEach-Object { '{0} {1:N0}' -f $_, $sum[$_] }) -join ' | '
  '{0,5:N1}s  {1,-8} toplam {2,6:N1} MB  ({3})' -f ((Get-Date) - $start).TotalSeconds, (Region), $total, $parts
  Start-Sleep -Milliseconds $IntervalMs
}
