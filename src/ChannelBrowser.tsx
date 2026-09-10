import { useState } from 'react'
import ChannelIcon from './ChannelIcons'
import { useEscape } from './useEscape'

type Channel={id:string;name:string;topic?:string|null;type:'TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM';position:number}
type Category={id:string;name:string;channels:Channel[]}

const typeLabels:Record<Channel['type'],string>={TEXT:'Metin kanalı',ANNOUNCEMENT:'Duyuru kanalı',VOICE:'Sesli oda',FORUM:'Forum'}

// Projedeki tüm kanalların listesi: ayrıldığın kanallara buradan geri katılırsın.
export default function ChannelBrowser({categories,prefs,onClose,onToggle,onOpen}:{
  categories:Category[]
  prefs:Record<string,{starred:boolean;notify:string;hidden?:boolean}>
  onClose:()=>void
  onToggle:(channelId:string,hidden:boolean)=>void
  onOpen:(channel:Channel)=>void
}){
  const[search,setSearch]=useState('')
  const[filter,setFilter]=useState<'all'|'joined'|'left'>('all')
  useEscape(true,onClose)
  const normalized=search.trim().toLocaleLowerCase('tr-TR')
  const rows=categories.flatMap(category=>category.channels.map(channel=>({channel,category})))
    .filter(({channel})=>{
      const hidden=!!prefs[`CHANNEL:${channel.id}`]?.hidden
      if(filter==='joined'&&hidden)return false
      if(filter==='left'&&!hidden)return false
      if(!normalized)return true
      return channel.name.toLocaleLowerCase('tr-TR').includes(normalized)||(channel.topic||'').toLocaleLowerCase('tr-TR').includes(normalized)
    })
  const leftCount=categories.flatMap(category=>category.channels).filter(channel=>prefs[`CHANNEL:${channel.id}`]?.hidden).length

  return <div className="action-modal-layer" onMouseDown={onClose}><section className="message-action-dialog channel-browser" onMouseDown={event=>event.stopPropagation()}>
    <header><div><small>KANALLAR</small><h3>Kanallara göz at</h3></div><button type="button" onClick={onClose}>×</button></header>
    <div className="channel-browser-filters">
      <div className="channel-browser-search"><span>⌕</span><input value={search} onChange={event=>setSearch(event.target.value)} placeholder="Kanal ara" autoFocus/></div>
      <div className="channel-browser-tabs">
        <button type="button" className={filter==='all'?'active':undefined} onClick={()=>setFilter('all')}>Tümü</button>
        <button type="button" className={filter==='joined'?'active':undefined} onClick={()=>setFilter('joined')}>Katıldıkların</button>
        <button type="button" className={filter==='left'?'active':undefined} onClick={()=>setFilter('left')}>Ayrıldıkların{leftCount?` · ${leftCount}`:''}</button>
      </div>
    </div>
    <div className="channel-browser-list">
      {rows.map(({channel,category})=>{
        const hidden=!!prefs[`CHANNEL:${channel.id}`]?.hidden
        return <div key={channel.id} className={hidden?'channel-browser-row left':'channel-browser-row'}>
          <span className="channel-browser-icon"><ChannelIcon type={channel.type}/></span>
          <button type="button" className="channel-browser-name" disabled={hidden} onClick={()=>onOpen(channel)}>
            <b>{channel.name}</b>
            <small>{channel.topic?.trim()||`${typeLabels[channel.type]} · ${category.name}`}</small>
          </button>
          <button type="button" className={hidden?'channel-browser-action join':'channel-browser-action'} onClick={()=>onToggle(channel.id,!hidden)}>{hidden?'Katıl':'Ayrıl'}</button>
        </div>
      })}
      {!rows.length&&<p className="channel-browser-empty">{normalized?'Aramanla eşleşen kanal yok.':'Bu listede kanal yok.'}</p>}
    </div>
  </section></div>
}
