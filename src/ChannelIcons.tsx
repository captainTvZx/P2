export type ChannelKind='TEXT'|'ANNOUNCEMENT'|'VOICE'|'FORUM'

export default function ChannelIcon({type}:{type:ChannelKind}){
  if(type==='TEXT')return <span aria-hidden="true">#</span>
  if(type==='ANNOUNCEMENT')return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9ZM10 21h4"/></svg>
  if(type==='VOICE')return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M11 5 6.5 9H3v6h3.5L11 19V5ZM15 9.5a4 4 0 0 1 0 5M18 7a7.5 7.5 0 0 1 0 10"/></svg>
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5h12a3 3 0 0 1 3 3v5a3 3 0 0 1-3 3H9l-5 4v-4a3 3 0 0 1-3-3V8a3 3 0 0 1 3-3Z"/><path d="M8 9h7M8 12h5"/></svg>
}
