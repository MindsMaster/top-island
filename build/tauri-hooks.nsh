; Tauri NSIS 钩子 卸载数据清理
;
; 须存 UTF-8 with BOM 否则 makensis 按 ACP 读 中文提示变乱码
;
; 本文件在顶部被 !include 时 ${PRODUCTNAME} 等未定义 逻辑须写宏体内

!define UNINST_ROOT "Software\Microsoft\Windows\CurrentVersion\Uninstall"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define STARTUP_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"

; 去首尾引号和结尾反斜杠 注册表两种写法都有
!macro NormalizePath VAR
  StrCpy $5 ${VAR} 1
  ${If} $5 == '"'
    StrCpy ${VAR} ${VAR} "" 1
    StrCpy $5 ${VAR} 1 -1
    ${If} $5 == '"'
      StrCpy ${VAR} ${VAR} -1
    ${EndIf}
  ${EndIf}
  StrCpy $5 ${VAR} 1 -1
  ${If} $5 == "\"
    StrCpy ${VAR} ${VAR} -1
  ${EndIf}
!macroend

; 卸载时清网易云目录的 bridge 代理 只认带标记文件的目录
; TAG 令标号唯一 同一 hive 不同视图会调两次
!macro RevertNcmBridge ROOT TAG
  StrCpy $2 0
  ncm_revert_next_${TAG}:
    EnumRegKey $3 ${ROOT} "${UNINST_ROOT}" $2
    StrCmp $3 "" ncm_revert_end_${TAG}
    IntOp $2 $2 + 1
    ReadRegStr $4 ${ROOT} "${UNINST_ROOT}\$3" "InstallLocation"
    !insertmacro NormalizePath $4
    StrCmp $4 "" ncm_revert_next_${TAG}
    ${IfNot} ${FileExists} "$4\msimg32.dll.topisland"
      Goto ncm_revert_next_${TAG}
    ${EndIf}
    Delete /REBOOTOK "$4\msimg32.dll"
    Delete /REBOOTOK "$4\msimg32_original.dll"
    Delete /REBOOTOK "$4\msimg32.dll.topisland"
    Delete /REBOOTOK "$4\msimg32.dll.new"
    DetailPrint "已清理网易云音乐增强：$4"
    Goto ncm_revert_next_${TAG}
  ncm_revert_end_${TAG}:
!macroend

!macro NSIS_HOOK_PREINSTALL
  SetShellVarContext current

  ; publisher 改名后模板读不到旧安装位置 用卸载键兜底
  ${If} $INSTDIR == "$LOCALAPPDATA\${PRODUCTNAME}"
  ${AndIfNot} ${FileExists} "$INSTDIR\uninstall.exe"
    ReadRegStr $9 SHCTX "${UNINSTKEY}" "InstallLocation"
    !insertmacro NormalizePath $9
    ${If} $9 != ""
    ${AndIf} $9 != $INSTDIR
    ${AndIf} ${FileExists} "$9\uninstall.exe"
      StrCpy $8 $INSTDIR
      StrCpy $INSTDIR $9
      SetOutPath $INSTDIR
      RMDir $8
      DetailPrint "沿用上一次的安装目录：$INSTDIR"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current

  ; MUI_UNGETLANGUAGE 靠此值识别语言 缺了自更新会弹语言框
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "Installer Language" $LANGUAGE
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 自更新拉起的卸载不提示不动数据 只手动卸载才清理
  ${If} ${Silent}
  ${OrIf} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
  ${Else}
    SetShellVarContext current

    !insertmacro RevertNcmBridge HKCU hkcu
    SetRegView 64
    !insertmacro RevertNcmBridge HKLM hklm64
    SetRegView 32
    !insertmacro RevertNcmBridge HKLM hklm32

    ; 自启值名见 infra/autolaunch.rs
    DeleteRegValue HKCU "${RUN_KEY}" "${BUNDLEID}"
    DeleteRegValue HKCU "${STARTUP_APPROVED_KEY}" "${BUNDLEID}"
    ; 0.0.2 前自启值名是产品名
    DeleteRegValue HKCU "${RUN_KEY}" "${PRODUCTNAME}"
    DeleteRegValue HKCU "${STARTUP_APPROVED_KEY}" "${PRODUCTNAME}"

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
