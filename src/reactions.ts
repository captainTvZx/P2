import type { Reaction } from './messageMenu'

// Sunucu tepki satırlarını ham gönderir; arayüz emoji başına toplar.
export function groupReactionRows(rows:{emoji:string;userId:string}[]|undefined,me:string):Reaction[]{
  if(!rows?.length)return []
  const map=new Map<string,Reaction>()
  for(const row of rows){
    const item=map.get(row.emoji)||{emoji:row.emoji,count:0,reacted:false}
    item.count+=1
    if(row.userId===me)item.reacted=true
    map.set(row.emoji,item)
  }
  return [...map.values()]
}
