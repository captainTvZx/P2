import { API_BASE, p2Fetch } from '../apiBase'

// Tek JSON istek yardımcısı: DirectMessages.tsx / ChannelContent.tsx / Workspace.tsx /
// library.ts içindeki yerel `api()` kopyalarının ortak hali. Hata gövdesi olduğu gibi fırlatılır
// ({code, message}); 401'de p2Fetch jetonu yeniler (overlay'de ana pencereden kiralar) ve yineler.
export async function request<T>(path:string,token:string,init?:RequestInit):Promise<T>{
  const response=await p2Fetch(API_BASE+path,{...init,headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`,...(init?.headers as Record<string,string>|undefined)}})
  const data=await response.json().catch(()=>({}))
  if(!response.ok)throw data
  return data as T
}
