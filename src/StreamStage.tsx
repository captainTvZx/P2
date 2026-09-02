import { useCall } from './CallProvider'
import StreamPlayer from './StreamPlayer'

// Aynı anda birden fazla yayın izlenebilir: kutucuklar Meet gibi bölünür.
export type StageItem={userId:string;stream:MediaStream;label:string;self?:boolean}

export default function StreamStage({items,onClose}:{items:StageItem[];onClose:(userId:string)=>void}){
  const call=useCall()
  if(!items.length)return null
  return <div className={`stream-stage count-${Math.min(items.length,4)}`}>
    {items.map(item=><StreamPlayer key={item.userId} userId={item.userId} stream={item.stream} label={item.label} self={item.self}
      quality={item.self?undefined:call.viewerQuality[item.userId]}
      onQuality={item.self?undefined:level=>call.requestQuality(item.userId,level)}
      onClose={item.self?undefined:()=>onClose(item.userId)}/>)}
  </div>
}
