; Kaldırılınca Zodyvel'in "Windows ile başlat" kaydı da silinir (uygulama kendisi yazar, kurulum değil).
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Zodyvel"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Zodyvel"
!macroend
