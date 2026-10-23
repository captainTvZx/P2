import type { TaskPriority } from './api/tasks'

// Oyun içi "Hata bildir" için saf yardımcılar (vitest). Görev açıklaması sohbet/görev biçimindedir:
// markdown satırları + sondaki ek işareti (attachmentMarker). Bileşen/DOM bağımlılığı yok.

export const BUG_SEVERITIES:{value:TaskPriority;label:string}[]=[
  {value:'LOW',label:'Düşük'},
  {value:'MEDIUM',label:'Orta'},
  {value:'HIGH',label:'Yüksek'},
  {value:'URGENT',label:'Acil'},
]
export const BUG_DESCRIPTION_LIMIT=12000

export type BugReportContext={
  projectName:string
  version?:string|null          // kurulu sürüm (Kütüphane)
  channel?:string|null          // sürüm kategorisi (alpha/beta/…)
  releaseId?:string|null
  clientVersion?:string|null    // P2 istemci sürümü
  os?:string|null               // varsayılan Windows
  playSeconds?:number|null      // bu oturum oynama süresi (varsa)
  description:string            // kullanıcının yazdığı metin
  attachment?:string|null       // attachmentMarker çıktısı (varsa)
}

// 65 → "01:05", 3725 → "1:02:05"
export function formatPlaySeconds(seconds:number){
  const total=Math.max(0,Math.floor(seconds))
  const h=Math.floor(total/3600),m=Math.floor((total%3600)/60),s=total%60
  const two=(n:number)=>String(n).padStart(2,'0')
  return h>0?`${h}:${two(m)}:${two(s)}`:`${two(m)}:${two(s)}`
}

function versionLine(ctx:BugReportContext){
  if(!ctx.version)return 'Sürüm: kurulu sürüm bilinmiyor'
  let line=`Sürüm: ${ctx.version}`
  if(ctx.channel)line+=` (${ctx.channel})`
  if(ctx.releaseId)line+=` · Sürüm kimliği: ${ctx.releaseId}`
  return line
}

export function buildBugDescription(ctx:BugReportContext){
  const lines=[
    '**Hata bildirimi** (oyun içi overlay)',
    `Proje: ${ctx.projectName||'—'}`,
    versionLine(ctx),
    `İstemci: P2 ${ctx.clientVersion||'?'}`,
    `İşletim sistemi: ${ctx.os||'Windows'}`,
  ]
  if(typeof ctx.playSeconds==='number'&&ctx.playSeconds>0)lines.push(`Oynama süresi (bu oturum): ${formatPlaySeconds(ctx.playSeconds)}`)
  lines.push('---')
  const body=ctx.description.trim()
  if(body)lines.push(body)
  const head=lines.join('\n')
  const marker=ctx.attachment?.trim()||''
  // Ek işareti her zaman sona ve kendi satırına; metin sınırı aşarsa açıklama kırpılır, ek korunur.
  const room=BUG_DESCRIPTION_LIMIT-(marker?marker.length+1:0)
  const text=head.length>room?head.slice(0,Math.max(0,room-1)).trimEnd()+'…':head
  return marker?`${text}\n${marker}`:text
}

// "Kanala da yaz": projenin 'genel' adlı TEXT kanalı, yoksa ilk TEXT kanal.
export type ReportChannel={id:string;name:string;type:string}
export function pickReportChannel(categories:{channels:ReportChannel[]}[]):ReportChannel|null{
  const text=categories.flatMap(category=>category.channels||[]).filter(channel=>channel.type==='TEXT')
  return text.find(channel=>channel.name.toLocaleLowerCase('tr-TR')==='genel')||text[0]||null
}

// Kanal mesajı: başlık + önem, sonra görev bağlantısı (numara) ve ek.
export function buildBugChannelMessage(input:{title:string;severityLabel:string;taskNumber?:number|null;attachment?:string|null}){
  const lines=[`**Hata bildirimi:** ${input.title.trim()} · Önem: ${input.severityLabel}${input.taskNumber?` · Görev #${input.taskNumber}`:''}`]
  if(input.attachment)lines.push(input.attachment)
  return lines.join('\n')
}
