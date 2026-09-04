import { lazy, ReactNode, RefObject, Suspense, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { ComposerIcon } from './ComposerIcons'
import { useEscape } from './useEscape'

// Mesaj menüsünün ortak parçaları: bağlam menüsü, tepki şeridi ve tepki seçici.
// Kanal, bireysel ve grup sohbetleri aynı yapıyı kullanır.
const EmojiPanel=lazy(()=>import('./EmojiPanel'))
const MENU_EDGE_GAP=12

export type Reaction={emoji:string;count:number;reacted:boolean}
export type MessageMenu={x:number;y:number;up:number;right?:boolean}
function useMenuPlacement(menu:MessageMenu|null,ref:RefObject<HTMLDivElement|null>){const[place,setPlace]=useState<{left:number;top:number}|null>(null);useLayoutEffect(()=>{const node=ref.current;if(!menu||!node){setPlace(null);return}const measure=()=>{const box=node.getBoundingClientRect(),gap=MENU_EDGE_GAP;const left=Math.max(gap,Math.min(menu.right?menu.x-box.width:menu.x,window.innerWidth-box.width-gap));const fitsBelow=menu.y+box.height<=window.innerHeight-gap;const top=Math.max(gap,Math.min(fitsBelow?menu.y:menu.up-box.height,window.innerHeight-box.height-gap));setPlace({left,top})};measure();const observer=new ResizeObserver(measure);observer.observe(node);window.addEventListener('resize',measure);return()=>{observer.disconnect();window.removeEventListener('resize',measure)}},[menu,ref]);return place}
export function MessageMenuPopup({menu,onClose,className,children}:{menu:MessageMenu;onClose:()=>void;className?:string;children:ReactNode}){const menuRef=useRef<HTMLDivElement>(null),place=useMenuPlacement(menu,menuRef);useEscape(true,onClose);return createPortal(<div className="message-menu-layer" onMouseDown={onClose}><div ref={menuRef} className={className||'message-context-menu'} style={{left:place?.left??menu.x,top:place?.top??menu.y,visibility:place?'visible':'hidden'}} onMouseDown={e=>e.stopPropagation()}>{children}</div></div>,document.body)}
const quickReactions=['💯','⭐','❤️','🌷']
export function ReactionStrip({onPick}:{onPick:(emoji:string)=>void}){return <div className="reaction-strip">{quickReactions.map(emoji=><button key={emoji} type="button" onClick={()=>onPick(emoji)} title={`${emoji} tepkisi ver`}>{emoji}</button>)}</div>}
export function anchorFrom(box:DOMRect,right?:boolean):MessageMenu{return {x:right?box.right:box.left,y:box.bottom+6,up:box.top-6,right}}
export function useReactionPicker(){const[menu,setMenu]=useState<MessageMenu|null>(null);return {menu,openAt:(box:DOMRect,right?:boolean)=>setMenu(anchorFrom(box,right)),openMenu:(value:MessageMenu)=>setMenu(value),close:()=>setMenu(null)}}
export function ReactionPicker({menu,onClose,onSelect}:{menu:MessageMenu;onClose:()=>void;onSelect:(emoji:string)=>void}){return <MessageMenuPopup menu={menu} onClose={onClose} className="reaction-picker-popup"><Suspense fallback={<div className="emoji-loading">Emojiler hazırlanıyor…</div>}><EmojiPanel height={352} onSelect={emoji=>{onSelect(emoji);onClose()}}/></Suspense></MessageMenuPopup>}
export function ReactionBar({reactions,onToggle}:{reactions:Reaction[];onToggle:(emoji:string)=>void;onAddAt?:(box:DOMRect)=>void;always?:boolean}){if(!reactions.length)return null;return <div className="message-reactions">{reactions.map(item=><button type="button" key={item.emoji} className={item.reacted?'reaction-pill active':'reaction-pill'} onClick={()=>onToggle(item.emoji)} title={item.reacted?'Tepkini kaldır':'Tepki ver'}><span>{item.emoji}</span><b>{item.count}</b></button>)}</div>}

// Tüm sohbetlerde ortak mesaj araçları: üstteki küçük düğmeler + üç nokta menüsü.
export type MessageActions={
  own:boolean
  content:string
  editedAt?:string|null
  link?:string
  reactions?:Reaction[]
  onReact:(emoji:string)=>void
  onQuote:()=>void
  onEdit:()=>void
  onDelete:()=>void
  onThread?:()=>void
  onForward?:()=>void
  onHistory?:()=>void
}

export function useMessageTools(){
  const[menu,setMenu]=useState<MessageMenu|null>(null)
  const picker=useReactionPicker()
  return{
    menu,setMenu,picker,
    openFrom:(box:DOMRect)=>setMenu(anchorFrom(box,true)),
    openAtPoint:(x:number,y:number)=>setMenu({x,y,up:y}),
    close:()=>setMenu(null),
  }
}

export function MessageTools({tools,actions}:{tools:ReturnType<typeof useMessageTools>;actions:MessageActions}){
  const copy=(value:string)=>{void navigator.clipboard.writeText(value);tools.setMenu(null)}
  return <>
    <div className="message-actions">
      <button type="button" title="Tepki ekle" aria-label="Tepki ekle" onClick={event=>tools.picker.openAt(event.currentTarget.getBoundingClientRect(),true)}><ComposerIcon name="reaction"/></button>
      <button type="button" title="Mesajı alıntıla" aria-label="Mesajı alıntıla" onClick={actions.onQuote}><ComposerIcon name="quote"/></button>
      {actions.onThread&&<button type="button" title="Thread'de yanıtla" aria-label="Thread'de yanıtla" onClick={actions.onThread}><ComposerIcon name="thread"/></button>}
      <button type="button" className="more" title="Diğer işlemler" aria-label="Diğer işlemler" onClick={event=>tools.openFrom(event.currentTarget.getBoundingClientRect())}><ComposerIcon name="more"/></button>
    </div>
    {tools.menu&&<MessageMenuPopup menu={tools.menu} onClose={()=>tools.setMenu(null)}>
      <ReactionStrip onPick={emoji=>{actions.onReact(emoji);tools.setMenu(null)}}/>
      <button className="menu-react" onClick={()=>{tools.picker.openMenu(tools.menu!);tools.setMenu(null)}}><span>☺</span>Tepki ekle<i>›</i></button>
      <hr/>
      <button onClick={()=>{actions.onQuote();tools.setMenu(null)}}><span>❝</span>Mesajı alıntıla</button>
      {actions.own&&<button onClick={()=>{actions.onEdit();tools.setMenu(null)}}><span>✎</span>Mesajı düzenle</button>}
      {actions.onThread&&<button onClick={()=>{actions.onThread!();tools.setMenu(null)}}><span>↩</span>Thread'de yanıtla</button>}
      {actions.onForward&&<button onClick={()=>{actions.onForward!();tools.setMenu(null)}}><span>↗</span>İlet</button>}
      <hr/>
      <button onClick={()=>copy(actions.content)}><span>▤</span>Metni kopyala</button>
      <button className="menu-coming"><span>⚑</span>Mesajı sabitle<small>yakında</small></button>
      {actions.editedAt&&actions.onHistory&&<button onClick={()=>{actions.onHistory!();tools.setMenu(null)}}><span>🕘</span>Düzenleme geçmişi</button>}
      {actions.link&&<button onClick={()=>copy(actions.link!)}><span>🔗</span>Mesaj bağlantısını kopyala</button>}
      {actions.own&&<><hr/><button className="danger" onClick={()=>{actions.onDelete();tools.setMenu(null)}}><span>🗑</span>Mesajı sil</button></>}
    </MessageMenuPopup>}
    {tools.picker.menu&&<ReactionPicker menu={tools.picker.menu} onClose={tools.picker.close} onSelect={emoji=>{actions.onReact(emoji);tools.picker.close()}}/>}
  </>
}
