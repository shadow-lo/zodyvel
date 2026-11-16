# İmleci şeride götürür, mininin açıldığını bölge boyutundan doğrular, imleci geri koyar.
$sp = Split-Path $MyInvocation.MyCommand.Path
Add-Type @'
using System.Runtime.InteropServices;
public struct PT2 { public int X; public int Y; }
public static class Cur2 {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out PT2 p);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
}
'@
[Cur2]::SetProcessDPIAware() | Out-Null
$orig = New-Object PT2
[Cur2]::GetCursorPos([ref]$orig) | Out-Null
$cx = [int]([Cur2]::GetSystemMetrics(0) / 2)
"once:"; & "$sp\level.ps1" | Where-Object { $_ -match "'Z'" }
[Cur2]::SetCursorPos($cx, 2) | Out-Null
Start-Sleep -Milliseconds 120
[Cur2]::SetCursorPos($cx + 3, 3) | Out-Null
Start-Sleep -Milliseconds 900
"seride gelince:"; & "$sp\level.ps1" | Where-Object { $_ -match "'Z'" }
[Cur2]::SetCursorPos($orig.X, $orig.Y) | Out-Null
Start-Sleep -Milliseconds 1200
"imlec gidince:"; & "$sp\level.ps1" | Where-Object { $_ -match "'Z'" }
