import { useSyncExternalStore } from 'react'

// Arama balonu Workspace'in içinde yaşıyor ama başlık çubuğu (Titlebar) dışında.
// Aralarındaki tek bağ bu küçük veri yolu: aktif arama özeti + balonun görünürlüğü.
export type CallSummary={active:boolean;label:string;participants:number;muted:boolean;startedAt:number|null;connectedAt:number|null;ringing:boolean;expandRequest:number}

let summary:CallSummary={active:false,label:'',participants:0,muted:false,startedAt:null,connectedAt:null,ringing:false,expandRequest:0}
const listeners=new Set<()=>void>()
function emit(){listeners.forEach(listener=>listener())}

export function setCallSummary(patch:Partial<CallSummary>){
  const next={...summary,...patch}
  if(JSON.stringify(next)===JSON.stringify(summary))return
  summary=next
  emit()
}
export function getCallSummary(){return summary}
// Başlık çubuğundaki menüden çağrılır: panel zaten görünür, sahneyi büyütür.
export function showCallPanel(){setCallSummary({expandRequest:summary.expandRequest+1})}
export function useCallSummary(){return useSyncExternalStore(listener=>{listeners.add(listener);return()=>{listeners.delete(listener)}},getCallSummary,getCallSummary)}
