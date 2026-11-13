; P2 Server (Rust) — ürüne özgü adımlar. ProgramData\P2 (server.json, günlükler, yedekler,
; güncelleme klasörü) kuruluma ve kaldırmaya hiç dokunulmaz; hizmet kaydı da değişmez.

!macro customPreInstall
!macroend

; WinSW tanımı mutlak yollarla yazılır (deploy/windows/prepare-service-config.ps1 ile aynı içerik).
; Hizmet kayıtlıysa bir sonraki başlatmada yeni XML okunur: `p2-server.exe --p2-server`. Electron
; döneminden kalan, hâlâ çalışan WinSW eski XML'deki "P2 Server.exe --p2-server" komutunu
; çalıştırırsa Rust konsolu işi p2-server.exe'ye devreder (p2-server-console main.rs).
!macro customInstall
  ${WordReplace} "$INSTDIR" "&" "&amp;" "+" $R1
  ReadEnvStr $R2 ProgramData
  ${if} $R2 == ""
    StrCpy $R2 "C:\ProgramData"
  ${endIf}
  ${WordReplace} "$R2\P2" "&" "&amp;" "+" $R2
  ClearErrors
  FileOpen $R3 "$INSTDIR\P2ServerService.xml" w
  ${IfNot} ${Errors}
    FileWriteWord $R3 0xFEFF
    FileWriteUTF16LE $R3 "<service>$\r$\n"
    FileWriteUTF16LE $R3 "  <id>P2Server</id>$\r$\n"
    FileWriteUTF16LE $R3 "  <name>P2 Server</name>$\r$\n"
    FileWriteUTF16LE $R3 "  <description>P2 API, dosya ve istemci guncelleme sunucusu.</description>$\r$\n"
    FileWriteUTF16LE $R3 "  <executable>$R1\p2-server.exe</executable>$\r$\n"
    FileWriteUTF16LE $R3 "  <arguments>--p2-server</arguments>$\r$\n"
    FileWriteUTF16LE $R3 "  <workingdirectory>$R1</workingdirectory>$\r$\n"
    FileWriteUTF16LE $R3 "  <logpath>$R2</logpath>$\r$\n"
    FileWriteUTF16LE $R3 "  <log mode=$\"roll-by-size$\">$\r$\n"
    FileWriteUTF16LE $R3 "    <sizeThreshold>10240</sizeThreshold>$\r$\n"
    FileWriteUTF16LE $R3 "    <keepFiles>4</keepFiles>$\r$\n"
    FileWriteUTF16LE $R3 "  </log>$\r$\n"
    FileWriteUTF16LE $R3 "  <startmode>Automatic</startmode>$\r$\n"
    FileWriteUTF16LE $R3 "  <stoptimeout>20sec</stoptimeout>$\r$\n"
    FileWriteUTF16LE $R3 "  <onfailure action=$\"restart$\" delay=$\"45 sec$\" />$\r$\n"
    FileWriteUTF16LE $R3 "</service>$\r$\n"
    FileClose $R3
  ${EndIf}
!macroend

!macro customUnInstall
!macroend
