; Tauri NSIS 钩子。两件事：卸载时的数据清理，以及老 Electron 版（0.0.1）的一次性迁移。
;
; 本文件必须存成 UTF-8 with BOM：没有 BOM 时 makensis 按系统代码页读，
; 下面的中文提示进到安装器里就是乱码（makensis 日志会打 "(ACP)"）。
;
; 另注意：本文件被模板在顶部 !include，那时 ${PRODUCTNAME} / $PassiveMode 还没定义，
; 所有逻辑都必须写在宏体里（宏体在 Section 内展开时才求值），顶层 !define 只放字面量。

!define UNINST_ROOT "Software\Microsoft\Windows\CurrentVersion\Uninstall"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define STARTUP_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
; 老版 appId com.topisland.app 对应的 electron-builder GUID
; （UUIDv5，namespace 50e065bc-3134-11e6-9bab-38c9862bdaf3）。
; 它是 Software\<GUID> 安装信息键的名字，清残留用；识别老版不靠它，见 LegacyScan。
!define LEGACY_GUID "8d1c5a58-b3e5-5de8-8173-817103161a87"
; 老版自启值名由 Electron 的 app.setLoginItemSettings 生成，和新版的值名不同
!define LEGACY_RUN_VALUE "electron.app.TopIsland"
; 老版的 userData 目录名（%APPDATA% 下，Electron 按 package.json 的 name 建），
; 身份校验用：认老版不能只看显示名，见 LegacyScan
!define LEGACY_DATA_DIR "top-island"

Var LegacyDir ; 老版安装目录，空 = 机器上没有老版
Var LegacyKey ; 老版卸载键的完整路径（相对 hive）
Var LegacyMachine ; 1 = 老版装在 HKLM（全用户）

; 去掉首尾引号和结尾反斜杠，注册表里的路径两种写法都有
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

; 扫 ${ROOT} 下的 Uninstall 子键找老版，命中就填 $LegacyDir / $LegacyKey。
;
; electron-builder 的卸载键名不是 appId，而是 appId 的 UUIDv5，所以按 appId
; （com.topisland.app）去 ReadRegStr 永远读不到——迁移链路一直没触发就是这个原因。
; 这里不赌 GUID 算得对，直接按 DisplayName + 老卸载器文件名认：
; 新版自己的卸载器叫 uninstall.exe，只有老版叫 "Uninstall TopIsland.exe"。
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
    ; 安装目录：老版把 InstallLocation 写在 Software\<GUID>，卸载键里不一定有，
    ; 都读不到就取卸载器路径的父目录
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
    ; 身份校验。认错的代价是 RMDir /r 掉别人的安装目录，所以显示名和卸载器
    ; 文件名对上还不够——别人也可以做个叫 TopIsland 的 electron-builder 应用。
    ; 再要求两条：目录里确实是个 Electron 应用（resources\app.asar），
    ; 且有一条只可能属于我们的痕迹（appId 派生的 Software\<GUID>，或我们的数据目录）。
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

; 删仍指向老 exe 的快捷方式。新老快捷方式同名（TopIsland.lnk），
; 新版装完已经覆盖过，所以必须按目标判断，不然会把新的删掉。
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

  ; 模板只从 Software\<publisher>\<product> 恢复上次的安装位置，publisher 改名后
  ; 那个键就读不到了，自更新会把应用搬到默认目录、把老目录留成孤儿。
  ; 卸载键名不含 publisher，拿它兜底。
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

  ; 老版按用户安装时键在 HKCU，选了「所有用户」则在 HKLM 的 64 位视图
  ; （electron-builder 对 64 位应用会 SetRegView 64，Tauri 模板全程不切视图）
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

    ; 老进程占着老目录，模板的 CheckIfAppIsRunning 只认新二进制名
    ${If} ${FileExists} "$LegacyDir\${PRODUCTNAME}.exe"
      nsExec::ExecToStack 'taskkill /F /IM ${PRODUCTNAME}.exe'
      Pop $9
      Pop $9
    ${EndIf}

    ; 装回老版所在目录：老用户在向导里挑过位置，自动更新不能把应用搬去别处。
    ; 静默/被动安装没有目录页，用户没得选，一律接管；向导安装只在用户没改过
    ; 默认目录时接管。已经装了新版的目录不动，老目录不可写也不动
    ; （老版装在 Program Files 时本安装器是 currentUser，没有管理员权限）。
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
        ; 装进同一个目录就没法在装完后整目录删老版了，先清空
        ; （安装目录里没有用户数据，数据在 %APPDATA%\top-island）
        SetOutPath $INSTDIR
        ; 模板进 Section 时已经建过默认目录，空的就收掉
        RMDir $9
        DetailPrint "沿用老版安装目录：$INSTDIR"
      ${Else}
        DetailPrint "老版目录不可写，安装到 $INSTDIR"
      ${EndIf}
    ${EndIf}

    ; 装进老版所在目录（刚接管过来的，或者本来就是同一个目录）时先整目录清空：
    ; 不清的话 Electron 那一整套文件和老卸载器会一直赖在里面。
    ; 安装目录不存用户数据，数据在 %APPDATA%\top-island。
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

  ; 被动更新会拉起旧卸载器，MUI_UNGETLANGUAGE 靠这个值识别语言，
  ; 缺了它每次自更新都会弹语言选择框
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "Installer Language" $LANGUAGE

  ${If} $LegacyDir != ""
    ; 老目录和新目录不同才删，相同说明新版就装在这里（PREINSTALL 已经清空过）
    ${If} $LegacyDir != $INSTDIR
    ${AndIf} ${FileExists} "$LegacyDir\Uninstall ${PRODUCTNAME}.exe"
      RMDir /r "$LegacyDir"
    ${EndIf}

    !insertmacro LegacyDeleteShortcuts current
    ${If} $LegacyMachine = 1
      !insertmacro LegacyDeleteShortcuts all
    ${EndIf}

    ; 自启：老版值名 electron.app.TopIsland，新版是 TopIsland（infra/autolaunch.rs），
    ; 不删就会留一条指向老 exe 的开机启动项；新版首启按 store.json 的 autoLaunch 重写
    DeleteRegValue HKCU "${RUN_KEY}" "${LEGACY_RUN_VALUE}"
    DeleteRegValue HKCU "${STARTUP_APPROVED_KEY}" "${LEGACY_RUN_VALUE}"

    ; electron-updater 的下载缓存
    RMDir /r "$LOCALAPPDATA\top-island-updater"

    ; 卸载键 + electron-builder 的安装信息键，删了「应用和功能」里才不会留一条老记录
    ${If} $LegacyMachine = 1
      SetRegView 64
      DeleteRegKey HKLM "$LegacyKey"
      DeleteRegKey HKLM "Software\${LEGACY_GUID}"
      SetRegView 32
    ${Else}
      DeleteRegKey HKCU "$LegacyKey"
      DeleteRegKey HKCU "Software\${LEGACY_GUID}"
    ${EndIf}

    ; 从 Electron 自更新过来这一趟没人拉起新版：electron-updater 传的是
    ; `--updated /S --force-run`，模板的 .onInstSuccess 只认自家的 /R。
    ; 向导安装交给完成页，别和它抢。
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
  ; 自更新期间卸载器会被拉起来（/S 静默、/P 被动、/UPDATE 更新），
  ; 这些场景一律不提示、不动数据，只有用户手动卸载才清理
  ${If} ${Silent}
  ${OrIf} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
  ${Else}
    SetShellVarContext current

    ; 应用内自启登记的值名（infra/autolaunch.rs 的 VALUE_NAME）
    DeleteRegValue HKCU "${RUN_KEY}" "${BUNDLEID}"
    DeleteRegValue HKCU "${STARTUP_APPROVED_KEY}" "${BUNDLEID}"
    ; 0.0.2 及以前用的是产品名，装过那几版的机器上还留着
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
