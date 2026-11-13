; P2 (Rust) Windows kurulum programı — istemci ("P2 Client") ve sunucu konsolu ("P2 Server").
;
; electron-builder 26.15.3'ün NSIS şablonlarının (oneClick:false, perMachine:false,
; allowToChangeInstallationDirectory, createDesktopShortcut:always) tek geçişli karşılığı. Aynı
; kayıt anahtarları (Software\<APP_GUID>, ...\Uninstall\<APP_GUID>), aynı kurulum klasörü
; (%LOCALAPPDATA%\Programs\<ürün> ya da Program Files\<ürün>), aynı kısayollar (AppUserModelID
; ile), aynı komut satırı (`--updated`, `/S`, `--force-run`, `/D=`, `/allusers`, `/currentuser`,
; `--keep-shortcuts`, `--delete-app-data`, `--no-desktop-shortcut`). Böylece Electron kurulumunun
; üzerine yerinde yükseltir: eski kaldırıcı `--updated` ile çağrılır (kullanıcı verisi silinmez,
; kısayollar korunur), sonra yeni dosyalar yazılır.
;
; Farklar (bilerek): kaldırıcı derleme anında üretilmez, kurulumda `WriteUninstaller` ile yazılır
; (electron-builder kaldırıcıyı üretmek için ara bir exe çalıştırır); uygulama 7z paketi yerine
; doğrudan katı LZMA ile gömülür; "kimin için kurulsun" sayfası yoktur (var olan kurulumun kipi
; izlenir, yoksa kullanıcıya kurulur; /allusers ile makineye); diller Türkçe ve İngilizce.
;
; Derleme: rust/packaging/build-installers.mjs (tanımları o verir; elle: README).

Unicode true
ManifestDPIAware true
ManifestSupportedOS all
SetCompressor /SOLID lzma
SetCompressorDictSize 64
RequestExecutionLevel user
FileBufSize 64

!addplugindir /x86-unicode "${PLUGINS_DIR}"
!addincludedir "${EB_INCLUDE_DIR}"

!include StdUtils.nsh
!include x64.nsh
!include WinVer.nsh
!include LogicLib.nsh
!include FileFunc.nsh
!include WordFunc.nsh
!include UAC.nsh
!include StrContains.nsh

; ------------------------------------------------------------------ kimlik

!define APP_EXECUTABLE_FILENAME "${PRODUCT_FILENAME}.exe"
!define UNINSTALL_FILENAME "Uninstall ${PRODUCT_FILENAME}.exe"
!define INSTALL_REGISTRY_KEY "Software\${APP_GUID}"
!define UNINSTALL_REGISTRY_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}"
!define FOLDERID_UserProgramFiles {5CD7AEE2-2219-4A67-B85D-6C9CE15660CB}
!define KF_FLAG_CREATE 0x00008000

Name "${PRODUCT_NAME}"
BrandingText "${PRODUCT_NAME} ${VERSION}"
OutFile "${OUTFILE}"
ShowInstDetails nevershow
ShowUninstDetails nevershow

VIProductVersion "${VERSION_QUAD}"
VIAddVersionKey /LANG=1033 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "CompanyName" "${COMPANY_NAME}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Copyright © ${COPYRIGHT_YEAR} ${COMPANY_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${APP_DESCRIPTION}"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"

; ------------------------------------------------------------------ komut satırı (electron-builder ile aynı)

!macro _isUpdated _a _b _t _f
  ${StdUtils.TestParameter} $R9 "updated"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isUpdated `"" isUpdated ""`

!macro _isForceRun _a _b _t _f
  ${StdUtils.TestParameter} $R9 "force-run"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isForceRun `"" isForceRun ""`

!macro _isKeepShortcuts _a _b _t _f
  ${StdUtils.TestParameter} $R9 "keep-shortcuts"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isKeepShortcuts `"" isKeepShortcuts ""`

!macro _isNoDesktopShortcut _a _b _t _f
  ${StdUtils.TestParameter} $R9 "no-desktop-shortcut"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isNoDesktopShortcut `"" isNoDesktopShortcut ""`

!macro _isDeleteAppData _a _b _t _f
  ${StdUtils.TestParameter} $R9 "delete-app-data"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isDeleteAppData `"" isDeleteAppData ""`

!macro _isForAllUsers _a _b _t _f
  ${StdUtils.TestParameter} $R9 "allusers"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isForAllUsers `"" isForAllUsers ""`

!macro _isForCurrentUser _a _b _t _f
  ${StdUtils.TestParameter} $R9 "currentuser"
  StrCmp "$R9" "true" `${_t}` `${_f}`
!macroend
!define isForCurrentUser `"" isForCurrentUser ""`

; ------------------------------------------------------------------ değişkenler

Var installMode
Var hasPerUserInstallation
Var hasPerMachineInstallation
Var perUserInstallationFolder
Var perMachineInstallationFolder
Var newStartMenuLink
Var oldStartMenuLink
Var newDesktopLink
Var oldDesktopLink
Var oldShortcutName
Var oldMenuDirectory
Var appExe
Var launchLink
Var keepShortcuts
Var isTryToKeepShortcuts
Var selfPid

; ------------------------------------------------------------------ arayüz (MUI2)

!include MUI2.nsh
!define MUI_ICON "${INSTALLER_ICON}"
!define MUI_UNICON "${INSTALLER_ICON}"
!define MUI_WELCOMEFINISHPAGE_BITMAP "${NSISDIR}\Contrib\Graphics\Wizard\nsis3-metro.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "${NSISDIR}\Contrib\Graphics\Wizard\nsis3-metro.bmp"

Function skipPageIfUpdated
  ${if} ${isUpdated}
    Abort
  ${endif}
FunctionEnd

!define MUI_PAGE_CUSTOMFUNCTION_PRE skipPageIfUpdated
!insertmacro MUI_PAGE_DIRECTORY
; Seçilen klasörün sonunda uygulama alt klasörü yoksa eklenir (electron-builder instFilesPre).
!define MUI_PAGE_CUSTOMFUNCTION_PRE instFilesPre
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION "StartApp"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Turkish"

LangString appRunning ${LANG_ENGLISH} "${PRODUCT_NAME} is running.$\r$\nClick OK to close it.$\r$\nIf it doesn't close, try closing it manually."
LangString appRunning ${LANG_TURKISH} "${PRODUCT_NAME} çalışıyor.$\r$\nKapatmak için Tamam'a tıklayın.$\r$\nKapanmazsa elle kapatmayı deneyin."
LangString appClosing ${LANG_ENGLISH} "Closing running ${PRODUCT_NAME}..."
LangString appClosing ${LANG_TURKISH} "Çalışan ${PRODUCT_NAME} kapatılıyor..."
LangString appCannotBeClosed ${LANG_ENGLISH} "${PRODUCT_NAME} cannot be closed. $\r$\nPlease close it manually and click Retry to continue."
LangString appCannotBeClosed ${LANG_TURKISH} "${PRODUCT_NAME} kapatılamaz. $\r$\nLütfen elle kapatın ve devam etmek için Tekrar'a tıklayın."
LangString uninstallFailed ${LANG_ENGLISH} "Failed to uninstall old application files. Please try running the installer again."
LangString uninstallFailed ${LANG_TURKISH} "Eski uygulama dosyaları kaldırılamadı. Lütfen kurulumu yeniden çalıştırın."
LangString x64WinRequired ${LANG_ENGLISH} "64-bit Windows is required"
LangString x64WinRequired ${LANG_TURKISH} "64 bit Windows gerekir"
LangString win7Required ${LANG_ENGLISH} "Windows 7 and above is required"
LangString win7Required ${LANG_TURKISH} "Windows 7 ya da üstü gerekir"
LangString loginWithAdminAccount ${LANG_ENGLISH} "Please log in with an administrator account to update ${PRODUCT_NAME} installed for all users."
LangString loginWithAdminAccount ${LANG_TURKISH} "Tüm kullanıcılar için kurulmuş ${PRODUCT_NAME} sürümünü güncellemek için yönetici hesabıyla oturum açın."

; ------------------------------------------------------------------ ortak yardımcılar

!macro check64BitAndSetRegView
  ${If} ${IsWin2000}
  ${OrIf} ${IsWinME}
  ${OrIf} ${IsWinXP}
  ${OrIf} ${IsWinVista}
    MessageBox MB_OK "$(win7Required)"
    Quit
  ${EndIf}
  ${If} ${RunningX64}
    SetRegView 64
  ${Else}
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(x64WinRequired)"
    Quit
  ${EndIf}
!macroend

!macro quitSuccess
  SetErrorLevel 0
  Quit
!macroend

; /D= boşluk içerebilir ve tırnaksızdır; NSIS kuralı gereği en sondadır (electron-builder GetDParameter).
!macro GetDParameter outVar
  Push $R8
  Push $R9
  Push $R7
  Push $R6
  Push $R5
  ${StdUtils.GetAllParameters} $R8 "0"
  StrCpy $R9 ""
  StrLen $R7 $R8
  IntOp $R7 $R7 - 2
  StrCpy $R6 0
  ${Do}
    StrCpy $R5 $R8 3 $R6
    ${If} $R5 == "/D="
    ${OrIf} $R5 == "/d="
      IntOp $R6 $R6 + 3
      StrCpy $R9 $R8 "" $R6
      ${Break}
    ${EndIf}
    IntOp $R6 $R6 + 1
  ${LoopUntil} $R6 > $R7
  StrCpy ${outVar} $R9
  Pop $R5
  Pop $R6
  Pop $R7
  Pop $R9
  Pop $R8
!macroend

!macro setInstallModePerUser
  StrCpy $installMode CurrentUser
  SetShellVarContext current
  ReadRegStr $perUserInstallationFolder HKCU "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${if} $perUserInstallationFolder != ""
    StrCpy $INSTDIR $perUserInstallationFolder
  ${else}
    StrCpy $0 "$LocalAppData\Programs"
    Push $1
    Push $2
    StrCpy $2 0
    System::Call 'SHELL32::SHGetKnownFolderPath(g "${FOLDERID_UserProgramFiles}", i ${KF_FLAG_CREATE}, p 0, *p .r2)i.r1'
    ${If} $1 == 0
      System::Call 'KERNEL32::lstrcpynW(w .r0, p r2, i ${NSIS_MAX_STRLEN})p'
    ${endif}
    ${If} $2 != 0
      System::Call 'OLE32::CoTaskMemFree(p r2)'
    ${endif}
    Pop $2
    Pop $1
    StrCpy $INSTDIR "$0\${APP_FILENAME}"
  ${endif}
  !insertmacro GetDParameter $R0
  ${If} $R0 != ""
    StrCpy $INSTDIR $R0
  ${endif}
!macroend

!macro setInstallModePerAllUsers
  StrCpy $installMode all
  SetShellVarContext all
  ReadRegStr $perMachineInstallationFolder HKLM "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${if} $perMachineInstallationFolder != ""
    StrCpy $INSTDIR $perMachineInstallationFolder
  ${else}
    StrCpy $INSTDIR "$PROGRAMFILES64\${APP_FILENAME}"
  ${endif}
  !insertmacro GetDParameter $R0
  ${If} $R0 != ""
    StrCpy $INSTDIR $R0
  ${endif}
!macroend

; Var olan kurulumun kipini bulur (multiUser.nsh initMultiUser). Kullanıcıya ve makineye kurulum
; ikisi de varsa ya da hiçbiri yoksa kullanıcıya kurulur.
!macro initMultiUser
  StrCpy $hasPerMachineInstallation "0"
  StrCpy $hasPerUserInstallation "0"
  ReadRegStr $perMachineInstallationFolder HKLM "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${if} $perMachineInstallationFolder != ""
    StrCpy $hasPerMachineInstallation "1"
  ${endif}
  ReadRegStr $perUserInstallationFolder HKCU "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${if} $perUserInstallationFolder != ""
    StrCpy $hasPerUserInstallation "1"
  ${endif}
  ${if} ${isForAllUsers}
    StrCpy $hasPerMachineInstallation "1"
    StrCpy $hasPerUserInstallation "0"
  ${endif}
  ${if} ${isForCurrentUser}
    StrCpy $hasPerMachineInstallation "0"
    StrCpy $hasPerUserInstallation "1"
  ${endif}
  ${if} $hasPerUserInstallation == "0"
  ${andif} $hasPerMachineInstallation == "1"
    !insertmacro setInstallModePerAllUsers
  ${else}
    !insertmacro setInstallModePerUser
  ${endif}
!macroend

; Makineye kurulum yönetici ister: UAC eklentisiyle yükseltilmiş iç süreç işi yapar, dış süreç
; onun çıkış koduyla çıkar (electron-builder: sessiz yükseltmede kurulum bölümünde, arayüzde kip
; sayfasında; burada ikisi için de açılışta).
!macro elevateIfPerMachine
  ${if} $installMode == "all"
  ${andIfNot} ${UAC_IsAdmin}
    !insertmacro UAC_RunElevated
    ${Switch} $0
      ${Case} 0
        ${If} $1 = 1
          SetErrorLevel $2
          Quit
        ${EndIf}
        ${If} $1 = 3
        ${OrIf} $2 = 0x666666
          MessageBox mb_IconStop|mb_TopMost|mb_SetForeground "$(loginWithAdminAccount)" /SD IDOK
        ${EndIf}
        SetErrorLevel 2
        Quit
      ${Case} 1223
        SetErrorLevel 1223
        Quit
      ${Default}
        MessageBox mb_IconStop|mb_TopMost|mb_SetForeground "Unable to elevate, error $0" /SD IDOK
        SetErrorLevel 2
        Quit
    ${EndSwitch}
  ${endif}
!macroend

!macro setLinkVars
  ReadRegStr $oldShortcutName SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" ShortcutName
  ${if} $oldShortcutName == ""
    StrCpy $oldShortcutName "${PRODUCT_FILENAME}"
  ${endIf}
  StrCpy $oldDesktopLink "$DESKTOP\$oldShortcutName.lnk"
  StrCpy $newDesktopLink "$DESKTOP\${SHORTCUT_NAME}.lnk"
  ReadRegStr $oldMenuDirectory SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" MenuDirectory
  ${if} $oldMenuDirectory == ""
    StrCpy $oldStartMenuLink "$SMPROGRAMS\$oldShortcutName.lnk"
  ${else}
    StrCpy $oldStartMenuLink "$SMPROGRAMS\$oldMenuDirectory\$oldShortcutName.lnk"
  ${endIf}
  StrCpy $newStartMenuLink "$SMPROGRAMS\${SHORTCUT_NAME}.lnk"
!macroend

; Kurulum klasöründen çalışan süreçler (getProcessInfo/allowOnlyOneInstallerInstance karşılığı).
; PowerShell varsa yola göre (kendi süreci hariç), yoksa exe adına göre tasklist. $R0 = 0: bulundu.
!macro FIND_PROCESS _RETURN
  nsExec::Exec `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -C "if ((Get-CimInstance -ClassName Win32_Process | ? {$$_.Path -and $$_.ProcessId -ne $selfPid -and $$_.Path.StartsWith('$INSTDIR\', 'CurrentCultureIgnoreCase')}).Count -gt 0) { exit 0 } else { exit 1 }"`
  Pop ${_RETURN}
  ${if} ${_RETURN} != 0
  ${andIf} ${_RETURN} != 1
    nsExec::Exec `"$SYSDIR\cmd.exe" /C tasklist /FI "USERNAME eq %USERNAME%" /FI "IMAGENAME eq ${APP_EXECUTABLE_FILENAME}" /FO CSV /NH | "$SYSDIR\findstr.exe" /B /I /C:"\"${APP_EXECUTABLE_FILENAME}\""`
    Pop ${_RETURN}
  ${endIf}
!macroend

!macro KILL_PROCESS _FORCE
  ${if} ${_FORCE} == 1
    StrCpy $R2 "-Force"
  ${else}
    StrCpy $R2 ""
  ${endIf}
  nsExec::Exec `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -C "Get-CimInstance -ClassName Win32_Process | ? {$$_.Path -and $$_.ProcessId -ne $selfPid -and $$_.Path.StartsWith('$INSTDIR\', 'CurrentCultureIgnoreCase')} | % { Stop-Process -Id $$_.ProcessId $R2 }"`
  Pop $R3
  ${if} $R3 != 0
    ${if} ${_FORCE} == 1
      nsExec::Exec `"$SYSDIR\cmd.exe" /C taskkill /F /IM "${APP_EXECUTABLE_FILENAME}" /FI "PID ne $selfPid" /FI "USERNAME eq %USERNAME%"`
    ${else}
      nsExec::Exec `"$SYSDIR\cmd.exe" /C taskkill /IM "${APP_EXECUTABLE_FILENAME}" /FI "PID ne $selfPid" /FI "USERNAME eq %USERNAME%"`
    ${endIf}
    Pop $R3
  ${endIf}
!macroend

!macro CHECK_APP_RUNNING
  !define CAR_ID ${__LINE__}
  System::Call 'kernel32::GetCurrentProcessId() i.r0'
  StrCpy $selfPid $0
  ${if} ${isUpdated}
    ; uygulama kendiliğinden kapanabilsin
    Sleep 300
  ${endIf}
  !insertmacro FIND_PROCESS $R0
  ${if} $R0 == 0
    ${if} ${isUpdated}
      Sleep 1000
      Goto doStopProcess_${CAR_ID}
    ${endIf}
    MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "$(appRunning)" /SD IDOK IDOK doStopProcess_${CAR_ID}
    Quit
    doStopProcess_${CAR_ID}:
    DetailPrint "$(appClosing)"
    !insertmacro KILL_PROCESS 0
    Sleep 300
    StrCpy $R1 0
    loop_${CAR_ID}:
      IntOp $R1 $R1 + 1
      !insertmacro FIND_PROCESS $R0
      ${if} $R0 == 0
        Sleep 1000
        !insertmacro KILL_PROCESS 1
        !insertmacro FIND_PROCESS $R0
        ${if} $R0 == 0
          DetailPrint `Waiting for "${PRODUCT_NAME}" to close.`
          Sleep 2000
        ${else}
          Goto not_running_${CAR_ID}
        ${endIf}
      ${else}
        Goto not_running_${CAR_ID}
      ${endIf}
      ${if} $R1 > 1
        MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(appCannotBeClosed)" /SD IDCANCEL IDRETRY loop_${CAR_ID}
        Quit
      ${else}
        Goto loop_${CAR_ID}
      ${endIf}
    not_running_${CAR_ID}:
  ${endIf}
  !undef CAR_ID
!macroend

!macro setIsTryToKeepShortcuts
  StrCpy $isTryToKeepShortcuts "true"
  ${ifNot} ${isUpdated}
    StrCpy $isTryToKeepShortcuts "false"
  ${endIf}
!macroend

; ------------------------------------------------------------------ kurulum

Function .onInit
  !insertmacro check64BitAndSetRegView
  ${IfNot} ${UAC_IsInnerInstance}
    ; Tek kurulum örneği (allowOnlyOneInstallerInstance: muteks adı APP_GUID).
    System::Call 'kernel32::CreateMutex(p 0, i 1, t "${APP_GUID}")?e'
    Pop $0
    ${If} $0 = 183
      BringToFront
      Abort
    ${EndIf}
  ${EndIf}
  ${If} ${UAC_IsInnerInstance}
  ${AndIfNot} ${UAC_IsAdmin}
    SetErrorLevel 0x666666
    Quit
  ${EndIf}
  !insertmacro initMultiUser
  !insertmacro elevateIfPerMachine
  !ifdef UNPACKED_SIZE_KB
    SectionSetSize 0 ${UNPACKED_SIZE_KB}
  !endif
FunctionEnd

Function instFilesPre
  ${StrContains} $0 "${APP_FILENAME}" $INSTDIR
  ${If} $0 == ""
    StrCpy $INSTDIR "$INSTDIR\${APP_FILENAME}"
  ${endIf}
FunctionEnd

Function StartApp
  ${if} ${isUpdated}
    StrCpy $1 "--updated"
  ${else}
    StrCpy $1 ""
  ${endif}
  ${StdUtils.ExecShellAsUser} $0 "$launchLink" "open" "$1"
FunctionEnd

; Eski sürümün kaldırıcısını `--updated` ile çalıştırır (installUtil.nsh uninstallOldVersion).
; Electron kurulumunun kaldırıcısı da bu sözleşmeyle çalışır: veri silinmez, `--keep-shortcuts`
; verilirse kısayollara dokunulmaz, `_?=` ile kendini kopyalamadan kurulum klasöründe kalır.
Var uninstallString
Var uninstallerFileName
Var uninstallerFileNameTemp
Var installationDir
Function GetInQuotes
  Exch $R0
  Push $R1
  Push $R2
  Push $R3
  StrCpy $R2 -1
  IntOp $R2 $R2 + 1
  StrCpy $R3 $R0 1 $R2
  StrCmp $R3 "" 0 +3
  StrCpy $R0 ""
  Goto Done
  StrCmp $R3 '"' 0 -5
  IntOp $R2 $R2 + 1
  StrCpy $R0 $R0 "" $R2
  StrCpy $R2 0
  IntOp $R2 $R2 + 1
  StrCpy $R3 $R0 1 $R2
  StrCmp $R3 "" 0 +3
  StrCpy $R0 ""
  Goto Done
  StrCmp $R3 '"' 0 -5
  StrCpy $R0 $R0 $R2
  Done:
  Pop $R3
  Pop $R2
  Pop $R1
  Exch $R0
FunctionEnd

Function GetFileParent
  Exch $R0
  Push $R1
  Push $R2
  Push $R3
  StrCpy $R1 0
  StrLen $R2 $R0
  loop:
    IntOp $R1 $R1 + 1
    IntCmp $R1 $R2 get 0 get
    StrCpy $R3 $R0 1 -$R1
    StrCmp $R3 "\" get
  Goto loop
  get:
    StrCpy $R0 $R0 -$R1
    Pop $R3
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

; $0: "HKCU" ya da "SHELL" — hangi kökteki kurulum kaldırılacak. Sonuç: $R0 (0 = başarılı),
; hata bayrağı: kaldırıcı çalıştırılamadı.
Function uninstallOldVersion
  Exch $R4
  ClearErrors
  StrCpy $R0 0
  ${if} $R4 == "HKCU"
    ReadRegStr $uninstallString HKCU "${UNINSTALL_REGISTRY_KEY}" UninstallString
  ${else}
    ReadRegStr $uninstallString SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" UninstallString
  ${endif}
  ${if} $uninstallString == ""
    ClearErrors
    Pop $R4
    Return
  ${endif}
  Push $uninstallString
  Call GetInQuotes
  Pop $uninstallerFileName
  ${if} $R4 == "HKCU"
    ReadRegStr $installationDir HKCU "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${else}
    ReadRegStr $installationDir SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${endif}
  ${if} $installationDir == ""
  ${andIf} $uninstallerFileName != ""
    Push $uninstallerFileName
    Call GetFileParent
    Pop $installationDir
  ${endif}
  ${if} $installationDir == ""
  ${andIf} $uninstallerFileName == ""
    ClearErrors
    Pop $R4
    Return
  ${endif}
  ${if} $installMode == "CurrentUser"
  ${orIf} $R4 == "HKCU"
    StrCpy $0 "/currentuser"
  ${else}
    StrCpy $0 "/allusers"
  ${endif}
  !insertmacro setIsTryToKeepShortcuts
  ${if} $isTryToKeepShortcuts == "true"
    ${if} $R4 == "HKCU"
      ReadRegStr $R5 HKCU "${INSTALL_REGISTRY_KEY}" KeepShortcuts
    ${else}
      ReadRegStr $R5 SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" KeepShortcuts
    ${endif}
    ${if} $R5 == "true"
    ${andIf} ${FileExists} "$appExe"
      StrCpy $0 "$0 --keep-shortcuts"
    ${endIf}
  ${endIf}
  ${if} ${isDeleteAppData}
    StrCpy $0 "$0 --delete-app-data"
  ${else}
    ; her zaman --updated: DELETE_APP_DATA_ON_UNINSTALL tanımlı eski kaldırıcı veriyi silmesin
    StrCpy $0 "$0 --updated"
  ${endif}
  StrCpy $uninstallerFileNameTemp "$PLUGINSDIR\old-uninstaller.exe"
  ClearErrors
  CopyFiles /SILENT "$uninstallerFileName" "$uninstallerFileNameTemp"
  StrCpy $R5 0
  UninstallLoop:
    IntOp $R5 $R5 + 1
    ${if} $R5 > 5
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(appCannotBeClosed)" /SD IDCANCEL IDRETRY OneMoreAttempt
      Pop $R4
      Return
    ${endIf}
  OneMoreAttempt:
    ClearErrors
    ExecWait '"$uninstallerFileNameTemp" /S /KEEP_APP_DATA $0 _?=$installationDir' $R0
    IfErrors TryInPlace CheckResult
    TryInPlace:
      ClearErrors
      ExecWait '"$uninstallerFileName" /S /KEEP_APP_DATA $0 _?=$installationDir' $R0
      IfErrors DoesNotExist
    CheckResult:
      ${if} $R0 == 0
        Pop $R4
        Return
      ${endIf}
    Sleep 1000
    Goto UninstallLoop
  DoesNotExist:
    Pop $R4
    SetErrors
FunctionEnd

Function handleUninstallResult
  IfErrors 0 +3
  DetailPrint `Uninstall was not successful. Not able to launch uninstaller!`
  Return
  ${if} $R0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(uninstallFailed): $R0" /SD IDOK
    DetailPrint `Uninstall was not successful. Uninstaller error code: $R0.`
    SetErrorLevel 2
    Quit
  ${endif}
FunctionEnd

!include "product-${PRODUCT_KIND}.nsh"

Section "install" INSTALL_SECTION_ID
  InitPluginsDir
  ${IfNot} ${Silent}
    SetDetailsPrint none
  ${endif}
  StrCpy $appExe "$INSTDIR\${APP_EXECUTABLE_FILENAME}"
  !insertmacro setLinkVars
  ; Yükseltilmiş iç süreç de (makineye kurulum) denetler: dış süreç kurulum yapmadan çıkar.
  !insertmacro CHECK_APP_RUNNING

  StrCpy $keepShortcuts "false"
  !insertmacro setIsTryToKeepShortcuts
  ${if} $isTryToKeepShortcuts == "true"
    ReadRegStr $R1 SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" KeepShortcuts
    ${if} $R1 == "true"
    ${andIf} ${FileExists} "$appExe"
      StrCpy $keepShortcuts "true"
    ${endIf}
  ${endif}

  Push "SHELL"
  Call uninstallOldVersion
  Call handleUninstallResult
  ${if} $installMode == "all"
    Push "HKCU"
    Call uninstallOldVersion
    Call handleUninstallResult
  ${endIf}

  !insertmacro customPreInstall

  ; Dosyalar önce geçici klasöre açılır, sonra tek seferde kopyalanır (extractAppPackage.nsh'deki
  ; atomik kopyanın karşılığı): kilitli bir dosya yüzünden yarım kurulum kalmasın.
  SetOutPath "$PLUGINSDIR\app"
  File /r "${STAGE_DIR}\*.*"
  SetOutPath $INSTDIR
  StrCpy $R1 0
  LoopCopy:
    IntOp $R1 $R1 + 1
    ClearErrors
    CopyFiles /SILENT "$PLUGINSDIR\app\*" $INSTDIR
    IfErrors 0 DoneCopy
    DetailPrint `Can't modify "${PRODUCT_NAME}"'s files.`
    ${if} $R1 < 5
      Sleep 1000
      Goto LoopCopy
    ${endIf}
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "$(appCannotBeClosed)" /SD IDCANCEL IDRETRY LoopCopy
    SetErrorLevel 2
    Quit
  DoneCopy:
  RMDir /r "$PLUGINSDIR\app"

  WriteUninstaller "$INSTDIR\${UNINSTALL_FILENAME}"

  ; registryAddInstallInfo
  WriteRegStr SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" InstallLocation "$INSTDIR"
  WriteRegStr SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" KeepShortcuts "true"
  WriteRegStr SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" ShortcutName "${SHORTCUT_NAME}"
  ${if} $installMode == "all"
    StrCpy $0 "/allusers"
  ${else}
    StrCpy $0 "/currentuser"
  ${endIf}
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" DisplayName "${UNINSTALL_DISPLAY_NAME}"
  StrCpy $2 "$INSTDIR\${UNINSTALL_FILENAME}"
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" UninstallString '"$2" $0'
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" QuietUninstallString '"$2" $0 /S'
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" "DisplayIcon" "$appExe,0"
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" "Publisher" "${COMPANY_NAME}"
  WriteRegStr SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" "Comments" "${APP_DESCRIPTION}"
  WriteRegDWORD SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" NoModify 1
  WriteRegDWORD SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" NoRepair 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}" "EstimatedSize" "$0"

  ; Başlat menüsü kısayolu (addStartMenuLink)
  ${if} $keepShortcuts == "false"
    ${if} $oldMenuDirectory != ""
      RMDir "$SMPROGRAMS\$oldMenuDirectory"
    ${endIf}
    CreateShortCut "$newStartMenuLink" "$appExe" "" "$appExe" 0 "" "" "${APP_DESCRIPTION}"
    ClearErrors
    WinShell::SetLnkAUMI "$newStartMenuLink" "${APP_ID}"
  ${elseif} $oldStartMenuLink != $newStartMenuLink
  ${andIf} ${FileExists} "$oldStartMenuLink"
    Rename $oldStartMenuLink $newStartMenuLink
    WinShell::UninstShortcut "$oldStartMenuLink"
    WinShell::SetLnkAUMI "$newStartMenuLink" "${APP_ID}"
    ${if} $oldMenuDirectory != ""
      RMDir "$SMPROGRAMS\$oldMenuDirectory"
    ${endIf}
  ${endIf}

  ; Masaüstü kısayolu (addDesktopLink, createDesktopShortcut: always → RECREATE_DESKTOP_SHORTCUT)
  ${ifNot} ${isNoDesktopShortcut}
    ${if} $keepShortcuts == "false"
      CreateShortCut "$newDesktopLink" "$appExe" "" "$appExe" 0 "" "" "${APP_DESCRIPTION}"
      ClearErrors
      WinShell::SetLnkAUMI "$newDesktopLink" "${APP_ID}"
    ${elseif} $oldDesktopLink != $newDesktopLink
    ${andIf} ${FileExists} "$oldDesktopLink"
      Rename $oldDesktopLink $newDesktopLink
      WinShell::UninstShortcut "$oldDesktopLink"
      WinShell::SetLnkAUMI "$newDesktopLink" "${APP_ID}"
    ${elseif} $oldDesktopLink != $newDesktopLink
    ${orIfNot} ${FileExists} "$oldDesktopLink"
      ${ifNot} ${isUpdated}
        CreateShortCut "$newDesktopLink" "$appExe" "" "$appExe" 0 "" "" "${APP_DESCRIPTION}"
        ClearErrors
        WinShell::SetLnkAUMI "$newDesktopLink" "${APP_ID}"
      ${endIf}
    ${endIf}
    System::Call 'Shell32::SHChangeNotify(i 0x8000000, i 0, i 0, i 0)'
  ${endIf}

  ${if} ${FileExists} "$newStartMenuLink"
    StrCpy $launchLink "$newStartMenuLink"
  ${else}
    StrCpy $launchLink "$INSTDIR\${APP_EXECUTABLE_FILENAME}"
  ${endIf}

  !insertmacro customInstall

  ; Yardımlı kurulum yalnız sessizken kendiliğinden açar (bitiş sayfasında "çalıştır" var).
  ${if} ${isForceRun}
  ${andIf} ${Silent}
    HideWindow
    Call StartApp
  ${endIf}
SectionEnd

; ------------------------------------------------------------------ kaldırma

Function un.onInit
  !insertmacro check64BitAndSetRegView
  ${GetParameters} $R0
  ${GetOptions} $R0 "/S" $R1
  ${IfNot} ${Errors}
    SetSilent silent
  ${EndIf}
  ${If} ${UAC_IsInnerInstance}
  ${AndIfNot} ${UAC_IsAdmin}
    SetErrorLevel 0x666666
    Quit
  ${EndIf}
  ; Kip: komut satırı (/allusers, /currentuser — kayıttaki UninstallString'de) ya da kayıt.
  StrCpy $hasPerMachineInstallation "0"
  StrCpy $hasPerUserInstallation "0"
  ReadRegStr $perMachineInstallationFolder HKLM "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${if} $perMachineInstallationFolder != ""
    StrCpy $hasPerMachineInstallation "1"
  ${endif}
  ReadRegStr $perUserInstallationFolder HKCU "${INSTALL_REGISTRY_KEY}" InstallLocation
  ${if} $perUserInstallationFolder != ""
    StrCpy $hasPerUserInstallation "1"
  ${endif}
  ${if} ${isForAllUsers}
    StrCpy $hasPerMachineInstallation "1"
    StrCpy $hasPerUserInstallation "0"
  ${endif}
  ${if} ${isForCurrentUser}
    StrCpy $hasPerMachineInstallation "0"
    StrCpy $hasPerUserInstallation "1"
  ${endif}
  ${if} $hasPerUserInstallation == "0"
  ${andif} $hasPerMachineInstallation == "1"
    StrCpy $installMode all
    SetShellVarContext all
    ${IfNot} ${UAC_IsAdmin}
      !insertmacro UAC_RunElevated
      SetErrorLevel $2
      Quit
    ${endif}
  ${else}
    StrCpy $installMode CurrentUser
    SetShellVarContext current
  ${endif}
  ${If} ${Silent}
    !insertmacro CHECK_APP_RUNNING
  ${EndIf}
FunctionEnd

Function un.atomicRMDir
  Exch $R0
  Push $R1
  Push $R2
  Push $R3
  StrCpy $R3 "$INSTDIR$R0\*.*"
  FindFirst $R1 $R2 $R3
  loop:
    StrCmp $R2 "" break
    StrCmp $R2 "." continue
    StrCmp $R2 ".." continue
    IfFileExists "$INSTDIR$R0\$R2\*.*" isDir isNotDir
    isDir:
      CreateDirectory "$PLUGINSDIR\old-install$R0\$R2"
      Push "$R0\$R2"
      Call un.atomicRMDir
      Pop $R3
      ${if} $R3 != 0
        Goto done
      ${endIf}
      Goto continue
    isNotDir:
      ClearErrors
      Rename "$INSTDIR$R0\$R2" "$PLUGINSDIR\old-install$R0\$R2"
      StrCmp "$R0\$R2" "\${UNINSTALL_FILENAME}" 0 +2
      ClearErrors
      IfErrors 0 +3
      StrCpy $R3 "$INSTDIR$R0\$R2"
      Goto done
    continue:
      FindNext $R1 $R2
      Goto loop
  break:
    StrCpy $R3 0
  done:
    FindClose $R1
    StrCpy $R0 $R3
    Pop $R3
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

Function un.restoreFiles
  Exch $R0
  Push $R1
  Push $R2
  Push $R3
  StrCpy $R3 "$PLUGINSDIR\old-install$R0\*.*"
  FindFirst $R1 $R2 $R3
  loop:
    StrCmp $R2 "" break
    StrCmp $R2 "." continue
    StrCmp $R2 ".." continue
    IfFileExists "$INSTDIR$R0\$R2\*.*" isDir isNotDir
    isDir:
      CreateDirectory "$INSTDIR$R0\$R2"
      Push "$R0\$R2"
      Call un.restoreFiles
      Pop $R3
      Goto continue
    isNotDir:
      Rename "$PLUGINSDIR\old-install$R0\$R2" "$INSTDIR$R0\$R2"
    continue:
      FindNext $R1 $R2
      Goto loop
  break:
    StrCpy $R0 0
    FindClose $R1
    Pop $R3
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

Section "un.Uninstall"
  SectionIn RO
  ${IfNot} ${Silent}
    !insertmacro CHECK_APP_RUNNING
  ${endIf}
  !insertmacro setLinkVars

  !insertmacro customUnInstall

  ${if} ${isUpdated}
    CreateDirectory "$PLUGINSDIR\old-install"
    Push ""
    Call un.atomicRMDir
    Pop $R0
    ${if} $R0 != 0
      DetailPrint "File is busy, aborting: $R0"
      Push ""
      Call un.restoreFiles
      Pop $R0
      Abort `Can't rename "$INSTDIR" to "$PLUGINSDIR\old-install".`
    ${endif}
  ${endif}
  SetOutPath $TEMP
  RMDir /r $INSTDIR

  ${ifNot} ${isKeepShortcuts}
    WinShell::UninstAppUserModelId "${APP_ID}"
    WinShell::UninstShortcut "$oldDesktopLink"
    Delete "$oldDesktopLink"
    WinShell::UninstShortcut "$oldStartMenuLink"
    Delete "$oldStartMenuLink"
    ReadRegStr $R1 SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}" MenuDirectory
    ${ifNot} $R1 == ""
      RMDir "$SMPROGRAMS\$R1"
    ${endIf}
  ${endIf}
  System::Call 'shell32::SHChangeNotify(i, i, i, i) v (0x08000000, 0, 0, 0)'

  ; Uygulama verisi (deleteAppDataOnUninstall): electron-builder'ın sildiği klasörler. P2'nin asıl
  ; profili (%APPDATA%\P2, Rust: %APPDATA%\P2-Rust) Electron'da da silinmiyordu; kütüphane durumu ve
  ; oturum korunur.
  StrCpy $R2 "0"
  ${if} ${isDeleteAppData}
    StrCpy $R2 "1"
  ${else}
    !ifdef DELETE_APP_DATA_ON_UNINSTALL
      ${ifNot} ${isUpdated}
        StrCpy $R2 "1"
      ${endif}
    !endif
  ${endif}
  ${if} $R2 == "1"
    ${if} $installMode == "all"
      SetShellVarContext current
    ${endif}
    RMDir /r "$APPDATA\${APP_FILENAME}"
    RMDir /r "$APPDATA\${APP_PACKAGE_NAME}"
    ${if} $installMode == "all"
      SetShellVarContext all
    ${endif}
  ${endif}

  DeleteRegKey SHELL_CONTEXT "${UNINSTALL_REGISTRY_KEY}"
  DeleteRegKey SHELL_CONTEXT "${INSTALL_REGISTRY_KEY}"
SectionEnd
