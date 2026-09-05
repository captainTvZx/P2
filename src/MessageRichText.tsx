import { Fragment, ReactNode, useEffect, useRef, useState } from 'react'
import { useMentionOpener } from './mentions'
import { formatSize, type UploadedFile } from './attachments'

const tokenPattern=/(\*\*[^*\n]+\*\*|\*[^*\n]+\*|\[[^\]]+\]\(https?:\/\/[^\s)]+\)|https?:\/\/[^\s]+|@[a-zA-Z0-9_.-]+)/g
function youtubeId(url:string){try{const parsed=new URL(url);if(parsed.hostname==='youtu.be')return parsed.pathname.slice(1).split('/')[0];if(parsed.hostname.endsWith('youtube.com'))return parsed.searchParams.get('v')||(/^\/shorts\/([^/]+)/.exec(parsed.pathname)?.[1]??null)}catch{}return null}
function inline(text:string,onMention:((username:string)=>void)|null):ReactNode[]{return text.split(tokenPattern).filter(Boolean).map((part,index)=>{if(part.startsWith('**')&&part.endsWith('**'))return <strong key={index}>{part.slice(2,-2)}</strong>;if(part.startsWith('*')&&part.endsWith('*'))return <em key={index}>{part.slice(1,-1)}</em>;const markdown=/^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/.exec(part);if(markdown)return <a key={index} href={markdown[2]} target="_blank" rel="noreferrer">{markdown[1]}</a>;if(/^https?:\/\//.test(part))return <a key={index} href={part} target="_blank" rel="noreferrer">{part}</a>;if(part.startsWith('@'))return onMention?<button type="button" className="mention" key={index} title={`${part} profilini aç`} onClick={()=>onMention(part.slice(1))}>{part}</button>:<mark className="mention" key={index}>{part}</mark>;return <Fragment key={index}>{part}</Fragment>})}

const filePattern=/^\[\[p2-file\|([^|]+)\|([^|]+)\|(\d+)\|(https?:\/\/[^\]]+)\]\]$/
function VideoPreview({file}:{file:UploadedFile}){
  const video=useRef<HTMLVideoElement>(null)
  async function fullscreen(){const element=video.current;if(!element)return;try{if(element.requestFullscreen)await element.requestFullscreen();else await (element as any).webkitEnterFullscreen?.()}catch{}}
  return <div className="message-video"><video ref={video} src={file.url} controls preload="metadata" playsInline/><button type="button" className="message-video-fullscreen" onClick={fullscreen}>Tam ekran</button><span>{file.name} · {formatSize(file.size)}</span></div>
}
function TextPreview({file}:{file:UploadedFile}){
  const[text,setText]=useState(''),[error,setError]=useState('')
  useEffect(()=>{let active=true;fetch(file.url).then(response=>{if(!response.ok)throw new Error();return response.text()}).then(value=>{if(active)setText(value.slice(0,100000))}).catch(()=>{if(active)setError('Metin önizlemesi yüklenemedi.')});return()=>{active=false}},[file.url])
  return <section className="message-text-preview"><header><span>{file.name} · {formatSize(file.size)}</span><a href={file.url} download={file.name}>İndir</a></header>{error?<p>{error}</p>:<pre>{text||'Metin yükleniyor…'}</pre>}</section>
}
function FileView({file}:{file:UploadedFile}){
  if(file.type.startsWith('image/'))return <a className="message-image" href={file.url} target="_blank" rel="noreferrer"><img src={file.url} alt={file.name}/><span>{file.name} · {formatSize(file.size)}</span></a>
  if(file.type.startsWith('video/'))return <VideoPreview file={file}/>
  if(file.type==='application/pdf')return <section className="message-pdf"><iframe src={file.url} title={`${file.name} PDF önizlemesi`}/><footer><span>{file.name} · {formatSize(file.size)}</span><a href={file.url} target="_blank" rel="noreferrer">Yeni pencerede aç</a></footer></section>
  if(file.type.startsWith('text/')||/\.(txt|md|log|csv|json|xml|ya?ml)$/i.test(file.name))return <TextPreview file={file}/>
  return <a className="message-file" href={file.url} target="_blank" rel="noreferrer" download={file.name}><i>DOSYA</i><span><b>{file.name}</b><small>{formatSize(file.size)}</small></span><em>İndir</em></a>
}
export default function MessageRichText({content}:{content:string}){
  const onMention=useMentionOpener(),files:UploadedFile[]=[]
  const visible=content.split('\n').filter(line=>{const match=filePattern.exec(line.trim());if(!match)return true;files.push({name:decodeURIComponent(match[1]),type:decodeURIComponent(match[2]),size:Number(match[3]),url:match[4]});return false}).join('\n')
  const blocks=visible.split(/(```[\s\S]*?```)/g).filter(Boolean)
  const urls=visible.match(/https?:\/\/[^\s]+/g)||[],videos=[...new Set(urls.map(youtubeId).filter((id):id is string=>Boolean(id)))]
  return <>{visible&&<div className="rich-message">{blocks.map((block,index)=>block.startsWith('```')&&block.endsWith('```')?<pre key={index}><code>{block.slice(3,-3).replace(/^\n|\n$/g,'')}</code></pre>:<p key={index}>{block.split('\n').map((line,lineIndex)=><Fragment key={lineIndex}>{inline(line,onMention)}{lineIndex<block.split('\n').length-1&&<br/>}</Fragment>)}</p>)}</div>}{files.length>0&&<div className="message-attachments">{files.map((file,index)=><FileView key={file.url+index} file={file}/>)}</div>}{videos.map(id=><div className="youtube-embed" key={id}><iframe src={`https://www.youtube-nocookie.com/embed/${id}`} title="YouTube videosu" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" allowFullScreen/></div>)}</>
}
