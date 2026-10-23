import { spawn } from 'node:child_process'
import { join } from 'node:path'
import { existsSync } from 'node:fs'

// Kurulu P2 servisi 4000'i kullanabilir. Geliştirme sunucusu ayrı portta çalışır;
// Electron preload ve Vite proxy aynı adresi otomatik kullanır.
const port=process.env.P2_DEV_API_PORT||'4010'
// Geliştirme API'si kurulu sunucuyla aynı veritabanını (.env) kullanır; görseller ve sürüm parçaları da
// onun klasöründen (ProgramData\P2) okunsun ki simgeler/kapaklar dev'de de yüklensin. Env ile ezilebilir.
const serverData=process.platform==='win32'?join(process.env.ProgramData||'C:\\ProgramData','P2'):''
const shared={}
if(serverData&&existsSync(join(serverData,'server.json'))){
  if(!process.env.P2_UPLOAD_DIR)shared.P2_UPLOAD_DIR=join(serverData,'uploads')
  if(!process.env.P2_RELEASE_DIR)shared.P2_RELEASE_DIR=join(serverData,'releases')
  console.log(`[dev] yüklemeler ve sürümler kurulu sunucunun klasöründen: ${serverData}`)
}
const env={...process.env,...shared,PORT:port,P2_DEV_API_PORT:port,P2_API_URL:`http://127.0.0.1:${port}`}
const executable=join(process.cwd(),'node_modules','concurrently','dist','bin','index.js')
const child=spawn(process.execPath,[executable,'-k','npm:dev:web','npm:dev:api','npm:dev:electron-local'],{cwd:process.cwd(),env,stdio:'inherit',windowsHide:false})
child.on('error',error=>{console.error(error);process.exitCode=1})
child.on('exit',code=>process.exit(code??0))
for(const signal of ['SIGINT','SIGTERM'])process.on(signal,()=>child.kill())
