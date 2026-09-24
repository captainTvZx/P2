import { Fragment, ReactNode, useEffect, useRef, useState } from 'react'
import { useMentionOpener } from './mentions'
import { formatSize, type UploadedFile } from './attachments'
import AttachmentViewer, { downloadAttachment, PdfThumbnail } from './AttachmentViewer'

const tokenPattern=/(\*\*[^*\n]+\*\*|\*[^*\n]+\*|\[[^\]]+\]\(https?:\/\/[^\s)]+\)|https?:\/\/[^\s]+|@[a-zA-Z0-9_.-]+)/g
const filePattern=/^\[\[p2-file\|([^|]+)\|([^|]+)\|(\d+)\|(https?:\/\/[^\]]+)\]\]$/
// Gömülü oynatıcı masaüstünde file:// kökeninden çalışmıyor (sonsuz yükleniyor);
// bu yüzden kart tıklanınca video kendi penceresinde gerçek YouTube sayfasında açılır.
function YoutubeCard({id}:{id:string}){
  // Sohbette oynar. Gömülü oynatıcı yalnızca gerçek bir http(s) kökeninde
  // çalışır; masaüstünde arayüz bu yüzden 127.0.0.1 üzerinden sunuluyor.
  // Eski file:// kurulumlarında kart açılıp video kendi penceresinde oynar.
  // Masaüstünde arayüz file:// kökeninde çalışıyor; YouTube oynatıcısı orada
  // açılmıyor. Çerçeve bu yüzden uygulamanın kendi yerel adresinden (127.0.0.1)
  // yükleniyor: oynatıcı normal bir web sayfasında çalıştığını görüyor.
  const shell=useRef<HTMLDivElement>(null)
  const[embedBase,setEmbedBase]=useState<string|null>(null)
  const[playing,setPlaying]=useState(false)
  const[broken,setBroken]=useState(false)
  useEffect(()=>{
    if(!window.p2?.media?.embedBase){setEmbedBase('');return}
    void window.p2.media.embedBase().then(base=>setEmbedBase(base||''))
  },[])
  const direct=location.protocol==='http:'||location.protocol==='https:'
  // Tam ekran: oynatıcının kendi düğmesi iç içe çerçevede her zaman
  // çalışmıyor, bu yüzden kendi düğmemiz kabı tam ekrana alıyor (çift tık da).
  const fullscreen=()=>{const node=shell.current;if(!node)return;if(document.fullscreenElement)void document.exitFullscreen();else void node.requestFullscreen?.().catch(()=>{})}
  if(playing)return <div className="youtube-embed" ref={shell} onDoubleClick={fullscreen}>
    <iframe src={direct?`https://www.youtube-nocookie.com/embed/${id}?autoplay=1&rel=0`:`${embedBase}/embed?v=${id}`} title="YouTube videosu" allow="accelerometer; autoplay; clipboard-write; encrypted-media; fullscreen; gyroscope; picture-in-picture; web-share" allowFullScreen/>
    <button type="button" className="youtube-full" title="Tam ekran (çift tıkla)" onClick={fullscreen}><svg viewBox="0 0 24 24"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/></svg></button>
  </div>
  const open=()=>{
    if(direct||embedBase){setPlaying(true);return}
    // Yerel sunucu açılamadıysa video kendi penceresinde oynar.
    if(window.p2?.media)void window.p2.media.openVideo(id)
    else window.open(`https://www.youtube.com/watch?v=${id}`,'_blank','noreferrer')
  }
  return <button type="button" className="youtube-card" onClick={open} title="Videoyu oynat">
    {!broken&&<img src={`https://i.ytimg.com/vi/${id}/hqdefault.jpg`} alt="" loading="lazy" onError={()=>setBroken(true)}/>}
    <span className="youtube-play" aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M8 5.5v13l11-6.5z"/></svg></span>
  </button>
}

function youtubeId(url:string){try{const parsed=new URL(url);if(parsed.hostname==='youtu.be')return parsed.pathname.slice(1).split('/')[0];if(parsed.hostname.endsWith('youtube.com'))return parsed.searchParams.get('v')||(/^\/shorts\/([^/]+)/.exec(parsed.pathname)?.[1]??null)}catch{}return null}
function inline(text:string,onMention:((username:string)=>void)|null):ReactNode[]{return text.split(tokenPattern).filter(Boolean).map((part,index)=>{if(part.startsWith('**')&&part.endsWith('**'))return <strong key={index}>{part.slice(2,-2)}</strong>;if(part.startsWith('*')&&part.endsWith('*'))return <em key={index}>{part.slice(1,-1)}</em>;const markdown=/^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)$/.exec(part);if(markdown)return <a key={index} href={markdown[2]} target="_blank" rel="noreferrer">{markdown[1]}</a>;if(/^https?:\/\//.test(part))return <a key={index} href={part} target="_blank" rel="noreferrer">{part}</a>;if(part.startsWith('@'))return onMention?<button type="button" className="mention" key={index} onClick={()=>onMention(part.slice(1))}>{part}</button>:<mark className="mention" key={index}>{part}</mark>;return <Fragment key={index}>{part}</Fragment>})}
function clock(seconds:number){if(!Number.isFinite(seconds))return '0:00';const minutes=Math.floor(seconds/60);return `${minutes}:${String(Math.floor(seconds%60)).padStart(2,'0')}`}

function VideoPreview({file}:{file:UploadedFile}){
  const video=useRef<HTMLVideoElement>(null),[playing,setPlaying]=useState(false),[current,setCurrent]=useState(0),[duration,setDuration]=useState(0),[volume,setVolume]=useState(1),[fullscreen,setFullscreen]=useState(false)
  useEffect(()=>{if(!fullscreen)return;const close=(event:KeyboardEvent)=>{if(event.key==='Escape')setFullscreen(false)};window.addEventListener('keydown',close);return()=>window.removeEventListener('keydown',close)},[fullscreen])
  function toggle(){const node=video.current;if(!node)return;if(node.paused)void node.play();else node.pause()}
  return <div className={`message-video p2-video${fullscreen?' p2-video-fullscreen':''}`}><video ref={video} src={file.url} preload="metadata" playsInline onClick={toggle} onPlay={()=>setPlaying(true)} onPause={()=>setPlaying(false)} onTimeUpdate={event=>setCurrent(event.currentTarget.currentTime)} onLoadedMetadata={event=>setDuration(event.currentTarget.duration)}/><div className="p2-video-controls"><button type="button" onClick={toggle}>{playing?'Ⅱ':'▶'}</button><span>{clock(current)} / {clock(duration)}</span><input className="p2-video-progress" type="range" min="0" max={duration||0} step="0.05" value={current} onChange={event=>{const value=Number(event.target.value);if(video.current)video.current.currentTime=value;setCurrent(value)}}/><button type="button" onClick={()=>{const next=volume?0:1;setVolume(next);if(video.current)video.current.volume=next}}>{volume?'🔊':'🔇'}</button><input className="p2-video-volume" type="range" min="0" max="1" step="0.05" value={volume} onChange={event=>{const value=Number(event.target.value);setVolume(value);if(video.current)video.current.volume=value}}/><button type="button" onClick={()=>setFullscreen(value=>!value)}>{fullscreen?'↙':'⛶'}</button></div><footer><span>{file.name} · {formatSize(file.size)}</span><button onClick={()=>void downloadAttachment(file)}>İndir</button></footer></div>
}
function TextPreview({file,onOpen}:{file:UploadedFile;onOpen:()=>void}){const[text,setText]=useState(''),[error,setError]=useState('');useEffect(()=>{let active=true;fetch(file.url).then(response=>response.text()).then(value=>active&&setText(value.slice(0,100000))).catch(()=>active&&setError('Metin önizlemesi yüklenemedi.'));return()=>{active=false}},[file.url]);return <section className="message-text-preview" role="button" tabIndex={0} onClick={onOpen} onKeyDown={event=>{if(event.key==='Enter')onOpen()}}><header><span>{file.name} · {formatSize(file.size)}</span><button onClick={event=>{event.stopPropagation();void downloadAttachment(file)}}>İndir</button></header>{error?<p>{error}</p>:<pre>{text||'Metin yükleniyor…'}</pre>}</section>}
function FileView({file}:{file:UploadedFile}){const[open,setOpen]=useState(false),viewer=open?<AttachmentViewer file={file} onClose={()=>setOpen(false)}/>:null;if(file.type.startsWith('image/'))return <><div className="message-image" role="button" tabIndex={0} onClick={()=>setOpen(true)}><img src={file.url} alt={file.name}/><footer><span>{file.name} · {formatSize(file.size)}</span><button title="İndir" aria-label="İndir" onClick={event=>{event.stopPropagation();void downloadAttachment(file)}}>İndir</button></footer></div>{viewer}</>;if(file.type.startsWith('video/'))return <VideoPreview file={file}/>;if(file.type==='application/pdf')return <><div className="message-pdf-preview" role="button" tabIndex={0} onClick={()=>setOpen(true)}><PdfThumbnail file={file}/><footer><span><b>{file.name}</b><small>{formatSize(file.size)}</small></span><button title="İndir" onClick={event=>{event.stopPropagation();void downloadAttachment(file)}}>İndir</button></footer></div>{viewer}</>;if(file.type.startsWith('text/')||/\.(txt|md|log|csv|json|xml|ya?ml)$/i.test(file.name))return <><TextPreview file={file} onOpen={()=>setOpen(true)}/>{viewer}</>;return <button type="button" className="message-file" onClick={()=>void downloadAttachment(file)}><i>DOSYA</i><span><b>{file.name}</b><small>{formatSize(file.size)}</small></span><em>İndir</em></button>}
function withoutCodeFences(value:string){let clean=value.trim(),previous='';while(clean!==previous){previous=clean;clean=clean.replace(/^(?:`+|'{3,})(?:[a-zA-Z0-9_+-]+)?\s*(?:\r?\n|$)/,'').replace(/(?:^|\r?\n)\s*(?:`+|'{3,})\s*$/,'').trim()}return clean}
function CodeBlock({code}:{code:string}){const clean=withoutCodeFences(code),[copied,setCopied]=useState(false);async function copy(){await navigator.clipboard.writeText(clean);setCopied(true);setTimeout(()=>setCopied(false),1500)}return <div className="message-code"><button type="button" onClick={copy}>{copied?'Kopyalandı':'Kopyala'}</button><pre><code>{clean}</code></pre></div>}

function RemoteImagePreview({url}:{url:string}){
  const[failed,setFailed]=useState(false),[loaded,setLoaded]=useState(false)
  if(failed)return null
  return <a className={`remote-image-preview${loaded?' loaded':''}`} href={url} target="_blank" rel="noreferrer"><img src={url} alt="Bağlantı görseli" loading="lazy" onLoad={()=>setLoaded(true)} onError={()=>setFailed(true)}/></a>
}

export default function MessageRichText({content}:{content:string}){
  const onMention=useMentionOpener(),files:UploadedFile[]=[]
  const visible=content.split('\n').filter(line=>{const match=filePattern.exec(line.trim());if(!match)return true;files.push({name:decodeURIComponent(match[1]),type:decodeURIComponent(match[2]),size:Number(match[3]),url:match[4]});return false}).join('\n')
  const blocks=visible.split(/(```[\s\S]*?```)/g).filter(Boolean),urls=[...new Set(visible.match(/https?:\/\/[^\s]+/g)||[])]
  const videos=[...new Set(urls.map(youtubeId).filter((id):id is string=>Boolean(id)))]
  const imageCandidates=urls.filter(url=>!youtubeId(url))
  return <>
    {visible&&<div className="rich-message">{blocks.map((block,index)=>block.startsWith('```')&&block.endsWith('```')?<CodeBlock key={index} code={block.slice(3,-3).replace(/^\n|\n$/g,'')}/>:<p key={index}>{block.split('\n').map((line,lineIndex)=><Fragment key={lineIndex}>{inline(line,onMention)}{lineIndex<block.split('\n').length-1&&<br/>}</Fragment>)}</p>)}</div>}
    {files.length>0&&<div className="message-attachments">{files.map((file,index)=><FileView key={file.url+index} file={file}/>)}</div>}
    {imageCandidates.map(url=><RemoteImagePreview key={url} url={url}/>)}
    {videos.map(id=><YoutubeCard key={id} id={id}/>)}
  </>
}
