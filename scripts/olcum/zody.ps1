# Mini açıkken Zody'nin CPU payı: A = imleç Zody'nin üzerinde (döngüler 20 adım/sn), B = imleç
# simge çubuğunun boş yerinde (döngüler ~7 adım/sn). İkisinde de imleç 500 ms'de bir 2 px kıpırdar (dinlenme
# 12 sn sonra animasyonları durdurmasın). -Shot ile A sırasında adanın görüntüsü kaydedilir.
param([int]$Seconds = 30, [int]$Rounds = 1, [string]$Shot = '')
$sp = Split-Path $MyInvocation.MyCommand.Path
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System.Runtime.InteropServices;
public struct PT3 { public int X; public int Y; }
public static class Cur3 {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out PT3 p);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
}
'@
[Cur3]::SetProcessDPIAware() | Out-Null
$orig = New-Object PT3
[Cur3]::GetCursorPos([ref]$orig) | Out-Null
$cx = [int]([Cur3]::GetSystemMetrics(0) / 2)
$cores = [Environment]::ProcessorCount
# Fiziksel piksel, %125 ölçek: minide Zody (AI kartı 78 px ya da müzikteki AI alanı 38 px) merkezden
# ~258 mantıksal px solda, ~104 px aşağıda; simge çubuğunun sağ boş kısmı merkezden 150 px sağda, 24 px aşağıda.
$scale = 1.25
$zx = $cx - [int](258 * $scale); $zy = [int](104 * $scale)
$bx = $cx + [int](150 * $scale); $by = [int](24 * $scale)

$all = Get-CimInstance Win32_Process
$ids = @(($all | Where-Object { $_.Name -eq 'zodyvel.exe' }).ProcessId)
do {
  $added = @($all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId })
  $ids += @($added.ProcessId)
} while ($added.Count)
function CpuMs { (Get-Process -Id $ids -ErrorAction SilentlyContinue | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum }
function Phase([int]$x, [int]$y, [string]$label, [bool]$snap) {
  [Cur3]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Seconds 2
  if ($snap -and $Shot) {
    $w = [int](640 * $scale); $h = [int](200 * $scale)
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($cx - [int]($w / 2), 0, 0, 0, $bmp.Size)
    $bmp.Save($Shot); $g.Dispose(); $bmp.Dispose()
  }
  $t0 = CpuMs
  for ($i = 0; $i -lt $Seconds * 2; $i++) { [Cur3]::SetCursorPos($x + ($i % 2) * 2, $y) | Out-Null; Start-Sleep -Milliseconds 500 }
  $t1 = CpuMs
  # Ölçüm sonunda ada hâlâ mini mi (kullanıcı fareyi oynattıysa kapanmış olabilir).
  $open = if (Wide (Level)) { 'mini acik' } else { 'ADA KAPANDI, olcum gecersiz' }
  "{0}: CPU %{1:N2} ({2})" -f $label, (($t1 - $t0) / ($Seconds * 1000 * $cores) * 100), $open
}

# Şeride gel, mini açılsın; sonra aşağı, Zody'ye.
[Cur3]::SetCursorPos($cx, 2) | Out-Null
Start-Sleep -Milliseconds 120
[Cur3]::SetCursorPos($cx + 3, 3) | Out-Null
# Selam (peek) açıksa önce Zody çıkar, mini biraz sonra açılır; "Tıklayınca" kipinde hiç açılmaz: o zaman
# genel kısayolla (Ctrl+Alt+Z) açılır. Bölge mini genişliğine (>500 px) gelene kadar bekle.
function Level { (& "$sp\level.ps1" 6>&1 | Out-String) }
function Wide([string]$lv) { $lv -match "'Z': bolge (\d+)x" -and [int]$Matches[1] -gt 500 }
for ($i = 0; $i -lt 8; $i++) {
  Start-Sleep -Milliseconds 300
  [Cur3]::SetCursorPos($cx + ($i % 2) * 2, 3) | Out-Null
  $lv = Level
  if (Wide $lv) { break }
}
if (-not (Wide $lv)) { & "$sp\hotkey.ps1" Z; Start-Sleep -Milliseconds 900; $lv = Level }
[Cur3]::SetCursorPos($cx, $by) | Out-Null
Start-Sleep -Milliseconds 500
$lv.Trim()
# A ve B sırayla tekrarlanır: arka plandaki ajan olayları iki tarafa eşit dağılsın.
for ($r = 0; $r -lt $Rounds; $r++) {
  Phase $zx $zy 'A (imlec Zody ustunde, 20 adim/sn)' ($r -eq 0)
  Phase $bx $by 'B (imlec cubukta, ~7 adim/sn)' $false
}
& "$sp\level.ps1"
[Cur3]::SetCursorPos($orig.X, $orig.Y) | Out-Null
