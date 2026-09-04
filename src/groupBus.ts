import type { GroupMessage } from './groupTypes'

// Grup thread yanıtları: olay Workspace'te yakalanır, açık thread paneline buradan iletilir.
export type ReplyEvent={groupId:string;parentId:string;message:GroupMessage;replies:number}
const handlers=new Map<string,Set<(event:ReplyEvent)=>void>>()

export function subscribeGroupReplies(groupId:string,handler:(event:ReplyEvent)=>void){
  const set=handlers.get(groupId)||new Set<(event:ReplyEvent)=>void>()
  handlers.set(groupId,set)
  set.add(handler)
  return()=>{set.delete(handler);if(!set.size)handlers.delete(groupId)}
}
export function publishGroupReply(event:ReplyEvent){handlers.get(event.groupId)?.forEach(handler=>handler(event))}

// Tepki güncellemeleri: açık thread panelindeki yanıtlar da tazelensin.
export type ReactionEvent={groupId:string;messageId:string;rows:{emoji:string;userId:string}[]}
const reactionHandlers=new Map<string,Set<(event:ReactionEvent)=>void>>()
export function subscribeGroupReactions(groupId:string,handler:(event:ReactionEvent)=>void){
  const set=reactionHandlers.get(groupId)||new Set<(event:ReactionEvent)=>void>()
  reactionHandlers.set(groupId,set)
  set.add(handler)
  return()=>{set.delete(handler);if(!set.size)reactionHandlers.delete(groupId)}
}
export function publishGroupReactions(event:ReactionEvent){reactionHandlers.get(event.groupId)?.forEach(handler=>handler(event))}
