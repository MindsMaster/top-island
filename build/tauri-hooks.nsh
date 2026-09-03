; Tauri NSIS 钩子。自更新走 /S 静默安装，静默卸载（更新场景）不提示、不清数据。
!macro NSIS_HOOK_PREUNINSTALL
  ${IfNot} ${Silent}
    SetShellVarContext current

    ; 应用内自启登记的值名（infra/autolaunch.rs 的 VALUE_NAME）
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "TopIsland"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "TopIsland"

    MessageBox MB_YESNO|MB_ICONQUESTION \
      "是否同时删除用户数据（设置、待办、剪贴板历史等）？" \
      /SD IDNO IDYES deleteData IDNO skipData

    deleteData:
      RMDir /r "$APPDATA\top-island"
    skipData:
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend
