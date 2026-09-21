// Bazı IDE/ajan terminalleri ELECTRON_RUN_AS_NODE=1 ile açılır. Electron CLI'ı
// doğrudan çağırmak bu durumda main sürecini düz Node olarak çalıştırır.
const {spawn}=require('node:child_process')
const electronPath=require('electron')
const env={...process.env}
delete env.ELECTRON_RUN_AS_NODE

const child=spawn(electronPath,['.',...process.argv.slice(2)],{
  cwd:require('node:path').join(__dirname,'..'),
  env,
  stdio:'inherit',
  windowsHide:false,
})

child.on('error',error=>{console.error(error);process.exitCode=1})
child.on('exit',code=>process.exit(code??0))
for(const signal of['SIGINT','SIGTERM'])process.on(signal,()=>child.kill())
