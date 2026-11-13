; P2 Client (Rust) — ürüne özgü adımlar. Electron istemcisinin kurulumunda özel adım yoktu.

!macro customPreInstall
!macroend

!macro customInstall
!macroend

; p2:// kaydı (uygulama açılışta HKCU\Software\Classes\p2'ye yazar; Electron
; `setAsDefaultProtocolClient('p2')` ile aynı biçim: "<exe>" "%1"). Güncelleme dışı kaldırmada
; kayıt bu kuruluma işaret ediyorsa silinir (StrCmp büyük/küçük harfe duyarsız); Electron
; kaldırıcısı silmiyordu, ölü bağlantı kalıyordu.
!macro customUnInstall
  ${ifNot} ${isUpdated}
    ReadRegStr $R1 HKCU "Software\Classes\p2\shell\open\command" ""
    ${if} $R1 == '"$INSTDIR\${APP_EXECUTABLE_FILENAME}" "%1"'
      DeleteRegKey HKCU "Software\Classes\p2"
    ${endIf}
  ${endIf}
!macroend
