; Assisted installer. Auto-update runs the uninstaller with ${isUpdated} set — never prompt then.
!macro customUnInstall
  ${ifNot} ${isUpdated}
    SetShellVarContext current

    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP_ID}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT_FILENAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT_NAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${APP_ID}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCT_FILENAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCT_NAME}"

    MessageBox MB_YESNO|MB_ICONQUESTION \
      "是否同时删除用户数据（设置、待办、剪贴板历史等）？" \
      /SD IDNO IDYES deleteData IDNO skipData

    deleteData:
      RMDir /r "$APPDATA\${APP_FILENAME}"
      !ifdef APP_PRODUCT_FILENAME
        RMDir /r "$APPDATA\${APP_PRODUCT_FILENAME}"
      !endif
      !ifdef APP_PACKAGE_NAME
        RMDir /r "$APPDATA\${APP_PACKAGE_NAME}"
        RMDir /r "$LOCALAPPDATA\${APP_PACKAGE_NAME}-updater"
      !endif
      ${If} "$LOCALAPPDATA\${APP_FILENAME}" != "$INSTDIR"
        RMDir /r "$LOCALAPPDATA\${APP_FILENAME}"
      ${EndIf}
      RMDir /r "$LOCALAPPDATA\${APP_FILENAME}-updater"
    skipData:
  ${endIf}
!macroend
