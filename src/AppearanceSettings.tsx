import { useRef, useState } from 'react'
import { updateSettings, useSettings } from './settings'
import { FONTS, P2_THEMES, defaultAppearance, paletteFor, type ThemeSpec } from './theme'
import { useT } from './i18n'

// Tema kartındaki küçük önizleme: temanın kendi paletinden üretilir.
function Preview({theme}:{theme:ThemeSpec}){
  const palette=paletteFor(theme)
  // Paletteki belirli indeksler yerine temanın ana renkleri gösterilir.
  const shell=palette[564]||theme.surface
  const panel=palette[579]||theme.surface
  return <span className="theme-preview" style={{background:shell}}>
    <i style={{background:theme.surface}}/>
    <i style={{background:panel,opacity:.55}}/>
    <b style={{background:theme.accent}}/>
  </span>
}

const MODES=[{value:'system',label:'look.system',hint:'look.system.hint'},{value:'light',label:'look.light',hint:'look.light.hint'},{value:'dark',label:'look.dark',hint:'look.dark.hint'}] as const

export default function AppearanceSettings(){
  const t=useT()
  const settings=useSettings()
  const appearance=settings.appearance||defaultAppearance
  const [tab,setTab]=useState<'p2'|'custom'>(appearance.themeId==='custom'?'custom':'p2')
  const [notice,setNotice]=useState('')
  const fileInput=useRef<HTMLInputElement>(null)
  const custom:ThemeSpec=appearance.custom||{id:'custom',name:'Özel tema',mode:'dark',accent:'#487eb0',surface:'#1b1f26',tint:0.35,contrast:1,saturation:1}

  function patch(next:Partial<typeof appearance>){updateSettings({appearance:{...appearance,...next}})}
  function patchCustom(next:Partial<ThemeSpec>){patch({themeId:'custom',custom:{...custom,...next}})}

  function exportTheme(){
    const theme=appearance.themeId==='custom'?custom:P2_THEMES.find(item=>item.id===appearance.themeId)||P2_THEMES[0]
    const blob=new Blob([JSON.stringify({p2Theme:1,...theme},null,2)],{type:'application/json'})
    const url=URL.createObjectURL(blob)
    const link=document.createElement('a')
    link.href=url;link.download=`${theme.name.toLocaleLowerCase('tr-TR').replace(/\s+/g,'-')}.p2theme.json`
    link.click()
    URL.revokeObjectURL(url)
    setNotice(t('look.exported'))
  }

  async function importTheme(file:File){
    try{
      const data=JSON.parse(await file.text())
      if(!data?.accent||!data?.surface)throw new Error('eksik')
      patchCustom({
        name:String(data.name||'İçe aktarılan tema').slice(0,40),
        mode:data.mode==='light'?'light':'dark',
        accent:String(data.accent),surface:String(data.surface),
        tint:Number(data.tint)||0.35,contrast:Number(data.contrast)||1,saturation:Number(data.saturation)||1,
      })
      setTab('custom')
      setNotice(t('look.imported'))
    }catch{setNotice(t('look.notTheme'))}
  }

  return <>
    <section className="settings-block">
      <h4>{t('look.colorMode')}</h4>
      <div className="settings-choices mode-choices">
        {MODES.map(option=><button type="button" key={option.value} className={appearance.mode===option.value?'active':undefined} onClick={()=>patch({mode:option.value})}>
          <b>{t(option.label)}</b>
          <small>{t(option.hint)}</small>
        </button>)}
      </div>
    </section>

    <section className="settings-block">
      <h4>{t('look.font')}</h4>
      <label className="settings-field"><span>{t('look.font')}</span><select value={appearance.font} onChange={event=>patch({font:event.target.value})}>
        {FONTS.map(font=><option key={font.id} value={font.id}>{font.name}</option>)}
      </select></label>
      <label className="settings-field"><span>{t('look.fontSize',{value:appearance.fontScale})}</span>
        <input type="range" min={85} max={125} step={5} value={appearance.fontScale} onChange={event=>patch({fontScale:Number(event.target.value)})}/>
      </label>
      <p className="settings-hint" style={{fontFamily:FONTS.find(font=>font.id===appearance.font)?.stack}}>{t('look.sample')}</p>
    </section>

    <section className="settings-block">
      <h4>{t('look.theme')}</h4>
      <div className="theme-tabs">
        <button type="button" className={tab==='p2'?'active':undefined} onClick={()=>setTab('p2')}>{t('look.p2themes')}</button>
        <button type="button" className={tab==='custom'?'active':undefined} onClick={()=>setTab('custom')}>{t('look.custom')}</button>
      </div>

      {tab==='p2'&&<div className="theme-grid">
        {P2_THEMES.map(theme=><button type="button" key={theme.id} className={appearance.themeId===theme.id?'theme-card active':'theme-card'} onClick={()=>patch({themeId:theme.id})}>
          <Preview theme={theme}/>
          <span><b>{theme.name}</b><small>{t(theme.mode==='light'?'look.light':'look.dark')}</small></span>
        </button>)}
      </div>}

      {tab==='custom'&&<div className="theme-custom">
        <Preview theme={custom}/>
        <div className="theme-fields">
          <label className="settings-field"><span>{t('look.themeName')}</span><input value={custom.name} onChange={event=>patchCustom({name:event.target.value.slice(0,40)})}/></label>
          <label className="settings-field"><span>{t('look.accent')}</span><input type="color" value={custom.accent} onChange={event=>patchCustom({accent:event.target.value})}/></label>
          <label className="settings-field"><span>{t('look.surface')}</span><input type="color" value={custom.surface} onChange={event=>patchCustom({surface:event.target.value})}/></label>
          <label className="settings-field"><span>{t('look.tint',{value:Math.round((custom.tint??0.35)*100)})}</span><input type="range" min={0} max={80} value={Math.round((custom.tint??0.35)*100)} onChange={event=>patchCustom({tint:Number(event.target.value)/100})}/></label>
          <label className="settings-field"><span>{t('look.contrast',{value:Math.round((custom.contrast??1)*100)})}</span><input type="range" min={80} max={125} value={Math.round((custom.contrast??1)*100)} onChange={event=>patchCustom({contrast:Number(event.target.value)/100})}/></label>
          <label className="settings-field"><span>{t('look.saturation',{value:Math.round((custom.saturation??1)*100)})}</span><input type="range" min={40} max={140} value={Math.round((custom.saturation??1)*100)} onChange={event=>patchCustom({saturation:Number(event.target.value)/100})}/></label>
        </div>
      </div>}

      <div className="settings-row">
        <button type="button" className="settings-button" onClick={exportTheme}>{t('look.export')}</button>
        <button type="button" className="settings-button" onClick={()=>fileInput.current?.click()}>{t('look.import')}</button>
        <input ref={fileInput} type="file" accept="application/json,.json" hidden onChange={event=>{const file=event.target.files?.[0];if(file)void importTheme(file);event.target.value=''}}/>
      </div>
      {notice&&<p className="settings-hint">{notice}</p>}
    </section>
  </>
}
