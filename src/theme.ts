// Görünüm: tema + yazı tipi + yazı ölçeği.
// Tüm arayüz src/p2-theme.css'teki anlamsal --p2-* token'larından türer. "P2 Mavi" varsayılan temadır ve
// değerleri CSS'te sabittir; diğer temalar bu token'ların yaprak değerlerini (lacivert merdiveni, vurgu
// merdiveni, cam gradyanlar, kenarlık/gölge alfa'ları, durum ve kimlik tonları) <html> üzerine satır içi
// yazar. Özel tema: vurgu + zemin rengi + ton/kontrast/canlılık kaydırıcıları.
export type ThemeMode='dark'|'light'
export type ThemeSpec={
  id:string
  name:string
  mode:ThemeMode
  accent:string          // vurgu rengi (düğme, seçili satır, bağlantı)
  surface:string         // en koyu (açık temada en açık) zemin
  tint?:number           // zeminin vurgu/zemin rengine doygunluk kayması (0-1)
  contrast?:number       // katmanlar arası açıklık farkı çarpanı (0.8-1.25)
  saturation?:number     // vurgu doygunluğu çarpanı (0.4-1.4)
}
export type Appearance={
  themeId:string
  custom:ThemeSpec|null
  font:string
  fontScale:number
  v?:number
}

// ---- renk yardımcıları ----
type Hsl={h:number;s:number;l:number}
const clamp=(value:number,min=0,max=1)=>Math.min(max,Math.max(min,value))

function parse(color:string):Hsl{
  const value=String(color||'').replace('#','')
  const full=value.length===3||value.length===4?value.split('').map(c=>c+c).join(''):value
  const r=parseInt(full.slice(0,2),16)/255,g=parseInt(full.slice(2,4),16)/255,b=parseInt(full.slice(4,6),16)/255
  if([r,g,b].some(Number.isNaN))return{h:222,s:.48,l:.05}
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
  return{h,s,l}
}

function toRgb({h,s,l}:Hsl){
  const c=(1-Math.abs(2*l-1))*s,x=c*(1-Math.abs(((h/60)%2)-1)),m=l-c/2
  const [r,g,b]=h<60?[c,x,0]:h<120?[x,c,0]:h<180?[0,c,x]:h<240?[0,x,c]:h<300?[x,0,c]:[c,0,x]
  return [r,g,b].map(value=>Math.round(clamp(value+m)*255))
}
const hex=(color:Hsl)=>'#'+toRgb(color).map(value=>value.toString(16).padStart(2,'0')).join('')
const rgba=(color:Hsl,alpha:number)=>`rgba(${toRgb(color).join(',')},${alpha})`
const hsl=(h:number,s:number,l:number):Hsl=>({h:(h+360)%360,s:clamp(s),l:clamp(l)})

// ---- hazır temalar ----
export const P2_THEMES:ThemeSpec[]=[
  {id:'p2-mavi',name:'P2 Mavi',mode:'dark',accent:'#2c7bff',surface:'#070b14',tint:0.55},
  {id:'gece',name:'Gece',mode:'dark',accent:'#487eb0',surface:'#1b1f26',tint:0.3},
  {id:'komur',name:'Kömür',mode:'dark',accent:'#7f8c99',surface:'#17191c',tint:0.12,saturation:0.75},
  {id:'okyanus',name:'Okyanus',mode:'dark',accent:'#1f9aa8',surface:'#121d22',tint:0.5},
  {id:'orman',name:'Orman',mode:'dark',accent:'#3f9c62',surface:'#141b17',tint:0.45},
  {id:'safak',name:'Şafak',mode:'dark',accent:'#d2704a',surface:'#1f1a1a',tint:0.4},
  {id:'menekse',name:'Menekşe',mode:'dark',accent:'#8b62d4',surface:'#1a1722',tint:0.45},
  {id:'gulkurusu',name:'Gül Kurusu',mode:'dark',accent:'#c05a7d',surface:'#20181d',tint:0.4},
  {id:'gece-yarisi',name:'Gece Yarısı',mode:'dark',accent:'#4460c8',surface:'#0f1220',tint:0.55,contrast:1.05},
  {id:'gece-mavi',name:'Gece Mavisi',mode:'dark',accent:'#3d7ea6',surface:'#141a20',tint:0.42},
  {id:'kahve',name:'Kahve',mode:'dark',accent:'#a1784e',surface:'#1c1815',tint:0.35,saturation:0.9},
  {id:'zeytin',name:'Zeytin',mode:'dark',accent:'#8a9a4b',surface:'#191b14',tint:0.35},
  {id:'kagit',name:'Kâğıt',mode:'light',accent:'#2f6fb0',surface:'#d9dde2',tint:0.16,contrast:0.92},
  {id:'sis',name:'Sis',mode:'light',accent:'#526f88',surface:'#d4d9de',tint:0.18,contrast:0.9},
  {id:'gun',name:'Gün',mode:'light',accent:'#267c6b',surface:'#d6e0dc',tint:0.2,contrast:0.92},
  {id:'lavanta',name:'Lavanta',mode:'light',accent:'#6652ad',surface:'#ddd9e7',tint:0.22,contrast:0.92},
  {id:'kum',name:'Kum',mode:'light',accent:'#9a6237',surface:'#e2d9cf',tint:0.18,contrast:0.9},
  {id:'nane',name:'Nane',mode:'light',accent:'#287e62',surface:'#d3dfda',tint:0.2,contrast:0.9},
  {id:'gul',name:'Gül',mode:'light',accent:'#a94e6d',surface:'#e2d5da',tint:0.2,contrast:0.9},
]
export const DEFAULT_THEME_ID='p2-mavi'
export const DEFAULT_CUSTOM:ThemeSpec={id:'custom',name:'Özel tema',mode:'dark',accent:'#2c7bff',surface:'#0b1120',tint:0.5,contrast:1,saturation:1}

export const FONTS=[
  {id:'inter',name:'Inter (varsayılan)',stack:"'Inter Variable',Inter,'Segoe UI',system-ui,sans-serif"},
  {id:'system',name:'Sistem yazı tipi',stack:"system-ui,'Segoe UI',Roboto,sans-serif"},
  {id:'segoe',name:'Segoe UI',stack:"'Segoe UI',Tahoma,sans-serif"},
  {id:'georgia',name:'Georgia (tırnaklı)',stack:"Georgia,'Times New Roman',serif"},
  {id:'mono',name:'Tek aralıklı',stack:"'Cascadia Mono',Consolas,'Courier New',monospace"},
]

export const defaultAppearance:Appearance={themeId:DEFAULT_THEME_ID,custom:null,font:'inter',fontScale:85,v:2}

const HEX_RE=/^#[0-9a-f]{6}$/i
function normalizeTheme(raw:unknown):ThemeSpec|null{
  const source=(raw&&typeof raw==='object'?raw:null) as Record<string,unknown>|null
  if(!source||!HEX_RE.test(String(source.accent))||!HEX_RE.test(String(source.surface)))return null
  const num=(value:unknown,fallback:number,min:number,max:number)=>{const n=Number(value);return Number.isFinite(n)?clamp(n,min,max):fallback}
  return{
    id:'custom',
    name:String(source.name||DEFAULT_CUSTOM.name).slice(0,40)||DEFAULT_CUSTOM.name,
    mode:source.mode==='light'?'light':'dark',
    accent:String(source.accent).toLowerCase(),
    surface:String(source.surface).toLowerCase(),
    tint:num(source.tint,0.5,0,1),
    contrast:num(source.contrast,1,0.8,1.25),
    saturation:num(source.saturation,1,0.4,1.4),
  }
}

// Eski kayıtlar (mode/themeId'siz veya eski 18 temalı sürüm) güvenli değerlere çekilir.
// v<2 kayıtlarda yazı ölçeği yeni varsayılana (%85) döner: eski %100 fazla büyüktü.
export function normalizeAppearance(raw:unknown):Appearance{
  const source=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>
  const font=FONTS.some(item=>item.id===source.font)?String(source.font):defaultAppearance.font
  const custom=normalizeTheme(source.custom)
  const themeId=source.themeId==='custom'&&custom?'custom':P2_THEMES.some(item=>item.id===source.themeId)?String(source.themeId):DEFAULT_THEME_ID
  const scale=Number(source.fontScale)
  const fontScale=source.v===2&&Number.isFinite(scale)?Math.max(70,Math.min(125,Math.round(scale/5)*5)):defaultAppearance.fontScale
  return {themeId,custom,font,fontScale,v:2}
}

export function themeOf(appearance:Appearance):ThemeSpec{
  if(appearance.themeId==='custom'&&appearance.custom)return appearance.custom
  return P2_THEMES.find(item=>item.id===appearance.themeId)||P2_THEMES[0]
}

// ---- palet üretimi: tema → --p2-* yaprak token değerleri ----
export function paletteFor(theme:ThemeSpec):Record<string,string>{
  const light=theme.mode==='light'
  const c=theme.contrast??1
  const surface=parse(theme.surface)
  const accentBase=parse(theme.accent)
  const accent=hsl(accentBase.h,clamp(accentBase.s*(theme.saturation??1),0.04,0.98),accentBase.l)
  const navySat=clamp(surface.s*(0.5+(theme.tint??0.5)),0,0.6)
  const L0=surface.l
  const navy=(offset:number)=>hsl(surface.h,navySat,L0+offset*c)
  const slateSat=clamp(navySat*0.7+0.1,0,0.5)
  const slate=(l:number)=>hsl(surface.h,slateSat,l)
  const a=(dl:number,sMul=1)=>hsl(accent.h,accent.s*sMul,accent.l+dl)

  const p:Record<string,string>={}
  if(!light){
    // Lacivert merdiveni: zeminden yukarı doğru katmanlar
    p['--p2-navy-950']=hex(navy(0));p['--p2-navy-900']=hex(navy(.03));p['--p2-navy-850']=hex(navy(.05))
    p['--p2-navy-800']=hex(navy(.08));p['--p2-navy-700']=hex(navy(.12));p['--p2-navy-600']=hex(navy(.17));p['--p2-navy-500']=hex(navy(.23))
    p['--p2-slate-400']=hex(slate(clamp(L0+.38*c,.3,.6)));p['--p2-slate-300']=hex(slate(clamp(L0+.55*c,.5,.75)))
    p['--p2-slate-200']=hex(slate(clamp(L0+.70*c,.65,.86)));p['--p2-slate-100']=hex(slate(clamp(L0+.86*c,.82,.95)));p['--p2-white']=hex(slate(.97))
    // Vurgu merdiveni
    p['--p2-blue-900']=hex(hsl(accent.h,accent.s*.6,L0+.10));p['--p2-blue-800']=hex(hsl(accent.h,accent.s*.6,L0+.16))
    p['--p2-blue-700']=hex(a(-.14));p['--p2-blue-600']=hex(accent);p['--p2-blue-500']=hex(a(.09))
    p['--p2-blue-400']=hex(a(.18,.9));p['--p2-blue-300']=hex(a(.25,.85));p['--p2-blue-200']=hex(a(.35,.8));p['--p2-blue-100']=hex(a(.44,.75))
    p['--p2-text-accent']=hex(hsl(accent.h,Math.max(accent.s,.35),.74))
    p['--p2-text-2']=rgba(parse(p['--p2-slate-100']),.78)
    p['--p2-hover-soft']=rgba(a(.25),.07)
    p['--p2-glass']=`linear-gradient(135deg, ${rgba(navy(.05),.96)}, ${rgba(navy(0),.94)})`
    p['--p2-glass-strong']=`linear-gradient(160deg, ${hex(navy(.09))} 0%, ${hex(navy(.05))} 48%, ${hex(navy(.03))} 100%)`
    p['--p2-overlay']=rgba(navy(-.01),.62)
    const edge=a(.25,.85)
    p['--p2-border-subtle']=rgba(edge,.10);p['--p2-border']=rgba(edge,.18);p['--p2-border-strong']=rgba(a(.15),.38)
    p['--p2-inset-top']='inset 0 1px 0 rgba(255,255,255,.08)'
    p['--p2-shadow-sm']='0 4px 12px rgba(0,0,0,.35)'
    p['--p2-shadow-md']='0 12px 30px rgba(0,0,0,.45), 0 0 0 1px rgba(0,0,0,.3)'
    p['--p2-shadow-lg']='0 18px 44px rgba(0,0,0,.55), 0 0 0 1px rgba(0,0,0,.35), var(--p2-accent-glow), var(--p2-inset-top)'
    p['--p2-neutral-fallback']='#79818c'
    for(let i=0;i<10;i++){const h=HUES[i];p[`--p2-hue-${i}-bg`]=`hsl(${h} 45% 24%)`;p[`--p2-hue-${i}-bg2`]=`hsl(${h} 45% 32%)`;p[`--p2-hue-${i}-fg`]=`hsl(${h} 70% 90%)`}
    p['--p2-success-text']='#8fe8b4';p['--p2-success-bg']='#122a1d';p['--p2-danger-text']='#ff9aa4';p['--p2-danger-bg']='#2a1420';p['--p2-warning-text']='#ffd08a';p['--p2-warning-bg']='#2a2210'
  }else{
    // Açık tema: zemin en açık katman, "navy" merdiveni beyaza doğru, "slate" merdiveni koyu metinler
    p['--p2-navy-950']=hex(navy(0));p['--p2-navy-900']=hex(navy(.045));p['--p2-navy-850']=hex(navy(.075))
    p['--p2-navy-800']=hex(hsl(surface.h,navySat*.6,clamp(L0+.105*c,0,.985)));p['--p2-navy-700']=hex(navy(-.05));p['--p2-navy-600']=hex(navy(-.09));p['--p2-navy-500']=hex(navy(-.17))
    p['--p2-slate-400']=hex(slate(clamp(L0-.32*c,.35,.6)));p['--p2-slate-300']=hex(slate(clamp(L0-.44*c,.3,.5)))
    p['--p2-slate-200']=hex(slate(clamp(L0-.56*c,.2,.4)));p['--p2-slate-100']=hex(slate(clamp(L0-.70*c,.1,.25)));p['--p2-white']=hex(slate(.07))
    p['--p2-blue-900']=hex(hsl(accent.h,accent.s*.5,.93));p['--p2-blue-800']=hex(hsl(accent.h,accent.s*.5,.87))
    p['--p2-blue-700']=hex(a(-.14));p['--p2-blue-600']=hex(accent);p['--p2-blue-500']=hex(a(-.08))
    p['--p2-blue-400']=hex(a(-.14,.9));p['--p2-blue-300']=hex(a(-.18,.85));p['--p2-blue-200']=hex(hsl(accent.h,accent.s*.7,.78));p['--p2-blue-100']=hex(hsl(accent.h,accent.s*.6,.88))
    p['--p2-text-accent']=hex(hsl(accent.h,Math.max(accent.s,.35),.32))
    p['--p2-text-2']=rgba(parse(p['--p2-slate-100']),.80)
    p['--p2-hover-soft']=rgba(a(-.1),.07)
    p['--p2-glass']=`linear-gradient(135deg, ${rgba(navy(.09),.97)}, ${rgba(navy(.04),.95)})`
    p['--p2-glass-strong']=`linear-gradient(160deg, ${hex(navy(.11))} 0%, ${hex(navy(.07))} 48%, ${hex(navy(.03))} 100%)`
    p['--p2-overlay']=rgba(slate(.12),.45)
    const edge=a(-.14)
    p['--p2-border-subtle']=rgba(edge,.12);p['--p2-border']=rgba(edge,.22);p['--p2-border-strong']=rgba(edge,.42)
    p['--p2-inset-top']='inset 0 1px 0 rgba(255,255,255,.6)'
    p['--p2-shadow-sm']='0 4px 12px rgba(20,30,50,.12)'
    p['--p2-shadow-md']='0 12px 30px rgba(20,30,50,.16), 0 0 0 1px rgba(20,30,50,.06)'
    p['--p2-shadow-lg']='0 18px 44px rgba(20,30,50,.22), 0 0 0 1px rgba(20,30,50,.08), var(--p2-accent-glow), var(--p2-inset-top)'
    p['--p2-neutral-fallback']='#8a929c'
    for(let i=0;i<10;i++){const h=HUES[i];p[`--p2-hue-${i}-bg`]=`hsl(${h} 50% 88%)`;p[`--p2-hue-${i}-bg2`]=`hsl(${h} 45% 80%)`;p[`--p2-hue-${i}-fg`]=`hsl(${h} 55% 26%)`}
    p['--p2-success-text']='#15723f';p['--p2-success-bg']='#dff7e8';p['--p2-danger-text']='#b1283a';p['--p2-danger-bg']='#ffe1e5';p['--p2-warning-text']='#8a5a00';p['--p2-warning-bg']='#fff0d6'
  }
  // Mod bağımsız vurgu türevleri
  const hover=parse(p['--p2-blue-500']),bar=parse(p['--p2-blue-300'])
  p['--p2-accent-soft']=rgba(accent,light?.14:.16);p['--p2-accent-soft-2']=rgba(accent,light?.24:.28)
  p['--p2-accent-grad']=`linear-gradient(135deg, ${hex(hover)}, ${hex(accent)})`
  p['--p2-accent-bar']=`linear-gradient(90deg, ${hex(hover)}, ${hex(bar)})`
  p['--p2-accent-glow']=`0 0 34px ${rgba(accent,.18)}`
  p['--p2-accent-glow-strong']=`0 6px 18px ${rgba(a(-.1),light?.3:.45)}`
  p['--p2-ring']=`0 0 0 3px ${rgba(hover,.35)}`
  p['--p2-text-on-accent']=accent.l>.62?hex(hsl(accent.h,.5,.08)):'#ffffff'
  return p
}
const HUES=[223,252,168,328,41,203,22,135,270,354]

export const THEME_TOKENS=Object.keys(paletteFor(DEFAULT_CUSTOM))

// Etkin temanın satır içi token'ları (P2 Mavi'de boş: CSS sabitleri geçerli). Ana süreçteki pencereler
// (oyun başlatma kartı) aynı görünümde çizilsin diye başlatma isteğiyle gönderilir.
export function currentThemeVars():{vars:Record<string,string>;mode:ThemeMode}{
  const vars:Record<string,string>={}
  if(typeof document==='undefined')return{vars,mode:'dark'}
  const style=document.documentElement.style
  for(const name of THEME_TOKENS){const value=style.getPropertyValue(name);if(value)vars[name]=value.trim()}
  return{vars,mode:document.documentElement.dataset.theme==='light'?'light':'dark'}
}

export function applyAppearance(appearance:Appearance){
  if(typeof document==='undefined')return
  const root=document.documentElement
  const theme=themeOf(appearance)
  if(theme.id===DEFAULT_THEME_ID){
    // Varsayılan tema CSS'teki sabit değerlerle çalışır; satır içi geçersiz kılmalar temizlenir.
    THEME_TOKENS.forEach(name=>root.style.removeProperty(name))
  }else{
    const palette=paletteFor(theme)
    THEME_TOKENS.forEach(name=>{if(palette[name])root.style.setProperty(name,palette[name]);else root.style.removeProperty(name)})
  }
  root.dataset.theme=theme.mode
  root.style.colorScheme=theme.mode
  // Ana süreç pencereleri (açılış, oyun başlatma kartı) aynı temayla çizilsin.
  try{(window as unknown as {p2?:{appearance?:{save?:(data:unknown)=>void}}}).p2?.appearance?.save?.(currentThemeVars())}catch{}
  const font=FONTS.find(item=>item.id===appearance.font)||FONTS[0]
  root.style.setProperty('--app-font',font.stack)
  root.style.setProperty('--app-font-scale',String((appearance.fontScale||85)/100))
  document.body.style.zoom=''
  // Sabit px kullanan mevcut kurallarda yalnızca yazıyı ölçekle; kutu ve yerleşimler değişmez.
  // (--p2-fs-* token'ları zaten ölçekle çarpılmış gelir, bu yürüyücü onlara dokunmaz.)
  const visit=(rules:CSSRuleList)=>Array.from(rules).forEach(rule=>{if(rule instanceof CSSStyleRule){const size=rule.style.fontSize;if(/^\d+(\.\d+)?px$/.test(size)&&!rule.style.getPropertyValue('--p2-font-scaled')){rule.style.setProperty('--p2-font-scaled','1');rule.style.fontSize=`calc(${size} * var(--app-font-scale))`}}else if('cssRules'in rule)try{visit((rule as CSSGroupingRule).cssRules)}catch{}})
  Array.from(document.styleSheets).forEach(sheet=>{try{visit(sheet.cssRules)}catch{}})
}
