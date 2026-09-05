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

; 老 Electron 版一次性迁移清理。迁移标志是老 appId 的卸载键存在，
; Tauri 自身更新不会写这个键，天然和被动自更新区分开
!define LEGACY_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.topisland.app"

!macro NSIS_HOOK_PREINSTALL
  SetShellVarContext current
  ReadRegStr $1 HKCU "${LEGACY_UNINSTALL_KEY}" "UninstallString"
  ${If} $1 != ""
    ; 老进程可能还占着老目录，先杀（模板只检测新二进制名）
    nsExec::ExecToStack 'taskkill /F /IM TopIsland.exe'
    Pop $0
    Pop $0
    ; 静默/被动迁移沿用老版安装位置。向导流程由用户在目录页自己选
    ${If} ${Silent}
    ${OrIf} $PassiveMode == 1
      ReadRegStr $0 HKCU "${LEGACY_UNINSTALL_KEY}" "InstallLocation"
      ${If} $0 != ""
      ${AndIf} ${FileExists} "$0\Uninstall TopIsland.exe"
        StrCpy $INSTDIR $0
        SetOutPath $INSTDIR
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current

  ; 被动更新会拉起旧卸载器，MUI_UNGETLANGUAGE 靠这个值识别语言，
  ; 缺了它每次自更新都会弹语言选择框
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "Installer Language" $LANGUAGE

  ReadRegStr $1 HKCU "${LEGACY_UNINSTALL_KEY}" "UninstallString"
  ${If} $1 != ""
    ReadRegStr $0 HKCU "${LEGACY_UNINSTALL_KEY}" "InstallLocation"
    ${If} $0 == ""
      StrCpy $0 "$LOCALAPPDATA\Programs\TopIsland"
    ${EndIf}
    ; 老目录和新目录不同才删，相同说明新版就装在这里
    ${If} $0 != $INSTDIR
    ${AndIf} ${FileExists} "$0\Uninstall TopIsland.exe"
      RMDir /r "$0"
    ${EndIf}

    ; 自启值名新老相同，新版首启会按设置重写
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "TopIsland"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "TopIsland"
    RMDir /r "$LOCALAPPDATA\top-island-updater"
    DeleteRegKey HKCU "${LEGACY_UNINSTALL_KEY}"

    ; 迁移装完无条件拉起新版，向导的完成页复选框靠不住
    Exec '"$INSTDIR\island-app.exe"'
  ${EndIf}
!macroend
