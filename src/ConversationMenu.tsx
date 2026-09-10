import { Fragment, useState } from 'react'
import { ComposerIcon } from './ComposerIcons'
import { anchorFrom, MessageMenuPopup, type MessageMenu } from './messageMenu'

export type MenuItem={icon:string;label:string;danger?:boolean;disabled?:boolean;separated?:boolean;onSelect:()=>void}

// Kanal başlığındaki ••• menüsünün aynısı; bireysel ve grup sohbetleri de aynı
// yapıyı kullansın diye tek yerde duruyor.
export default function ConversationMenu({items,label='Diğer işlemler'}:{items:MenuItem[];label?:string}){
  const[menu,setMenu]=useState<MessageMenu|null>(null)
  return <>
    <button type="button" className={menu?'tool more active':'tool more'} title={label} aria-label={label}
      onClick={event=>setMenu(menu?null:anchorFrom(event.currentTarget.getBoundingClientRect(),true))}><ComposerIcon name="more"/></button>
    {menu&&<MessageMenuPopup menu={menu} onClose={()=>setMenu(null)} className="message-context-menu tool-menu">
      {items.map((item,index)=><Fragment key={item.label}>
        {item.separated&&index>0&&<hr/>}
        <button type="button" className={item.danger?'danger':undefined} disabled={item.disabled} onClick={()=>{setMenu(null);item.onSelect()}}>
          <span>{item.icon}</span>{item.label}
        </button>
      </Fragment>)}
    </MessageMenuPopup>}
  </>
}
