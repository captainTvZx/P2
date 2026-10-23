// Ayarlardaki overlay kısayollarını ve ekran görüntüsü klasörünü Electron ana sürecine yayar
// (DLL kısayol tablosu + host yapılandırması). Açılışta bir kez, sonra yalnız ilgili ayar değişince
// gönderilir. main.tsx'ten yalnız ana pencerede çağrılır.
import { toNativeHotkeys } from './hotkeys'
import { getSettings, subscribeSettings } from './settings'

export function startOverlaySettingsSync():()=>void{
  let lastHotkeys='',lastDir:string|null=null
  const push=()=>{
    const bridge=window.p2?.gameOverlay
    if(!bridge)return
    const settings=getSettings()
    const signature=JSON.stringify(settings.overlayHotkeys)
    if(signature!==lastHotkeys&&bridge.setHotkeys){
      lastHotkeys=signature
      try{void Promise.resolve(bridge.setHotkeys(toNativeHotkeys(settings.overlayHotkeys))).catch(()=>{})}catch{}
    }
    const dir=settings.overlayScreenshotDir||''
    if(dir!==lastDir&&bridge.configure){lastDir=dir;try{bridge.configure({screenshotDir:dir})}catch{}}
  }
  push()
  return subscribeSettings(push)
}
