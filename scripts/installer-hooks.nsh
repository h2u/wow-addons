; Identifies NSIS installations without treating a portable copy as installed.
!macro NSIS_HOOK_POSTINSTALL
  Push $0
  FileOpen $0 "$INSTDIR\.wow-addons-installed" w
  FileWrite $0 "WoW Addons NSIS installation"
  FileClose $0
  Pop $0
!macroend
!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\.wow-addons-installed"
!macroend
