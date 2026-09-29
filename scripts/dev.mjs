import { spawn } from 'node:child_process'
import { join } from 'node:path'

// Kurulu P2 servisi 4000'i kullanabilir. Geliştirme sunucusu ayrı portta çalışır;
// Electron preload ve Vite proxy aynı adresi otomatik kullanır.
const port=process.env.P2_DEV_API_PORT||'4010'
const env={...process.env,PORT:port,P2_DEV_API_PORT:port,P2_API_URL:`http://127.0.0.1:${port}`}
const executable=join(process.cwd(),'node_modules','concurrently','dist','bin','index.js')
const child=spawn(process.execPath,[executable,'-k','npm:dev:web','npm:dev:api','npm:dev:electron-local'],{cwd:process.cwd(),env,stdio:'inherit',windowsHide:false})
child.on('error',error=>{console.error(error);process.exitCode=1})
child.on('exit',code=>process.exit(code??0))
for(const signal of ['SIGINT','SIGTERM'])process.on(signal,()=>child.kill())
