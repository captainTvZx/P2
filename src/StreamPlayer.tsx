import { useCallback, useEffect, useRef, useState } from 'react'
import { CallIcon } from './CallIcons'
import { useCall } from './CallProvider'
import QualityMenu from './QualityMenu'
import { FRAME_RATES, QUALITIES, type QualityId, type ViewerQuality } from './screenShare'
import { penColor, PEN_PALETTE, subscribePointers, subscribeStrokes, type StrokePoint } from './drawBus'
import { updateSettings, useSettings } from './settings'

// Canlı yayın izleyici: yayın alınırken arka planda kaydedilir, böylece izleyen kişi
// duraklatabilir, geriye sarabilir ve "CANLI" ile tekrar akışa dönebilir.
const BUFFER_SECONDS=180
const STROKE_HOLD=2200      // çizgi bu kadar tam görünür kalır
const STROKE_FADE=2600      // sonra bu sürede yavaşça silinir
const CURSOR_LIFE=2600      // imleç hareketsiz kalırsa bu sürede kaybolur

function clock(value:number){
  const total=Math.max(0,Math.round(value))
  return `${Math.floor(total/60)}:${String(total%60).padStart(2,'0')}`
}

export default function StreamPlayer({userId,stream,label,self,quality,onQuality,onClose}:{userId:string;stream:MediaStream;label:string;self?:boolean;quality?:ViewerQuality;onQuality?:(level:ViewerQuality)=>void;onClose?:()=>void}){
  const call=useCall()
  const settings=useSettings()
  const suspend=call.suspendAudio
  const setUserVolume=call.setUserVolume
  const video=useRef<HTMLVideoElement>(null)
  const chunks=useRef<Blob[]>([])
  const replayUrl=useRef('')
  const[buffered,setBuffered]=useState(0)
  const[mode,setMode]=useState<'live'|'replay'>('live')
  const[playing,setPlaying]=useState(true)
  const[position,setPosition]=useState(0)
  const[duration,setDuration]=useState(0)
  const[recordable,setRecordable]=useState(false)
  const[fullscreen,setFullscreen]=useState(false)
  const[expanded,setExpanded]=useState(false)   // tam ekran API'si engellenirse pencere içinde büyütme
  const[preview,setPreview]=useState(true)      // kendi yayınının önizlemesi
  const[pen,setPen]=useState(false)
  const[nearBar,setNearBar]=useState(false)   // kalem açıkken çubuk yalnızca alt kenarda görünür
  const[drawingNow,setDrawingNow]=useState(false)
  const canvas=useRef<HTMLCanvasElement>(null)
  const strokes=useRef(new Map<string,{color:string;width:number;points:StrokePoint[];touched:number}>())
  const drawing=useRef<{id:string;buffer:StrokePoint[];sent:number}|null>(null)
  const cursors=useRef(new Map<string,{x:number;y:number;name:string;color:string;at:number}>())
  const pointerSent=useRef(0)
  const myName=call.call?.participants.find(item=>item.userId===call.me)?.displayName||'Sen'
  const shell=useRef<HTMLDivElement>(null)
  const volume=call.userVolumes[userId]??100

  // Gelen yayını sürekli kaydet: en fazla son 3 dakika saklanır.
  useEffect(()=>{
    if(self||typeof MediaRecorder==='undefined')return
    let recorder:MediaRecorder
    try{recorder=new MediaRecorder(stream,{mimeType:'video/webm;codecs=vp8,opus'})}
    catch{try{recorder=new MediaRecorder(stream)}catch{setRecordable(false);return}}
    chunks.current=[];setBuffered(0);setRecordable(true)
    recorder.ondataavailable=event=>{
      if(!event.data.size)return
      chunks.current.push(event.data)
      // İlk parça webm başlığını taşır, o hep kalmalı; eskiler baştan sonra atılır.
      if(chunks.current.length>BUFFER_SECONDS+1)chunks.current.splice(1,1)
      setBuffered(chunks.current.length-1)
    }
    try{recorder.start(1000)}catch{setRecordable(false)}
    return()=>{try{if(recorder.state!=='inactive')recorder.stop()}catch{}}
  },[stream,self])

  // Canlı moda dönüş: kayıttan çalma bırakılır, ses yeniden canlı akıştan gelir.
  useEffect(()=>{
    const node=video.current
    if(!node||mode!=='live')return
    if(replayUrl.current){URL.revokeObjectURL(replayUrl.current);replayUrl.current=''}
    node.removeAttribute('src')
    node.srcObject=stream
    node.muted=true
    void node.play().catch(()=>{})
    setPlaying(true);setPosition(0)
    suspend(userId,false)
  },[mode,stream,suspend,userId])

  useEffect(()=>()=>{
    if(replayUrl.current)URL.revokeObjectURL(replayUrl.current)
    suspend(userId,false)
  },[suspend,userId])

  const enterReplay=useCallback(async(target:number)=>{
    const node=video.current
    if(self||!node||!chunks.current.length)return
    const blob=new Blob(chunks.current,{type:'video/webm'})
    if(replayUrl.current)URL.revokeObjectURL(replayUrl.current)
    replayUrl.current=URL.createObjectURL(blob)
    node.srcObject=null
    node.src=replayUrl.current
    node.muted=false
    node.volume=Math.min(1,volume/100)
    // Geçmiş görüntüyü oynatırken ayrı RemoteAudio canlı konuşmayı taşımaya
    // devam eder. Ekran kaydına sarılması kişinin o anki sesini kesmemeli.
    suspend(userId,false)
    setMode('replay')
    setPosition(target)
    const known=Math.max(1,chunks.current.length-1)   // kaydedilen saniye sayısı
    await new Promise<void>(resolve=>{
      if(node.readyState>0){resolve();return}
      node.addEventListener('loadedmetadata',()=>resolve(),{once:true})
      window.setTimeout(resolve,800)
    })
    let total=node.duration
    if(!Number.isFinite(total)||total<=0){
      // Parça parça kaydedilen webm'de süre yazmaz; sonuna sarıp oynatıcıya hesaplatırız.
      await new Promise<void>(resolve=>{
        let done=false
        const finish=()=>{if(done)return;done=true;node.removeEventListener('seeked',finish);node.removeEventListener('durationchange',finish);resolve()}
        node.addEventListener('seeked',finish)
        node.addEventListener('durationchange',finish)
        node.currentTime=1e6
        window.setTimeout(finish,900)
      })
      total=Number.isFinite(node.duration)&&node.duration>0?node.duration:known
    }
    setDuration(total)
    node.currentTime=Math.max(0,Math.min(total-0.15,target))
    void node.play().catch(()=>{});setPlaying(true)
  },[self,suspend,userId,volume])

  // Gelen çizgiler (kendi çizgim de aynı yoldan gelir).
  useEffect(()=>subscribeStrokes(userId,message=>{
    const existing=strokes.current.get(message.id)
    if(existing){existing.points.push(...message.points);existing.touched=performance.now()}
    else strokes.current.set(message.id,{color:message.color,width:message.width||4,points:[...message.points],touched:performance.now()})
  }),[userId])

  // Karşı tarafın kalem imleci: adıyla birlikte görünür, hareketsiz kalınca kaybolur.
  useEffect(()=>subscribePointers(userId,message=>{
    if(message.gone){cursors.current.delete(message.from);return}
    cursors.current.set(message.from,{x:message.x,y:message.y,name:message.name,color:message.color,at:performance.now()})
  }),[userId])

  // Görüntünün gerçekte kapladığı alan (object-fit:contain kenar boşluklarıyla birlikte).
  const contentBox=useCallback((width:number,height:number)=>{
    const node=video.current
    const source={w:node?.videoWidth||0,h:node?.videoHeight||0}
    if(!source.w||!source.h)return{x:0,y:0,width,height}
    const scale=Math.min(width/source.w,height/source.h)
    const shown={w:source.w*scale,h:source.h*scale}
    return{x:(width-shown.w)/2,y:(height-shown.h)/2,width:shown.w,height:shown.h}
  },[])

  useEffect(()=>{
    let frame=0
    const paint=()=>{
      frame=requestAnimationFrame(paint)
      const node=canvas.current
      if(!node)return
      const rect=node.getBoundingClientRect()
      if(!rect.width||!rect.height)return
      const ratio=window.devicePixelRatio||1
      if(node.width!==Math.round(rect.width*ratio)||node.height!==Math.round(rect.height*ratio)){
        node.width=Math.round(rect.width*ratio);node.height=Math.round(rect.height*ratio)
      }
      const context=node.getContext('2d')
      if(!context)return
      context.setTransform(ratio,0,0,ratio,0,0)
      context.clearRect(0,0,rect.width,rect.height)
      const box=contentBox(rect.width,rect.height)
      const now=performance.now()
      context.lineCap='round';context.lineJoin='round'
      context.shadowColor='rgba(0,0,0,.55)';context.shadowBlur=4
      for(const[id,stroke]of[...strokes.current.entries()]){
        const age=now-stroke.touched
        if(age>STROKE_HOLD+STROKE_FADE){strokes.current.delete(id);continue}
        context.globalAlpha=age<STROKE_HOLD?1:Math.max(0,1-(age-STROKE_HOLD)/STROKE_FADE)
        context.strokeStyle=stroke.color
        context.lineWidth=stroke.width
        context.beginPath()
        stroke.points.forEach((point,index)=>{
          const x=box.x+point.x*box.width,y=box.y+point.y*box.height
          if(index===0)context.moveTo(x,y);else context.lineTo(x,y)
        })
        context.stroke()
      }
      context.shadowBlur=0
      for(const[id,cursor]of[...cursors.current.entries()]){
        const age=now-cursor.at
        if(age>CURSOR_LIFE){cursors.current.delete(id);continue}
        context.globalAlpha=age>CURSOR_LIFE-600?Math.max(0,(CURSOR_LIFE-age)/600):1
        const x=box.x+cursor.x*box.width,y=box.y+cursor.y*box.height
        // kalem ucu: küçük bir üçgen + halka
        context.fillStyle=cursor.color
        context.beginPath()
        context.moveTo(x,y);context.lineTo(x+11,y+4);context.lineTo(x+4.5,y+6);context.lineTo(x+3,y+12)
        context.closePath();context.fill()
        context.font='600 11px Inter, "Segoe UI", sans-serif'
        const width=context.measureText(cursor.name).width
        const left=Math.min(x+12,box.x+box.width-width-14),top=y+10
        context.fillStyle=cursor.color
        context.beginPath()
        if(context.roundRect)context.roundRect(left,top,width+12,17,8)
        else context.rect(left,top,width+12,17)
        context.fill()
        context.fillStyle='#14181d'
        context.fillText(cursor.name,left+6,top+12)
      }
      context.globalAlpha=1
    }
    frame=requestAnimationFrame(paint)
    return()=>cancelAnimationFrame(frame)
  },[contentBox])

  const ink=settings.penInk||penColor(call.me)
  const width=settings.penWidth||4
  const flushStroke=useCallback(()=>{
    const active=drawing.current
    if(!active||!active.buffer.length)return
    const points=active.buffer.splice(0)
    active.sent=performance.now()
    call.sendStroke({target:userId,id:active.id,color:ink,width,points})
  },[call,ink,width,userId])
  const pointFrom=useCallback((event:React.PointerEvent<HTMLCanvasElement>)=>{
    const rect=event.currentTarget.getBoundingClientRect()
    const box=contentBox(rect.width,rect.height)
    return{x:(event.clientX-rect.left-box.x)/box.width,y:(event.clientY-rect.top-box.y)/box.height}
  },[contentBox])

  const span=mode==='replay'?(duration||buffered):buffered
  const value=mode==='live'?span:position

  async function togglePlay(){
    const node=video.current
    if(!node)return
    if(mode==='live'){await enterReplay(Math.max(0,buffered-0.4));node.pause();setPlaying(false);return}
    if(node.paused){void node.play().catch(()=>{});setPlaying(true)}
    else{node.pause();setPlaying(false)}
  }
  async function seek(next:number){
    if(mode==='live'){await enterReplay(next);return}
    const node=video.current
    if(!node)return
    node.currentTime=next;setPosition(next)
  }
  useEffect(()=>{
    const sync=()=>setFullscreen(Boolean(document.fullscreenElement)&&document.fullscreenElement===shell.current)
    document.addEventListener('fullscreenchange',sync)
    return()=>document.removeEventListener('fullscreenchange',sync)
  },[])
  useEffect(()=>{
    if(!expanded)return
    const escape=(event:KeyboardEvent)=>{if(event.key!=='Escape')return;event.preventDefault();window.p2?.windowControls.setFullScreen?.(false);setExpanded(false)}
    window.addEventListener('keydown',escape)
    return()=>window.removeEventListener('keydown',escape)
  },[expanded])
  useEffect(()=>()=>{if(expanded)window.p2?.windowControls.setFullScreen?.(false)},[expanded])
  async function toggleFullscreen(){
    const next=!(fullscreen||expanded)
    // Masaüstünde pencereyi gerçekten tam ekrana alıyoruz (F11 gibi); tarayıcıda Fullscreen API.
    if(window.p2?.windowControls.setFullScreen){
      window.p2.windowControls.setFullScreen(next)
      setExpanded(next)
      return
    }
    if(!next){
      if(document.fullscreenElement)await document.exitFullscreen().catch(()=>{})
      setFullscreen(false);setExpanded(false);return
    }
    try{await shell.current!.requestFullscreen();setFullscreen(true)}
    catch{setExpanded(true)}
  }
  function changeVolume(next:number){
    setUserVolume(userId,next)
    const node=video.current
    if(node&&mode==='replay')node.volume=Math.min(1,next/100)
  }

  const big=fullscreen||expanded
  // Kalemle çizerken çubuk ekranın o kısmını kapatmasın: yalnızca fare alta gelince görünür.
  const barHidden=pen&&(drawingNow||!nearBar)
  return <div className={`stream-player${big?' fullscreen':''}${barHidden?' bar-hidden':''}`} ref={shell}
    onPointerMove={event=>{
      if(!pen)return
      const rect=event.currentTarget.getBoundingClientRect()
      // Sınırda gidip gelmeyi önlemek için iki eşik: 92px'de açılır, 140px'in üstünde kapanır.
      setNearBar(current=>event.clientY>rect.bottom-92?true:event.clientY<rect.bottom-140?false:current)
    }}
    onPointerLeave={()=>{
      setNearBar(false)
      if(pen)call.sendPointer({target:userId,name:myName,color:ink,x:0,y:0,gone:true})
    }}>
    {self&&!preview&&<div className="stream-self-card"><CallIcon name="screen"/><b>Ekranını paylaşıyorsun</b><small>Önizleme kapalı</small></div>}
    <video ref={video} autoPlay playsInline muted={mode==='live'} hidden={self&&!preview}
      onTimeUpdate={event=>{if(mode==='replay')setPosition(event.currentTarget.currentTime)}}
      onEnded={()=>setMode('live')}/>
    <canvas ref={canvas} className={pen?'stream-canvas drawing':'stream-canvas'}
      onPointerDown={event=>{
        if(!pen)return
        event.currentTarget.setPointerCapture(event.pointerId)
        drawing.current={id:`${call.me.slice(0,6)}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2,6)}`,buffer:[pointFrom(event)],sent:0}
        setDrawingNow(true)
        flushStroke()
      }}
      onPointerMove={event=>{
        if(!pen)return
        const point=pointFrom(event)
        if(performance.now()-pointerSent.current>55){
          pointerSent.current=performance.now()
          call.sendPointer({target:userId,name:myName,color:ink,x:point.x,y:point.y})
        }
        if(!drawing.current)return
        drawing.current.buffer.push(point)
        if(performance.now()-drawing.current.sent>60)flushStroke()
      }}
      onPointerUp={()=>{setDrawingNow(false);if(drawing.current){flushStroke();drawing.current=null}}}
      onPointerCancel={()=>{setDrawingNow(false);if(drawing.current){flushStroke();drawing.current=null}}}/>
    <b className="stream-label">{label}{mode==='replay'&&<i> · kayıttan</i>}</b>
    <div className="stream-bar">
      {!self&&<>
        <button type="button" className="stream-icon" onClick={()=>void togglePlay()} title={mode==='replay'&&!playing?'Devam et':'Duraklat'} aria-label="Duraklat/Devam et">
          <CallIcon name={mode==='replay'&&!playing?'play':'pause'}/>
        </button>
        <input className="stream-timeline" type="range" min={0} max={Math.max(span,1)} step={0.2} value={value} disabled={!recordable||buffered<2}
          onChange={event=>void seek(Number(event.target.value))} title="Yayında geriye sar"/>
        {mode==='live'
          ? <span className="stream-time live"><i/>CANLI</span>
          : <>
              <span className="stream-time">-{clock(Math.max(0,span-position))}</span>
              <button type="button" className="stream-live" onClick={()=>setMode('live')} title="Canlı yayına dön">CANLIYA DÖN</button>
            </>}
        <div className="stream-volume" title="Yayın sesi">
          <CallIcon name={volume?'volume':'volumeOff'}/>
          <input type="range" min={0} max={100} value={volume} onChange={event=>changeVolume(Number(event.target.value))}/>
        </div>
        {onQuality&&<QualityMenu value={quality||'auto'} onChange={onQuality}/>}
      </>}
      {self&&call.share&&<>
        <label className="stream-setting">Çözünürlük<select value={call.share.quality} onChange={event=>void call.updateShare({quality:event.target.value as QualityId})}>
          {QUALITIES.map(item=><option key={item.id} value={item.id}>{item.label}</option>)}
        </select></label>
        <label className="stream-setting">FPS<select value={call.share.fps} onChange={event=>void call.updateShare({fps:Number(event.target.value)})}>
          {FRAME_RATES.map(rate=><option key={rate} value={rate}>{rate}</option>)}
        </select></label>
        <button type="button" className="stream-stop" onClick={()=>void call.stopScreen()} title="Yayını durdur">Yayını durdur</button>
      </>}
      {pen&&<div className="pen-tools">
        <div className="pen-colors">
          {[penColor(call.me),...PEN_PALETTE.filter(item=>item!==penColor(call.me))].map(color=>
            <button type="button" key={color} className={color===ink?'active':undefined} style={{background:color}} title={color===penColor(call.me)?'Kendi rengin':'Renk'} onClick={()=>updateSettings({penInk:color})}/>)}
        </div>
        <input className="pen-width" type="range" min={2} max={14} step={1} value={width} title={`Kalınlık: ${width}px`} onChange={event=>updateSettings({penWidth:Number(event.target.value)})}/>
        <i className="pen-preview" style={{background:ink,width:width,height:width}}/>
      </div>}
      <button type="button" className={pen?'stream-icon active':'stream-icon'} onClick={()=>{
        if(pen)call.sendPointer({target:userId,name:myName,color:ink,x:0,y:0,gone:true})
        setPen(current=>!current)
      }} title={pen?'Kalemi bırak':'Ekrana çiz'} aria-label="Kalem" style={pen?{color:ink}:undefined}>
        <CallIcon name="pen"/>
      </button>
      {self
        ? <button type="button" className="stream-icon" onClick={()=>setPreview(current=>!current)} title={preview?'Önizlemeyi gizle':'Önizlemeyi göster'} aria-label="Önizleme">
            <CallIcon name={preview?'cameraOff':'camera'}/>
          </button>
        : <button type="button" className="stream-icon" onClick={()=>void toggleFullscreen()} title={big?'Tam ekrandan çık':'Tam ekran'} aria-label="Tam ekran">
            <CallIcon name={big?'shrink':'expand'}/>
          </button>}
      {onClose&&<button type="button" className="feature-close" onClick={onClose} title="Yayını kapat">×</button>}
    </div>
  </div>
}
