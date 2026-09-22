; Tauri NSIS 钩子 卸载数据清理和老 Electron 版一次性迁移
;
; 须存 UTF-8 with BOM 否则 makensis 按 ACP 读 中文提示变乱码
;
; 本文件在顶部被 !include 时 ${PRODUCTNAME} 等未定义 逻辑须写宏体内

!define UNINST_ROOT "Software\Microsoft\Windows\CurrentVersion\Uninstall"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define STARTUP_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
; 老版 appId 的 electron-builder UUIDv5 Software GUID 键名 清残留用 识别不靠它
!define LEGACY_GUID "8d1c5a58-b3e5-5de8-8173-817103161a87"
; 老版自启值名由 setLoginItemSettings 生成 与新版不同
!define LEGACY_RUN_VALUE "electron.app.TopIsland"
; 老版 userData 目录名 Electron 按 package.json name 建 身份校验用
!define LEGACY_DATA_DIR "top-island"

Var LegacyDir ; 老版安装目录 空即无老版
Var LegacyKey ; 老版卸载键路径 相对 hive
Var LegacyMachine ; 1 = 老版在 HKLM

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

; 扫 ROOT 下 Uninstall 键找老版 命中填 LegacyDir LegacyKey
; electron-builder 卸载键名是 appId 的 UUIDv5 按 appId 读不到
; 按 DisplayName 加卸载器文件名认 只有老版叫 Uninstall TopIsland.exe
!macro LegacyScan ROOT
  StrCpy $8 0
  legacy_scan_next_${ROOT}:
    EnumRegKey $9 ${ROOT} "${UNINST_ROOT}" $8
    StrCmp $9 "" legacy_scan_end_${ROOT}
    IntOp $8 $8 + 1
    ReadRegStr $7 ${ROOT} "${UNINST_ROOT}\$9" "DisplayName"
    StrCmp $7 "${PRODUCTNAME}" 0 legacy_scan_next_${ROOT}
    ReadRegStr $7 ${ROOT} "${UNINST_ROOT}\$9" "UninstallString"
    ${StrLoc} $6 $7 "Uninstall ${PRODUCTNAME}.exe" ">"
    StrCmp $6 "" legacy_scan_next_${ROOT}
    ; InstallLocation 老版写在 Software GUID 键 兜底取卸载器父目录
    ReadRegStr $6 ${ROOT} "${UNINST_ROOT}\$9" "InstallLocation"
    !insertmacro NormalizePath $6
    ${If} $6 == ""
      ReadRegStr $6 ${ROOT} "Software\${LEGACY_GUID}" "InstallLocation"
      !insertmacro NormalizePath $6
    ${EndIf}
    ${If} $6 == ""
      !insertmacro NormalizePath $7
      ${GetParent} $7 $6
    ${EndIf}
    ; 身份校验 认错会误删别人目录
    ; 要求 Electron 应用加一条只属我们的痕迹
    ${IfNot} ${FileExists} "$6\resources\app.asar"
      DetailPrint "$6 里没有 Electron 应用，不认作老版"
      Goto legacy_scan_next_${ROOT}
    ${EndIf}
    ReadRegStr $5 ${ROOT} "Software\${LEGACY_GUID}" "InstallLocation"
    ${If} $5 == ""
    ${AndIfNot} ${FileExists} "$APPDATA\${LEGACY_DATA_DIR}\store.json"
      DetailPrint "$6 认不出是我们的老版，不动它"
      Goto legacy_scan_next_${ROOT}
    ${EndIf}
    StrCpy $LegacyKey "${UNINST_ROOT}\$9"
    StrCpy $LegacyDir $6
  legacy_scan_end_${ROOT}:
!macroend

; 删指向老 exe 的快捷方式 同名须按目标判断
!macro LegacyDeleteShortcuts CTX
  SetShellVarContext ${CTX}
  !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$LegacyDir\${PRODUCTNAME}.exe"
  Pop $9
  ${If} $9 = 1
    Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  ${EndIf}
  !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$LegacyDir\${PRODUCTNAME}.exe"
  Pop $9
  ${If} $9 = 1
    Delete "$DESKTOP\${PRODUCTNAME}.lnk"
  ${EndIf}
  SetShellVarContext current
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

  StrCpy $LegacyDir ""
  StrCpy $LegacyKey ""
  StrCpy $LegacyMachine 0

  ; 老版全用户装在 HKLM 64 位视图 本模板不切视图
  !insertmacro LegacyScan HKCU
  ${If} $LegacyDir == ""
    SetRegView 64
    !insertmacro LegacyScan HKLM
    SetRegView 32
    ${If} $LegacyDir != ""
      StrCpy $LegacyMachine 1
    ${EndIf}
  ${EndIf}

  ${If} $LegacyDir != ""
    DetailPrint "检测到老版本：$LegacyDir"

    ; 模板只认新二进制名 须手动杀老进程
    ${If} ${FileExists} "$LegacyDir\${PRODUCTNAME}.exe"
      nsExec::ExecToStack 'taskkill /F /IM ${PRODUCTNAME}.exe'
      Pop $9
      Pop $9
    ${EndIf}

    ; 接管老版所在目录 静默被动一律接管 向导只在未改默认目录时接管
    ; 已装新版的目录和老目录不可写时不动
    StrCpy $7 0
    ${If} ${Silent}
    ${OrIf} $PassiveMode = 1
      StrCpy $7 1
    ${ElseIf} $INSTDIR == "$LOCALAPPDATA\${PRODUCTNAME}"
      StrCpy $7 1
    ${EndIf}
    ${If} $7 = 1
    ${AndIf} $LegacyDir != $INSTDIR
    ${AndIfNot} ${FileExists} "$INSTDIR\uninstall.exe"
      ClearErrors
      FileOpen $8 "$LegacyDir\.topisland-install-probe" w
      ${IfNot} ${Errors}
        FileClose $8
        Delete "$LegacyDir\.topisland-install-probe"
        StrCpy $9 $INSTDIR
        StrCpy $INSTDIR $LegacyDir
        ; 装进老目录先清空 用户数据在 APPDATA 不在此
        SetOutPath $INSTDIR
        ; 收掉模板已建的空默认目录
        RMDir $9
        DetailPrint "沿用老版安装目录：$INSTDIR"
      ${Else}
        DetailPrint "老版目录不可写，安装到 $INSTDIR"
      ${EndIf}
    ${EndIf}

    ; INSTDIR 里有老卸载器就整目录清空 数据在 APPDATA 不在此
    ${If} ${FileExists} "$INSTDIR\Uninstall ${PRODUCTNAME}.exe"
      RMDir /r "$INSTDIR"
      CreateDirectory "$INSTDIR"
      SetOutPath $INSTDIR
      DetailPrint "已清掉老版文件：$INSTDIR"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current

  ; MUI_UNGETLANGUAGE 靠此值识别语言 缺了自更新会弹语言框
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "Installer Language" $LANGUAGE

  ${If} $LegacyDir != ""
    ; 新老目录不同才删 相同即新版已装在此
    ${If} $LegacyDir != $INSTDIR
    ${AndIf} ${FileExists} "$LegacyDir\Uninstall ${PRODUCTNAME}.exe"
      RMDir /r "$LegacyDir"
    ${EndIf}

    !insertmacro LegacyDeleteShortcuts current
    ${If} $LegacyMachine = 1
      !insertmacro LegacyDeleteShortcuts all
    ${EndIf}

    ; 老自启值指向老 exe 须删 新版值名在 infra/autolaunch.rs
    DeleteRegValue HKCU "${RUN_KEY}" "${LEGACY_RUN_VALUE}"
    DeleteRegValue HKCU "${STARTUP_APPROVED_KEY}" "${LEGACY_RUN_VALUE}"

    ; electron-updater 的下载缓存
    RMDir /r "$LOCALAPPDATA\top-island-updater"

    ; 删卸载键和安装信息键 应用和功能不留老记录
    ${If} $LegacyMachine = 1
      SetRegView 64
      DeleteRegKey HKLM "$LegacyKey"
      DeleteRegKey HKLM "Software\${LEGACY_GUID}"
      SetRegView 32
    ${Else}
      DeleteRegKey HKCU "$LegacyKey"
      DeleteRegKey HKCU "Software\${LEGACY_GUID}"
    ${EndIf}

    ; electron-updater 传 --updated /S --force-run 模板只认 /R 这里补拉起
    ; 向导安装交给完成页
    ${If} ${Silent}
    ${OrIf} $PassiveMode = 1
      ${GetOptions} $CMDLINE "/R" $9
      ${If} ${Errors}
        nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
      ${EndIf}
    ${EndIf}
  ${EndIf}
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
