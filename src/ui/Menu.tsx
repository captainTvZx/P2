import type { ReactNode } from 'react'
import { MessageMenuPopup, type MessageMenu } from '../messageMenu'

// Bağlam menüsü / popover: mevcut yerleşim mantığı (MessageMenuPopup) + P2 Mavi görünümü.
export default function Menu({anchor,onClose,origin,children}:{anchor:MessageMenu;onClose:()=>void;origin?:'top left'|'top right'|'bottom left'|'bottom right';children:ReactNode}){
  return <MessageMenuPopup menu={anchor} onClose={onClose} className="p2-menu anim-menu" origin={origin||(anchor.right?'top right':'top left')}>{children}</MessageMenuPopup>
}

export function MenuItem({icon,label,hint,kbd,danger,disabled,onSelect}:{icon?:ReactNode;label:ReactNode;hint?:ReactNode;kbd?:string;danger?:boolean;disabled?:boolean;onSelect:()=>void}){
  return <button type="button" className={`p2-menu-item${danger?' danger':''}`} disabled={disabled} onClick={onSelect}>
    <span className="p2-menu-icon">{icon}</span>
    <span>{label}</span>
    {kbd?<kbd>{kbd}</kbd>:hint?<small>{hint}</small>:<span/>}
  </button>
}

export function MenuLabel({children}:{children:ReactNode}){return <span className="p2-menu-label p2-kicker">{children}</span>}
export function MenuDivider(){return <hr/>}
