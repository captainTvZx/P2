import { createContext, useContext } from 'react'

// Mesaj içindeki @kullanıcı etiketine tıklanınca profili açan geri çağrı.
export const MentionContext=createContext<((username:string)=>void)|null>(null)
export function useMentionOpener(){return useContext(MentionContext)}
