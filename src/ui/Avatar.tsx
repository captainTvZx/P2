import { avatarHue, userInitials } from '../initials'

// Tek avatar bileşeni: baş harfler ya da fotoğraf, kimliğe bağlı ton (.hue-N), isteğe bağlı durum noktası.
type Size='xs'|'sm'|'md'|'lg'|'xl'|'xxl'
export default function Avatar({id,name,url,size='md',ring,round,status,className=''}:{
  id:string
  name:string
  url?:string|null
  size?:Size
  ring?:boolean
  round?:boolean
  status?:'online'|'away'|'busy'|'offline'|null
  className?:string
}){
  const avatar=<span className={`p2-avatar ${size} ${avatarHue(id)}${ring?' ring':''}${round?' round':''}${className?' '+className:''}`} aria-hidden="true">
    {url?<img src={url} alt="" draggable={false}/>:userInitials(name)}
  </span>
  if(!status)return avatar
  return <span className="p2-avatar-wrap">{avatar}<i className={`p2-avatar-status ${status}`}/></span>
}
