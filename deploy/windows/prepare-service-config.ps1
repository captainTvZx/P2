param(
  [Parameter(Mandatory = $true)]
  [string]$InstallDir
)

# Rust sürümü: hizmet p2-server.exe --p2-server (konsol ayrı süreç: p2-server-console.exe).
$resolved = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$exe = [Security.SecurityElement]::Escape((Join-Path $resolved 'p2-server.exe'))
$working = [Security.SecurityElement]::Escape($resolved)
$programData = [Security.SecurityElement]::Escape((Join-Path $env:ProgramData 'P2'))
$xml = @"
<service>
  <id>P2Server</id>
  <name>P2 Server</name>
  <description>P2 API, dosya ve istemci guncelleme sunucusu.</description>
  <executable>$exe</executable>
  <arguments>--p2-server</arguments>
  <workingdirectory>$working</workingdirectory>
  <logpath>$programData</logpath>
  <log mode="roll-by-size">
    <sizeThreshold>10240</sizeThreshold>
    <keepFiles>4</keepFiles>
  </log>
  <startmode>Automatic</startmode>
  <stoptimeout>20sec</stoptimeout>
  <onfailure action="restart" delay="45 sec" />
</service>
"@
[IO.File]::WriteAllText((Join-Path $resolved 'P2ServerService.xml'), $xml, [Text.UTF8Encoding]::new($false))
