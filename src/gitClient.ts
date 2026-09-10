// Git istemcisinin masaüstü köprüsü. Depo listesi cihaza özgü olduğu için
// localStorage'da tutulur; git komutları her zaman ana süreçte çalışır.
export type GitFile={path:string;from:string|null;staged:boolean;unstaged:boolean;index:string;work:string;untracked:boolean;conflicted:boolean}
export type GitStatus={branch:string;upstream:string;ahead:number;behind:number;oid:string;files:GitFile[];stashes:number}
export type GitCommit={hash:string;short:string;author:string;email:string;at:number;subject:string;parents:string[];refs:string[]}
export type GitBranch={name:string;upstream:string;short:string;at:number;current:boolean;subject:string;kind:'local'|'remote'}
export type GitRemote={name:string;url:string}
export type GitLfs={installed:boolean;version?:string;patterns:string[];files:string[]}
export type GitInfo={ok:true;root:string;name:string;status:GitStatus;branches:{local:GitBranch[];remote:GitBranch[]};remotes:GitRemote[];log:GitCommit[];lfs:GitLfs}
export type GitFail={ok:false;error:string;auth?:boolean}

export type Repo={path:string;name:string}
const KEY='p2:repos'

export const gitBridge=()=>(typeof window!=='undefined'?window.p2?.git:undefined)
export const gitAvailable=()=>Boolean(gitBridge())

export function loadRepos():Repo[]{
  try{const raw=window.localStorage.getItem(KEY);return raw?JSON.parse(raw):[]}catch{return []}
}
export function saveRepos(repos:Repo[]){
  try{window.localStorage.setItem(KEY,JSON.stringify(repos))}catch{}
}

// Dosya durumundan okunur bir etiket: arayüz rozetleri buradan gelir.
export function fileState(file:GitFile){
  if(file.conflicted)return {label:'ÇAKIŞMA',tone:'conflict'}
  if(file.untracked)return {label:'YENİ',tone:'added'}
  const code=file.staged?file.index:file.work
  if(code==='A')return {label:'EKLENDİ',tone:'added'}
  if(code==='D')return {label:'SİLİNDİ',tone:'deleted'}
  if(code==='R')return {label:'TAŞINDI',tone:'moved'}
  if(code==='C')return {label:'KOPYA',tone:'moved'}
  return {label:'DEĞİŞTİ',tone:'changed'}
}

export function relativeTime(at:number){
  const diff=(Date.now()-at)/1000
  if(diff<60)return 'az önce'
  if(diff<3600)return `${Math.floor(diff/60)} dk önce`
  if(diff<86400)return `${Math.floor(diff/3600)} sa önce`
  if(diff<2592000)return `${Math.floor(diff/86400)} gün önce`
  return new Date(at).toLocaleDateString('tr-TR',{day:'numeric',month:'long',year:'numeric'})
}
