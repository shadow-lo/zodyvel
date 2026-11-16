# Ada penceresinin bölge kutusu (fiziksel piksel): hap ~230x40, açık ada daha büyük.
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public struct RC { public int L, T, R, B; }
public static class Wn {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("gdi32.dll")] public static extern IntPtr CreateRectRgn(int a, int b, int c, int d);
  [DllImport("user32.dll")] public static extern int GetWindowRgn(IntPtr h, IntPtr r);
  [DllImport("gdi32.dll")] public static extern int GetRgnBox(IntPtr r, out RC b);
  [DllImport("gdi32.dll")] public static extern bool DeleteObject(IntPtr o);
}
'@
$zid = (Get-Process zodyvel).Id
[Wn]::EnumWindows({ param($h, $l)
  $p = 0; [Wn]::GetWindowThreadProcessId($h, [ref]$p) | Out-Null
  if ($p -eq $zid -and [Wn]::IsWindowVisible($h)) {
    $sb = New-Object System.Text.StringBuilder 128; [Wn]::GetWindowTextW($h, $sb, 128) | Out-Null
    $r = [Wn]::CreateRectRgn(0, 0, 0, 0); $kind = [Wn]::GetWindowRgn($h, $r); $b = New-Object RC; [Wn]::GetRgnBox($r, [ref]$b) | Out-Null; [Wn]::DeleteObject($r) | Out-Null
    Write-Host ("pencere '{0}': bolge {1}x{2} (tur {3})" -f $sb, ($b.R - $b.L), ($b.B - $b.T), $kind)
  }
  $true }, [IntPtr]::Zero) | Out-Null

