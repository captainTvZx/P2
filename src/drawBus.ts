// Yayın üzerine kalemle çizim: çizgiler herkese iletilir, kısa süre sonra yavaşça silinir.
export type StrokePoint={x:number;y:number}   // yayın görüntüsüne göre 0..1 arası konum
export type StrokeMessage={target:string;id:string;color:string;width:number;points:StrokePoint[];from:string}

type Handler=(message:StrokeMessage)=>void
const handlers=new Map<string,Set<Handler>>()

export function subscribeStrokes(target:string,handler:Handler){
  const set=handlers.get(target)||new Set<Handler>()
  handlers.set(target,set)
  set.add(handler)
  return()=>{set.delete(handler);if(!set.size)handlers.delete(target)}
}
export function publishStroke(message:StrokeMessage){
  handlers.get(message.target)?.forEach(handler=>handler(message))
}

// Kalem imleci: kişi çizmeden gezdirirken de diğerleri imleci ve adını görür.
export type PointerMessage={target:string;from:string;name:string;color:string;x:number;y:number;gone?:boolean}
const pointerHandlers=new Map<string,Set<(message:PointerMessage)=>void>>()
export function subscribePointers(target:string,handler:(message:PointerMessage)=>void){
  const set=pointerHandlers.get(target)||new Set<(message:PointerMessage)=>void>()
  pointerHandlers.set(target,set)
  set.add(handler)
  return()=>{set.delete(handler);if(!set.size)pointerHandlers.delete(target)}
}
export function publishPointer(message:PointerMessage){
  pointerHandlers.get(message.target)?.forEach(handler=>handler(message))
}

// Kişi başına sabit kalem rengi (avatar tonlarıyla aynı mantık).
export const PEN_PALETTE=['#ff6b6b','#ffa94d','#ffd43b','#69db7c','#4dabf7','#da77f2','#ffffff','#14181d']
const PENS=['#ff6b6b','#ffa94d','#ffd43b','#69db7c','#38d9a9','#4dabf7','#748ffc','#da77f2','#f783ac','#63e6be']
export function penColor(key:string){
  let hash=0
  for(let index=0;index<key.length;index++)hash=(hash*31+key.charCodeAt(index))>>>0
  return PENS[hash%PENS.length]
}
