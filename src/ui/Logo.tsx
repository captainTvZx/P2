import p2Icon from '../assets/p2-icon.png'

// P2 logosu: metin "P2" işaretleri yerine gerçek simge.
export default function Logo({size='md',pop,className=''}:{size?:'xs'|'sm'|'md'|'lg'|'xl';pop?:boolean;className?:string}){
  return <img src={p2Icon} alt="P2" draggable={false} className={`p2-logo ${size}${pop?' pop':''}${className?' '+className:''}`}/>
}
