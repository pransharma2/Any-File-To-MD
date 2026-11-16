; Adds / removes the Explorer right-click entries ("Convert to Markdown").
; The app itself writes the per-user registry keys, so the logic lives in one place (src/shell.rs).

!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\any-file-to-md.exe" --register-shell'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\any-file-to-md.exe" --unregister-shell'
!macroend
