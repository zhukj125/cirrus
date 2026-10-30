!macro StopCirrusMcpSidecar
  DetailPrint "Stopping Cirrus MCP sidecar..."
  nsExec::ExecToLog 'taskkill /F /IM cirrus-mcp.exe'
  Pop $0
  Sleep 400
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro StopCirrusMcpSidecar
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro StopCirrusMcpSidecar
!macroend
