import { BASE_PALETTE } from './themePalette'

// Tema motoru: uygulamanın tüm renkleri --cN değişkenlerinde. Bir tema, temel paleti
// ton/doygunluk/parlaklık dönüşümünden geçirerek yeniden boyar; böylece 650+ rengin
// tamamı tek bir tanımla tutarlı biçimde değişir.
export type ColorMode='dark'|'light'|'system'
export type ThemeSpec={
  id:string
  name:string
  mode:'dark'|'light'
  accent:string          // vurgu rengi (mavi düğmeler, seçili satırlar)
  surface:string         // en koyu/açık zemin
  tint?:number           // yüzeylerin vurgu rengine kayma miktarı (0-1)
  contrast?:number       // metin/zemin ayrımı çarpanı (0.8-1.2)
  saturation?:number     // renkli öğelerin doygunluk çarpanı
}

export type Appearance={
  mode:ColorMode
  themeId:string
  custom:ThemeSpec|null
  font:string
  fontScale:number
}

// ---- renk yardımcıları ----
type Hsl={h:number;s:number;l:number;a:number}

function parse(color:string):Hsl{
  const value=color.replace('#','')
  const full=value.length===3||value.length===4?value.split('').map(c=>c+c).join(''):value
  const r=parseInt(full.slice(0,2),16)/255,g=parseInt(full.slice(2,4),16)/255,b=parseInt(full.slice(4,6),16)/255
  const a=full.length===8?parseInt(full.slice(6,8),16)/255:1
  const max=Math.max(r,g,b),min=Math.min(r,g,b),delta=max-min
  let h=0
  if(delta){
    if(max===r)h=((g-b)/delta)%6
    else if(max===g)h=(b-r)/delta+2
    else h=(r-g)/delta+4
    h*=60
    if(h<0)h+=360
  }
  const l=(max+min)/2
  const s=delta===0?0:delta/(1-Math.abs(2*l-1))
  return{h,s,l,a}
}

function toHex({h,s,l,a}:Hsl){
  const c=(1-Math.abs(2*l-1))*s,x=c*(1-Math.abs(((h/60)%2)-1)),m=l-c/2
  const [r,g,b]=h<60?[c,x,0]:h<120?[x,c,0]:h<180?[0,c,x]:h<240?[0,x,c]:h<300?[x,0,c]:[c,0,x]
  const channel=(value:number)=>Math.round(Math.min(1,Math.max(0,value+m))*255).toString(16).padStart(2,'0')
  return `#${channel(r)}${channel(g)}${channel(b)}${a<1?Math.round(a*255).toString(16).padStart(2,'0'):''}`
}

const clamp=(value:number,min=0,max=1)=>Math.min(max,Math.max(min,value))

// Temel palet "Gece" temasına göre yazıldı: dönüşümler bu referansa göre hesaplanır.
const BASE_ACCENT_HUE=209
const BASE_ACCENT_SAT=0.42
const BASE_SURFACE_L=0.13

function hueGap(a:number,b:number){return Math.abs(((a-b+540)%360)-180)}

function transform(base:Hsl,theme:ThemeSpec,accent:Hsl,surface:Hsl):Hsl{
  const contrast=theme.contrast??1
  if(base.s>0.28){
    // Renkli öğeler: vurgu ailesindekiler temanın rengine kayar, diğerleri (yeşil/kırmızı) korunur.
    const nearAccent=hueGap(base.h,BASE_ACCENT_HUE)<45
    const h=nearAccent?(accent.h+(base.h-BASE_ACCENT_HUE)*0.3+360)%360:base.h
    const s=clamp(base.s*(theme.saturation??1)*(nearAccent?accent.s/BASE_ACCENT_SAT:1),0.08,0.95)
    const l=theme.mode==='light'?clamp(base.l*0.78+0.02,0.2,0.62):base.l
    return{h,s,l,a:base.a}
  }
  // Nötrler (zemin, çizgi, metin): tek bir parlaklık ekseni üzerinde kaydırılır.
  const h=(surface.h+(base.h-218)*0.15+360)%360
  const s=clamp((theme.tint??0.35)*0.3+base.s*0.2,0,0.35)
  const offset=(base.l-BASE_SURFACE_L)*contrast
  const l=theme.mode==='light'?clamp(surface.l-offset*0.95,0.04,0.995):clamp(surface.l+offset,0.02,0.99)
  return{h,s,l,a:base.a}
}

export function paletteFor(theme:ThemeSpec){
  if(theme.id==='gece')return BASE_PALETTE   // temel palet zaten bu tema
  const accent=parse(theme.accent),surface=parse(theme.surface)
  return BASE_PALETTE.map(color=>{
    const base=parse(color)
    if(base.s===0&&(base.l>0.97||base.l<0.03))return color   // saf beyaz/siyah dokunulmaz
    return toHex(transform(base,theme,accent,surface))
  })
}

// ---- hazır temalar ----
export const P2_THEMES:ThemeSpec[]=[
  {id:'gece',name:'Gece',mode:'dark',accent:'#487eb0',surface:'#1b1f26',tint:0.3},
  {id:'komur',name:'Kömür',mode:'dark',accent:'#7f8c99',surface:'#17191c',tint:0.12,saturation:0.75},
  {id:'okyanus',name:'Okyanus',mode:'dark',accent:'#1f9aa8',surface:'#121d22',tint:0.5},
  {id:'orman',name:'Orman',mode:'dark',accent:'#3f9c62',surface:'#141b17',tint:0.45},
  {id:'safak',name:'Şafak',mode:'dark',accent:'#d2704a',surface:'#1f1a1a',tint:0.4},
  {id:'menekse',name:'Menekşe',mode:'dark',accent:'#8b62d4',surface:'#1a1722',tint:0.45},
  {id:'gulkurusu',name:'Gül Kurusu',mode:'dark',accent:'#c05a7d',surface:'#20181d',tint:0.4},
  {id:'gece-yarisi',name:'Gece Yarısı',mode:'dark',accent:'#4460c8',surface:'#0f1220',tint:0.55,contrast:1.05},
  {id:'kagit',name:'Kâğıt',mode:'light',accent:'#2f6fb0',surface:'#f6f7f9',tint:0.18},
  {id:'gun',name:'Gün',mode:'light',accent:'#1f8f7a',surface:'#f4f8f6',tint:0.24},
  {id:'lavanta',name:'Lavanta',mode:'light',accent:'#6d55c4',surface:'#f6f5fb',tint:0.26},
  {id:'kum',name:'Kum',mode:'light',accent:'#b4703a',surface:'#faf6f1',tint:0.22},
]

export const FONTS=[
  {id:'inter',name:'Inter (varsayılan)',stack:"Inter,'Segoe UI',system-ui,sans-serif"},
  {id:'system',name:'Sistem yazı tipi',stack:"system-ui,'Segoe UI',Roboto,sans-serif"},
  {id:'segoe',name:'Segoe UI',stack:"'Segoe UI',Tahoma,sans-serif"},
  {id:'georgia',name:'Georgia (tırnaklı)',stack:"Georgia,'Times New Roman',serif"},
  {id:'mono',name:'Tek aralıklı',stack:"'Cascadia Mono',Consolas,'Courier New',monospace"},
]

export const defaultAppearance:Appearance={mode:'system',themeId:'gece',custom:null,font:'inter',fontScale:100}

export function themeOf(appearance:Appearance):ThemeSpec{
  const wantsLight=appearance.mode==='light'||(appearance.mode==='system'&&typeof window!=='undefined'&&window.matchMedia?.('(prefers-color-scheme: light)').matches)
  if(appearance.themeId==='custom'&&appearance.custom)return{...appearance.custom,mode:wantsLight?'light':appearance.custom.mode}
  const chosen=P2_THEMES.find(item=>item.id===appearance.themeId)
  if(chosen&&(appearance.mode!=='system'?chosen.mode===(wantsLight?'light':'dark'):true))return chosen
  // Seçilen tema istenen moda uymuyorsa aynı ailenin karşılığına geçilir.
  return P2_THEMES.find(item=>item.mode===(wantsLight?'light':'dark'))||P2_THEMES[0]
}

export function applyAppearance(appearance:Appearance){
  if(typeof document==='undefined')return
  const theme=themeOf(appearance)
  const palette=paletteFor(theme)
  const root=document.documentElement
  palette.forEach((color,index)=>root.style.setProperty(`--c${index}`,color))
  root.style.setProperty('color-scheme',theme.mode)
  root.dataset.theme=theme.mode
  const font=FONTS.find(item=>item.id===appearance.font)||FONTS[0]
  root.style.setProperty('--app-font',font.stack)
  root.style.setProperty('--app-font-scale',String((appearance.fontScale||100)/100))
}
