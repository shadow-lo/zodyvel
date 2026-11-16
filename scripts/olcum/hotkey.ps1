param([string]$Key = 'Z')
# Ctrl+Alt+<tuş> gönderir (Zodyvel kısayolu denemesi için).
Add-Type @'
using System.Runtime.InteropServices;
public static class Kb {
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, System.UIntPtr extra);
}
'@
$vk = @{ Z = 0x5A; Enter = 0x0D; Back = 0x08 }[$Key]
$up = 2
[Kb]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
[Kb]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
[Kb]::keybd_event([byte]$vk, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 40
[Kb]::keybd_event([byte]$vk, 0, $up, [UIntPtr]::Zero)
[Kb]::keybd_event(0x12, 0, $up, [UIntPtr]::Zero)
[Kb]::keybd_event(0x11, 0, $up, [UIntPtr]::Zero)
