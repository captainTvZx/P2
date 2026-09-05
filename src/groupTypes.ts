// Grup sohbetleri için ortak tipler.
export type GroupMember={id:string;username:string;displayName:string;status?:string;avatarUrl?:string|null}
export type ChatGroup={
  id:string
  name:string
  channelId:string|null
  createdById:string
  updatedAt:string
  members:GroupMember[]
  lastReadAt:string|null
  unread:number
  lastMessage:{id:string;content:string;createdAt:string;authorId:string}|null
}
export type GroupMessage={
  id:string
  content:string
  kind:string
  createdAt:string
  editedAt:string|null
  author:{id:string;username:string;displayName:string}
  quotedMessage?:{id:string;content:string;author:{displayName:string}}|null
  reactions?:{emoji:string;userId:string}[]
  parentId?:string|null
  _count?:{replies:number}
  replies?:{id:string;createdAt:string;author:{id:string;displayName:string}}[]
}
