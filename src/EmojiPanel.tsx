import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import EmojiPicker, { Categories, EmojiStyle, SuggestionMode, Theme, type EmojiClickData } from 'emoji-picker-react'
import { useEscape } from './useEscape'

// Windows'un Segoe UI Emoji fontu bayrak dizilerini çizemiyor; bayraklar "DE", "EE" gibi
// harf çiftleri olarak görünüyor. Bu yüzden bayrak kategorisi listelenmiyor ve daha önce
// seçilmiş bayraklar "son kullanılanlar" listesinden temizleniyor.
const categories=[{category:Categories.SUGGESTED,name:'Son kullanılanlar'},{category:Categories.SMILEYS_PEOPLE,name:'İfadeler ve insanlar'},{category:Categories.ANIMALS_NATURE,name:'Hayvanlar ve doğa'},{category:Categories.FOOD_DRINK,name:'Yiyecek ve içecek'},{category:Categories.TRAVEL_PLACES,name:'Seyahat ve mekanlar'},{category:Categories.ACTIVITIES,name:'Etkinlikler'},{category:Categories.OBJECTS,name:'Nesneler'},{category:Categories.SYMBOLS,name:'Semboller'}]
const flagCodepoint=/1f1[e-f][0-9a-f]/i
function pruneFlagSuggestions(){try{const raw=window.localStorage.getItem('epr_suggested');if(!raw)return;const list=JSON.parse(raw);if(!Array.isArray(list))return;const kept=list.filter((item:{unified?:string})=>!flagCodepoint.test(item?.unified||''));if(kept.length!==list.length)window.localStorage.setItem('epr_suggested',JSON.stringify(kept))}catch{}}
pruneFlagSuggestions()

export default function EmojiPanel({height=352,onSelect,onDismiss}:{height?:number;onSelect:(emoji:string)=>void;onDismiss?:()=>void}){
  const[selected,setSelected]=useState<EmojiClickData|null>(null)
  const panel=useRef<HTMLDivElement>(null)
  // Uygulamanın genel button stilleri aktif kategori düğmesinin sprite
  // background-image değerini silebiliyor. Görünen pasif düğmeden gerçek sprite
  // URL'sini okuyup bütün kategori düğmelerine inline ver; aktif sınıf değişse de
  // ikon kaybolmasın.
  useLayoutEffect(()=>{
    const root=panel.current
    if(!root)return
    const restore=()=>{
      const buttons=Array.from(root.querySelectorAll<HTMLButtonElement>('.epr-category-nav .epr-cat-btn'))
      const source=buttons.find(button=>getComputedStyle(button).backgroundImage!=='none')
      const sprite=source&&getComputedStyle(source).backgroundImage
      if(!sprite||sprite==='none')return
      buttons.forEach(button=>{button.style.backgroundImage=sprite})
    }
    restore()
    const observer=new MutationObserver(restore)
    observer.observe(root,{subtree:true,attributes:true,attributeFilter:['class']})
    return()=>observer.disconnect()
  },[])
  useEscape(Boolean(onDismiss),()=>onDismiss?.())
  useEffect(()=>{
    if(!onDismiss)return
    const close=(event:PointerEvent)=>{
      const target=event.target as HTMLElement|null
      if(!target||panel.current?.contains(target)||target.closest('.emoji-toggle'))return
      onDismiss()
    }
    document.addEventListener('pointerdown',close,true)
    return()=>document.removeEventListener('pointerdown',close,true)
  },[onDismiss])
  const choose=(item:EmojiClickData)=>{setSelected(item);onSelect(item.emoji)}
  return <div ref={panel} className="p2-emoji-panel"><header><div><b>Emoji seç</b><small>Ara veya bir kategori seç</small></div><span>{selected?.emoji||'😊'}</span></header><div className="emoji-quick-row">{['😀','😂','😍','👍','🔥','🎉','❤️','👀'].map(emoji=><button type="button" key={emoji} onClick={()=>onSelect(emoji)}>{emoji}</button>)}</div><EmojiPicker categories={categories} theme={document.documentElement.dataset.theme==='light'?Theme.LIGHT:Theme.DARK} emojiStyle={EmojiStyle.NATIVE} suggestedEmojisMode={SuggestionMode.RECENT} skinTonesDisabled lazyLoadEmojis searchPlaceholder="Emoji ara…" searchClearButtonLabel="Temizle" width="100%" height={height-112} previewConfig={{showPreview:false}} onEmojiClick={choose}/><footer><span>{selected?.emoji||'✨'}</span><div><b>{selected?.names.at(-1)||'Bir emoji seç'}</b><small>{selected?`:${selected.names.at(-1)?.replace(/\s+/g,'_')}:`:'Mesajına eklemek için tıkla'}</small></div></footer></div>
}
