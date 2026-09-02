import { createContext, ReactNode, useCallback, useContext, useEffect, useRef, useState } from 'react'
import { canUseMedia, mediaBlockReason, mediaDevices } from './media'
import { audioConstraints, getSettings, useSettings, videoConstraints } from './settings'
import { playCue } from './notifications'
import { canUseSystemAudio, startSystemAudio, type SystemAudio } from './systemAudio'
import { publishPointer, publishStroke, type PointerMessage, type StrokeMessage } from './drawBus'
import { bitrateFor, quality, type QualityId, type ShareInfo, type ShareOptions, type ViewerQuality } from './screenShare'
import { emitRealtime, useRealtimeEvents } from './realtime'

export type CallParticipant={userId:string;displayName:string;muted:boolean;deafened:boolean;video:boolean;screen:boolean}
export type CallDisplayParticipant=CallParticipant&{pending?:boolean}
export type CallState={id:string;kind:'direct'|'channel';organizationId:string;channelId:string|null;caller?:string|null;callee?:string|null;participants:CallParticipant[];invited:string[]}
export type IncomingCall={call:CallState;from:{userId:string;displayName:string};silent?:boolean}
type RemoteMedia={stream:MediaStream;hasVideo:boolean}

type CallApi={
  me:string
  call:CallState|null
  incoming:IncomingCall|null
  joining:boolean
  error:string
  muted:boolean
  cameraOn:boolean
  screenOn:boolean
  deafened:boolean
  startedAt:number|null
  connectedAt:number|null
  ringing:boolean
  localStream:MediaStream|null
  localVideo:MediaStream|null
  remote:Record<string,RemoteMedia>
  speaking:Record<string,boolean>
  links:Record<string,RTCPeerConnectionState>
  micReady:boolean
  watching:string[]
  audioSuspended:Set<string>
  suspendAudio:(userId:string,on:boolean)=>void
  sendStroke:(message:Omit<StrokeMessage,'from'>)=>void
  sendPointer:(message:Omit<PointerMessage,'from'>)=>void
  watch:(userId:string|null)=>void   // null: hepsini kapat
  levels:Record<string,number>
  locallyMuted:Set<string>
  userVolumes:Record<string,number>
  deafenedFor:Set<string>
  voiceRooms:Record<string,CallParticipant[]>
  startDirect:(userId:string)=>Promise<void>
  joinChannel:(channelId:string,ring?:boolean)=>Promise<void>
  accept:()=>Promise<void>
  decline:()=>void
  leave:()=>void
  invite:(userId:string)=>void
  toggleMute:()=>void
  toggleCamera:()=>Promise<void>
  toggleScreen:()=>Promise<void>
  startScreen:(options:ShareOptions)=>Promise<void>
  stopScreen:()=>Promise<void>
  share:ShareInfo|null
  updateShare:(next:{quality?:QualityId;fps?:number})=>Promise<void>
  viewerQuality:Record<string,ViewerQuality>
  requestQuality:(userId:string,level:ViewerQuality)=>void
  toggleDeafen:()=>void
  toggleLocalMute:(userId:string)=>void
  setUserVolume:(userId:string,volume:number)=>void
  togglePeerDeafen:(userId:string)=>void
  muteForEveryone:(userId:string)=>void
}

const CallContext=createContext<CallApi|null>(null)
export function useCall(){
  const value=useContext(CallContext)
  if(!value)throw new Error('useCall bir CallProvider içinde kullanılmalı')
  return value
}

type Peer={connection:RTCPeerConnection;audioSender:RTCRtpSender;videoSender:RTCRtpSender;pending:RTCIceCandidateInit[];makingOffer:boolean;polite:boolean;nudge:()=>void}

export function CallProvider({auth,organizationId,children}:{auth:{user:{id:string};accessToken:string};organizationId:string;children:ReactNode}){
  const[call,setCall]=useState<CallState|null>(null)
  const[incoming,setIncoming]=useState<IncomingCall|null>(null)
  const[joining,setJoining]=useState(false)
  const[error,setError]=useState('')
  const[muted,setMuted]=useState(false)
  const[cameraOn,setCameraOn]=useState(false)
  const[screenOn,setScreenOn]=useState(false)
  const[deafened,setDeafened]=useState(false)
  const[startedAt,setStartedAt]=useState<number|null>(null)
  const[connectedAt,setConnectedAt]=useState<number|null>(null)
  const[localStream,setLocalStream]=useState<MediaStream|null>(null)
  const[localVideo,setLocalVideo]=useState<MediaStream|null>(null)
  const[remote,setRemote]=useState<Record<string,RemoteMedia>>({})
  const[speaking,setSpeaking]=useState<Record<string,boolean>>({})
  const[links,setLinks]=useState<Record<string,RTCPeerConnectionState>>({})
  const[micReady,setMicReady]=useState(false)
  const[share,setShare]=useState<ShareInfo|null>(null)
  // Birden fazla yayın aynı anda izlenebilir; liste izlenen kişilerin kimliklerini tutar.
  const[watching,setWatching]=useState<string[]>([])
  const watchingRef=useRef<string[]>([])
  watchingRef.current=watching
  // İzlemeye başlayınca/bırakınca yayıncıya haber gider (o taraf ses duyar).
  const watch=useCallback((userId:string|null)=>{
    const callId=callRef.current?.id
    const current=watchingRef.current
    if(!userId){
      if(callId)for(const item of current)emitRealtime('call:signal',{callId,to:item,data:{watch:false}})
      setWatching([]);return
    }
    const on=!current.includes(userId)
    if(callId)emitRealtime('call:signal',{callId,to:userId,data:{watch:on}})
    setWatching(on?[...current,userId]:current.filter(item=>item!==userId))
  },[])
  // Geçmişe sarılan yayında ses kayıttan çalar; o sırada canlı ses susturulur.
  const[audioSuspended,setAudioSuspended]=useState(new Set<string>())
  const suspendAudio=useCallback((userId:string,on:boolean)=>setAudioSuspended(current=>{
    const next=new Set(current)
    if(on)next.add(userId);else next.delete(userId)
    return next
  }),[])
  const[viewerQuality,setViewerQuality]=useState<Record<string,ViewerQuality>>({})
  const[levels,setLevels]=useState<Record<string,number>>({})
  const[locallyMuted,setLocallyMuted]=useState<Set<string>>(new Set())
  const[userVolumes,setUserVolumes]=useState<Record<string,number>>({})
  const[deafenedFor,setDeafenedFor]=useState<Set<string>>(new Set())
  const[voiceRooms,setVoiceRooms]=useState<Record<string,CallParticipant[]>>({})

  const peers=useRef(new Map<string,Peer>())
  const building=useRef(new Map<string,Promise<Peer>>())   // aynı kişi için iki bağlantı kurulmasın
  const retryTimers=useRef(new Map<string,number>())
  const retryCounts=useRef(new Map<string,number>())
  const earlySignals=useRef<{callId:string;from:string;data:any}[]>([])
  const localRef=useRef<MediaStream|null>(null)
  const cameraTrack=useRef<MediaStreamTrack|null>(null)
  const screenTrack=useRef<MediaStreamTrack|null>(null)
  const callRef=useRef<CallState|null>(null)
  const analysers=useRef(new Map<string,{context:AudioContext;stop:()=>void}>())
  const shareRef=useRef<(ShareOptions&{height:number})|null>(null)   // yayının yakalama ayarları
  const viewerLevels=useRef(new Map<string,ViewerQuality>())          // izleyicilerin istediği kalite
  const shareMix=useRef<{context:AudioContext;track:MediaStreamTrack}|null>(null)
  const remoteStreams=useRef(new Map<string,MediaStream>())
  const shareAudioStream=useRef<MediaStream|null>(null)   // ayrı seçilen bilgisayar sesi kaynağı
  const systemAudio=useRef<SystemAudio|null>(null)          // yerel WASAPI loopback köprüsü   // kişi başına tek akış: ses + video birlikte
  const deafenedRef=useRef(new Set<string>())
  const settings=useSettings()
  const holding=useRef(false)

  callRef.current=call
  deafenedRef.current=deafenedFor
  const me=auth.user.id

  // ---- konuşma göstergesi ----
  const watchSpeaking=useCallback((userId:string,stream:MediaStream)=>{
    if(analysers.current.has(userId))return
    try{
      const context=new AudioContext()
      const analyser=context.createAnalyser()
      analyser.fftSize=512
      context.createMediaStreamSource(stream).connect(analyser)
      const data=new Uint8Array(analyser.frequencyBinCount)
      let frame=0,lastLevel=0,lastLevelAt=0
      const tick=()=>{
        analyser.getByteTimeDomainData(data)
        let peak=0
        for(const value of data)peak=Math.max(peak,Math.abs(value-128)/128)
        const level=Math.min(1,Math.max(0,(peak-.025)/.32))
        const now=performance.now()
        if(now-lastLevelAt>70&&Math.abs(lastLevel-level)>=.025){lastLevel=level;lastLevelAt=now;setLevels(current=>({...current,[userId]:level}))}
        setSpeaking(current=>{
          const active=peak>0.06
          if(current[userId]===active)return current
          return{...current,[userId]:active}
        })
        frame=requestAnimationFrame(tick)
      }
      tick()
      analysers.current.set(userId,{context,stop:()=>{cancelAnimationFrame(frame);void context.close().catch(()=>{})}})
    }catch{}
  },[])
  const stopSpeaking=useCallback((userId?:string)=>{
    if(userId){analysers.current.get(userId)?.stop();analysers.current.delete(userId);setSpeaking(current=>{const next={...current};delete next[userId];return next});setLevels(current=>{const next={...current};delete next[userId];return next});return}
    analysers.current.forEach(item=>item.stop())
    analysers.current.clear()
    setSpeaking({})
    setLevels({})
  },[])

  // ---- yerel akış ----
  const ensureLocalStream=useCallback(async()=>{
    if(localRef.current)return localRef.current
    const devices=mediaDevices()
    if(!devices?.getUserMedia)throw new Error(mediaBlockReason())
    const stream=await devices.getUserMedia({audio:audioConstraints()})
    localRef.current=stream
    setLocalStream(stream)
    setMicReady(stream.getAudioTracks().length>0)
    watchSpeaking(me,stream)
    return stream
  },[me,watchSpeaking])

  const teardown=useCallback(()=>{
    if(callRef.current)playCue('leave')   // odadan kendim çıktım
    peers.current.forEach(peer=>peer.connection.close())
    peers.current.clear()
    building.current.clear();earlySignals.current=[];remoteStreams.current.clear()
    retryTimers.current.forEach(timer=>window.clearTimeout(timer))
    retryTimers.current.clear();retryCounts.current.clear()
    cameraTrack.current?.stop();cameraTrack.current=null
    screenTrack.current?.stop();screenTrack.current=null
    shareRef.current=null;viewerLevels.current.clear()
    if(shareMix.current){shareMix.current.track.stop();void shareMix.current.context.close().catch(()=>{});shareMix.current=null}
    localRef.current?.getTracks().forEach(track=>track.stop())
    localRef.current=null
    stopSpeaking()
    setLocalStream(null);setLocalVideo(null);setRemote({});setLinks({});setMicReady(false);setWatching([]);setAudioSuspended(new Set());setCall(null);setStartedAt(null);setConnectedAt(null)
    setMuted(false);setCameraOn(false);setScreenOn(false);setDeafened(false);setShare(null);setViewerQuality({})
  },[stopSpeaking])

  // ---- eş bağlantıları ----
  // İzleyicinin istediği kalite, o kişiye giden video akışına uygulanır (her eş ayrı bağlantı).
  const applyEncoding=useCallback(async(peer:Peer,level:ViewerQuality)=>{
    const parameters=peer.videoSender.getParameters()
    if(!parameters.encodings||!parameters.encodings.length)parameters.encodings=[{}]
    const encoding=parameters.encodings[0]
    const current=shareRef.current
    if(!current||level==='auto'){delete encoding.maxBitrate;delete encoding.maxFramerate;encoding.scaleResolutionDownBy=1}
    else{
      const preset=quality(level)
      encoding.maxBitrate=bitrateFor(level,current.fps)
      encoding.maxFramerate=current.fps
      encoding.scaleResolutionDownBy=Math.max(1,(current.height||preset.height)/preset.height)
    }
    await peer.videoSender.setParameters(parameters).catch(()=>{})
  },[])

  const buildPeer=useCallback(async(callId:string,otherId:string,polite:boolean)=>{
    const stream=await ensureLocalStream()
    const options=getSettings()
    const iceServers:RTCIceServer[]=[]
    if(options.stunUrl)iceServers.push({urls:options.stunUrl})
    // TURN: iki taraf birbirine doğrudan ulaşamazsa (katı NAT, mobil veri) ses röle üzerinden akar.
    if(options.turnUrl)iceServers.push({urls:options.turnUrl,username:options.turnUsername||undefined,credential:options.turnPassword||undefined})
    const connection=new RTCPeerConnection({iceServers,iceTransportPolicy:options.forceRelay&&options.turnUrl?'relay':'all'})
    // Yayın/kamera zaten açıksa sonradan katılan da ilk andan itibaren görüntüyü alır.
    const audioSender=connection.addTrack(shareMix.current?.track||stream.getAudioTracks()[0],stream)
    const videoSender=connection.addTransceiver('video',{direction:'sendrecv'}).sender
    const activeVideo=screenTrack.current||cameraTrack.current
    if(activeVideo)await videoSender.replaceTrack(activeVideo).catch(()=>{})
    const peer:Peer={connection,audioSender,videoSender,pending:[],makingOffer:false,polite,nudge:()=>{}}
    peers.current.set(otherId,peer)

    connection.onicecandidate=event=>{
      if(event.candidate)emitRealtime('call:signal',{callId,to:otherId,data:{candidate:event.candidate.toJSON()}})
    }
    connection.ontrack=event=>{
      // Ses ve video ayrı olaylarla gelir; ikisini tek akışta toplarız, yoksa biri diğerini ezer.
      const stream=remoteStreams.current.get(otherId)||new MediaStream()
      remoteStreams.current.set(otherId,stream)
      if(!stream.getTracks().includes(event.track))stream.addTrack(event.track)
      // Karşı taraf kamerayı/yayını kapattığında parça "muted" olur; kutucuk buna göre gösterilir.
      const refresh=()=>setRemote(current=>({...current,[otherId]:{stream,hasVideo:stream.getVideoTracks().some(track=>track.readyState==='live'&&!track.muted)}}))
      refresh()
      if(event.track.kind==='audio')watchSpeaking(otherId,stream)
      event.track.onended=refresh
      event.track.onmute=refresh
      event.track.onunmute=refresh
      stream.onremovetrack=refresh
      stream.onaddtrack=refresh
    }
    // Sinyal mesajı yolda kaybolursa (proxy sıfırlaması) pazarlık sessizce asılı kalır; belirli aralıkla tazeleriz.
    const scheduleRetry=()=>{
      window.clearTimeout(retryTimers.current.get(otherId))
      retryTimers.current.set(otherId,window.setTimeout(()=>{
        if(connection.connectionState==='connected'||connection.connectionState==='closed')return
        // Yanıtlayan taraf tazelemez; yalnızca beklemeye devam eder.
        if(polite){scheduleRetry();return}
        const tries=(retryCounts.current.get(otherId)||0)+1
        retryCounts.current.set(otherId,tries)
        if(tries>4){setError('Ses bağlantısı kurulamadı. Ayarlar > Ses ve Görüntü bölümünden TURN sunucusu eklemeyi dene.');return}
        // Teklifi yalnızca bir taraf tazeler; diğer taraf yanıtlar.
        if(!polite&&connection.signalingState==='stable')void(async()=>{
          const offer=await connection.createOffer({iceRestart:true})
          await connection.setLocalDescription(offer)
          emitRealtime('call:signal',{callId,to:otherId,data:{description:connection.localDescription}})
        })().catch(()=>{})
        scheduleRetry()
      },8000))
    }
    scheduleRetry()

    connection.onconnectionstatechange=()=>{
      setLinks(current=>({...current,[otherId]:connection.connectionState}))
      if(connection.connectionState==='connected'){
        window.clearTimeout(retryTimers.current.get(otherId))
        retryTimers.current.delete(otherId);retryCounts.current.delete(otherId)
      }
      if(connection.connectionState==='failed'||connection.connectionState==='closed'){
        window.clearTimeout(retryTimers.current.get(otherId))
        retryTimers.current.delete(otherId)
        peers.current.delete(otherId)
        remoteStreams.current.delete(otherId)
        setRemote(current=>{const next={...current};delete next[otherId];return next})
        stopSpeaking(otherId)
        if(connection.connectionState==='failed')setError('Ses bağlantısı kurulamadı. Ayarlar > Ses ve Görüntü bölümünden TURN sunucusu eklemeyi dene.')
      }
    }
    // Teklifi kimlik sırasına göre tek taraf başlatır (glare önlemi).
    if(shareRef.current)void applyEncoding(peer,viewerLevels.current.get(otherId)||'auto')

    // Video göndericisi hâlâ gönderime geçmediyse (teklif çakışmada düşmüş olabilir) pazarlığı tazeler.
    // Chromium bu durumda negotiationneeded'ı tekrar tetiklemediği için kendimiz zorluyoruz.
    const nudge=(tries=0)=>{
      if(tries>8||connection.connectionState==='closed')return
      window.setTimeout(async()=>{
        if(!videoSender.track||connection.connectionState==='closed')return
        const sending=connection.getTransceivers().some(item=>item.sender===videoSender&&item.currentDirection?.includes('send'))
        if(sending)return
        if(connection.signalingState!=='stable'||peer.makingOffer){nudge(tries+1);return}
        try{
          peer.makingOffer=true
          await connection.setLocalDescription()
          emitRealtime('call:signal',{callId,to:otherId,data:{description:connection.localDescription}})
        }catch{}
        finally{peer.makingOffer=false}
        nudge(tries+1)
      },700)
    }
    peer.nudge=()=>nudge()
    if(activeVideo)peer.nudge()

    // Sonradan eklenen kamera/ekran parçalarının da yeni bir m-line ile pazarlığa girmesi için
    // "perfect negotiation": iki taraf da teklif verebilir, çakışmayı kibar taraf geri alır.
    connection.onnegotiationneeded=async()=>{
      try{
        peer.makingOffer=true
        await connection.setLocalDescription()
        emitRealtime('call:signal',{callId,to:otherId,data:{description:connection.localDescription}})
      }catch{}
      finally{peer.makingOffer=false}
    }
    return peer
  },[applyEncoding,ensureLocalStream,stopSpeaking,watchSpeaking])

  // Aynı kişi için call:state ve call:signal aynı anda bağlantı kurmaya çalışabilir; ikinci çağrı ilkini bekler.
  const createPeer=useCallback((callId:string,otherId:string,polite:boolean)=>{
    const existing=peers.current.get(otherId)
    if(existing)return Promise.resolve(existing)
    const inflight=building.current.get(otherId)
    if(inflight)return inflight
    const job=buildPeer(callId,otherId,polite)
    building.current.set(otherId,job)
    void job.catch(()=>{}).then(()=>{if(building.current.get(otherId)===job)building.current.delete(otherId)})
    return job
  },[buildPeer])

  const handleSignal=useCallback(async(payload:{callId:string;from:string;data:any})=>{
    let peer:Peer
    try{peer=await createPeer(payload.callId,payload.from,me<payload.from)}
    catch(problem:any){setError(problem?.message||'Mikrofon açılamadığı için ses bağlantısı kurulamadı.');return}
    const{connection}=peer
    if(payload.data?.cursor){
      const message={...payload.data.cursor,from:payload.from}
      publishPointer(message)
      // Yayını ben yapıyorsam çizim gerçek ekranımda da görünmeli.
      if(message.target===me&&screenTrack.current)window.p2?.overlay?.paint({kind:'cursor',...message})
      return
    }
    if(payload.data?.draw){
      const message={...payload.data.draw,from:payload.from}
      publishStroke(message)
      if(message.target===me&&screenTrack.current)window.p2?.overlay?.paint({kind:'stroke',...message})
      return
    }
    if(payload.data?.watch!==undefined){
      playCue(payload.data.watch?'viewerJoin':'viewerLeave')
      return
    }
    if(payload.data?.quality){
      // Karşı taraf yayınımın kalitesini değiştirmek istiyor.
      viewerLevels.current.set(payload.from,payload.data.quality)
      await applyEncoding(peer,payload.data.quality)
      return
    }
    if(payload.data?.description){
      const description=new RTCSessionDescription(payload.data.description)
      const collision=description.type==='offer'&&(peer.makingOffer||connection.signalingState!=='stable')
      // Kaba taraf çakışan teklifi yok sayar; kibar taraf kendi teklifini geri alıp karşınınkini uygular.
      if(!peer.polite&&collision)return
      try{
        await connection.setRemoteDescription(description)
        if(description.type==='offer'){
          await connection.setLocalDescription()
          emitRealtime('call:signal',{callId:payload.callId,to:payload.from,data:{description:connection.localDescription}})
        }
      }catch{return}
      for(const candidate of peer.pending.splice(0))await connection.addIceCandidate(candidate).catch(()=>{})
    }else if(payload.data?.candidate){
      if(connection.remoteDescription)await connection.addIceCandidate(payload.data.candidate).catch(()=>{})
      else peer.pending.push(payload.data.candidate)
    }
  },[applyEncoding,createPeer,me])


  const syncPeers=useCallback(async(next:CallState)=>{
    const others=next.participants.map(participant=>participant.userId).filter(userId=>userId!==me)
    // Bir eş kurulamazsa (mikrofon izni yoksa) diğerleri de kalmasın; hata görünür olsun.
    for(const otherId of others){
      try{await createPeer(next.id,otherId,me<otherId)}
      catch(problem:any){setError(problem?.message||'Mikrofon açılamadığı için ses bağlantısı kurulamadı.')}
    }
    for(const[userId,peer]of[...peers.current.entries()]){
      if(others.includes(userId))continue
      peer.connection.close()
      peers.current.delete(userId)
      remoteStreams.current.delete(userId)
      setRemote(current=>{const nextRemote={...current};delete nextRemote[userId];return nextRemote})
      stopSpeaking(userId)
    }
  },[createPeer,me,stopSpeaking])

  // Bas-konuş / bas-sustur: tuş basılıyken mikrofon açılır (veya kapanır).
  const applyMicrophone=useCallback(()=>{
    const stream=localRef.current
    if(!stream)return
    const mode=settings.voiceMode
    const enabled=mode==='open'?!muted:mode==='ptt'?holding.current&&!muted:!holding.current&&!muted
    stream.getAudioTracks().forEach(track=>{track.enabled=enabled})
    return enabled
  },[muted,settings.voiceMode])
  useEffect(()=>{applyMicrophone()},[applyMicrophone,localStream])
  useEffect(()=>{
    if(settings.voiceMode==='open')return
    const key=settings.voiceKey||'Space'
    const matches=(event:globalThis.KeyboardEvent)=>event.code===key||event.key===key
    const down=(event:globalThis.KeyboardEvent)=>{
      if(!matches(event)||holding.current)return
      const target=event.target as HTMLElement|null
      // Yazı yazarken boşluk tuşu metin girişini bozmasın.
      if(target&&(target.isContentEditable||['INPUT','TEXTAREA','SELECT'].includes(target.tagName)))return
      holding.current=true
      const enabled=applyMicrophone()
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,muted:!enabled})
    }
    const up=(event:globalThis.KeyboardEvent)=>{
      if(!matches(event)||!holding.current)return
      holding.current=false
      const enabled=applyMicrophone()
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,muted:!enabled})
    }
    window.addEventListener('keydown',down)
    window.addEventListener('keyup',up)
    return()=>{window.removeEventListener('keydown',down);window.removeEventListener('keyup',up);holding.current=false;applyMicrophone()}
  },[applyMicrophone,settings.voiceMode,settings.voiceKey])

  // ---- sunucu olayları ----
  useRealtimeEvents(auth.accessToken,organizationId,{
    'call:incoming':(payload:IncomingCall)=>{
      if(callRef.current)return   // zaten bir aramadayız
      setIncoming(payload)
    },
    'call:state':(payload:{call:CallState})=>{
      const mine=payload.call.participants.some(participant=>participant.userId===me)
      if(mine){
        // Odaya giren/çıkan ve yayın açan/kapatan için kısa işaretler.
        const before=callRef.current
        if(before?.id!==payload.call.id)playCue('join')   // odaya kendim girdim
        if(before?.id===payload.call.id){
          for(const participant of payload.call.participants){
            if(participant.userId===me)continue
            const previous=before.participants.find(item=>item.userId===participant.userId)
            if(!previous)playCue('join')
            else if(participant.screen&&!previous.screen)playCue('streamStart')
            else if(!participant.screen&&previous.screen)playCue('streamStop')
          }
          for(const participant of before.participants){
            if(participant.userId===me)continue
            if(!payload.call.participants.some(item=>item.userId===participant.userId))playCue('leave')
          }
        }
        setIncoming(current=>current&&current.call.id===payload.call.id?null:current)
        setCall(payload.call)
        setStartedAt(current=>current??Date.now())
        // Karşı taraf katılana kadar "çalıyor"; sesli odada katılır katılmaz bağlısın.
        if(payload.call.kind==='channel'||payload.call.participants.length>1)setConnectedAt(current=>current??Date.now())
        void syncPeers(payload.call).then(async()=>{
          // Erken gelip kuyruğa alınan sinyaller, eşler kurulduktan sonra sırayla işlenir.
          const queued=earlySignals.current.filter(signal=>signal.callId===payload.call.id)
          earlySignals.current=[]
          for(const signal of queued)await handleSignal(signal)
        })
      }else if(callRef.current?.id===payload.call.id){
        setCall(payload.call)
      }else if(incoming?.call.id===payload.call.id){
        setIncoming(current=>current?{...current,call:payload.call}:current)
      }
    },
    'call:signal':(payload:{callId:string;from:string;data:any})=>{
      // Sinyal, kendi call:state'imizden önce gelebilir; atmak yerine kuyruğa alıp sonra işleriz.
      if(callRef.current?.id!==payload.callId){earlySignals.current=[...earlySignals.current.slice(-40),payload];return}
      void handleSignal(payload)
    },
    'connect':()=>{if(callRef.current)emitRealtime('call:sync',{callId:callRef.current.id})},
    'call:ended':(payload:{callId:string})=>{
      setIncoming(current=>current?.call.id===payload.callId?null:current)
      if(callRef.current?.id===payload.callId)teardown()
    },
    'call:declined':(payload:{callId:string;userId:string})=>{
      if(callRef.current?.id===payload.callId)setError('Arama reddedildi.')
      window.setTimeout(()=>setError(''),2600)
    },
    'call:force-mute':(payload:{callId:string})=>{
      if(callRef.current?.id!==payload.callId)return
      localRef.current?.getAudioTracks().forEach(track=>{track.enabled=false})
      setMuted(true)
    },
    'voice:presence':(payload:{channelId:string;participants:CallParticipant[]})=>{
      setVoiceRooms(current=>({...current,[payload.channelId]:payload.participants}))
    },
  })

  useEffect(()=>()=>teardown(),[teardown])

  // ---- eylemler ----
  const startDirect=useCallback(async(userId:string)=>{
    setError('');setJoining(true)
    try{
      await ensureLocalStream()
      emitRealtime('call:start',{organizationId,kind:'direct',targetUserId:userId})
    }catch(problem:any){setError(canUseMedia()?'Mikrofon açılamadı. Ayarlardan cihazını kontrol et.':mediaBlockReason());void problem}
    finally{setJoining(false)}
  },[ensureLocalStream,organizationId])

  const joinChannel=useCallback(async(channelId:string,ring=false)=>{
    setError('');setJoining(true)
    try{
      await ensureLocalStream()
      emitRealtime('call:start',{organizationId,kind:'channel',channelId,ring})
    }catch(problem:any){setError(canUseMedia()?'Mikrofon açılamadı. Ayarlardan cihazını kontrol et.':mediaBlockReason());void problem}
    finally{setJoining(false)}
  },[ensureLocalStream,organizationId])

  const accept=useCallback(async()=>{
    if(!incoming)return
    setJoining(true)
    try{
      await ensureLocalStream()
      emitRealtime('call:accept',{callId:incoming.call.id})
      setIncoming(null)
    }catch{setError(canUseMedia()?'Mikrofon açılamadı.':mediaBlockReason())}
    finally{setJoining(false)}
  },[ensureLocalStream,incoming])

  const decline=useCallback(()=>{
    if(!incoming)return
    emitRealtime('call:decline',{callId:incoming.call.id})
    setIncoming(null)
  },[incoming])

  const leave=useCallback(()=>{
    if(callRef.current)emitRealtime('call:leave',{callId:callRef.current.id})
    teardown()
  },[teardown])

  const invite=useCallback((userId:string)=>{
    if(callRef.current)emitRealtime('call:invite',{callId:callRef.current.id,userId})
  },[])

  const toggleMute=useCallback(()=>{
    const stream=localRef.current
    if(!stream)return
    const next=!muted
    setMuted(next)
    stream.getAudioTracks().forEach(track=>{track.enabled=!next&&settings.voiceMode!=='ptt'})
    if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,muted:next})
  },[muted,settings.voiceMode])

  const replaceVideo=useCallback(async(track:MediaStreamTrack|null)=>{
    for(const peer of peers.current.values()){
      await peer.videoSender.replaceTrack(track).catch(()=>{})
      if(track)peer.nudge()
    }
  },[])

  // Bilgisayar sesi yayına eklendiğinde mikrofonla karıştırılıp tek ses parçası olarak gider.
  const outgoingAudio=useCallback(()=>shareMix.current?.track||localRef.current?.getAudioTracks()[0]||null,[])
  const clearShareMix=useCallback(async()=>{
    const mix=shareMix.current
    if(!mix)return
    shareMix.current=null
    mix.track.stop()
    void mix.context.close().catch(()=>{})
    const mic=localRef.current?.getAudioTracks()[0]||null
    for(const[userId,peer]of peers.current.entries())if(!deafenedRef.current.has(userId))await peer.audioSender.replaceTrack(mic).catch(()=>{})
  },[])
  const applyShareAudio=useCallback(async(stream:MediaStream)=>{
    const systemTrack=stream.getAudioTracks()[0]
    if(!systemTrack)return
    const context=new AudioContext()
    const destination=context.createMediaStreamDestination()
    const mic=localRef.current?.getAudioTracks()[0]
    if(mic)context.createMediaStreamSource(new MediaStream([mic])).connect(destination)
    context.createMediaStreamSource(new MediaStream([systemTrack])).connect(destination)
    const mixed=destination.stream.getAudioTracks()[0]
    shareMix.current={context,track:mixed}
    systemTrack.onended=()=>{void clearShareMix()}
    for(const[userId,peer]of peers.current.entries())if(!deafenedRef.current.has(userId))await peer.audioSender.replaceTrack(mixed).catch(()=>{})
  },[clearShareMix])


  const toggleCamera=useCallback(async()=>{
    if(!canUseMedia()){setError(mediaBlockReason());return}
    if(cameraOn){
      cameraTrack.current?.stop();cameraTrack.current=null
      if(!screenOn){await replaceVideo(null);setLocalVideo(null)}
      setCameraOn(false)
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,video:false})
      return
    }
    try{
      const stream=await mediaDevices()!.getUserMedia({video:videoConstraints()})
      const track=stream.getVideoTracks()[0]
      cameraTrack.current=track
      if(!screenOn){await replaceVideo(track);setLocalVideo(new MediaStream([track]))}
      setCameraOn(true)
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,video:true})
    }catch{setError('Kamera açılamadı.')}
  },[cameraOn,replaceVideo,screenOn])

  const stopScreen=useCallback(async()=>{
    screenTrack.current?.stop();screenTrack.current=null
    systemAudio.current?.stop();systemAudio.current=null
    shareAudioStream.current?.getTracks().forEach(track=>track.stop());shareAudioStream.current=null
    shareRef.current=null
    await clearShareMix()
    await replaceVideo(cameraOn?cameraTrack.current:null)
    for(const peer of peers.current.values())await applyEncoding(peer,'auto')
    setLocalVideo(cameraOn&&cameraTrack.current?new MediaStream([cameraTrack.current]):null)
    setScreenOn(false);setShare(null)
    window.p2?.overlay?.hide()
    playCue('streamStop')
    if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,screen:false})
  },[applyEncoding,cameraOn,clearShareMix,replaceVideo])

  const startScreen=useCallback(async(options:ShareOptions)=>{
    if(!canUseMedia()||!mediaDevices()?.getDisplayMedia){setError(mediaBlockReason());return}
    const preset=quality(options.quality)
    try{
      // Masaüstünde kaynağı biz seçiyoruz; tarayıcıda seçimi tarayıcının kendi penceresi yapar.
      const video={width:{ideal:preset.width},height:{ideal:preset.height},frameRate:{ideal:options.fps,max:options.fps}}
      const capture=async(withAudio:boolean)=>{
        if(options.sourceId)window.p2?.screen?.select({sourceId:options.sourceId,audio:withAudio})
        return mediaDevices()!.getDisplayMedia({video,audio:withAudio})
      }
      // Bilgisayar sesi: ayrı bir kaynak seçildiyse oradan, yoksa ekran sesinden (loopback) alınır.
      let silentReason=''
      let stream:MediaStream
      if(options.audio&&canUseSystemAudio()){
        // Masaüstünde hoparlör sesi yerel modülden alınır (Chromium'un loopback'i çoğu cihazda açılmıyor).
        stream=await capture(false)
        try{systemAudio.current=await startSystemAudio();shareAudioStream.current=systemAudio.current.stream}
        catch(problem:any){silentReason=(problem?.message||'Bilgisayar sesi alınamadı.')+' Yayın sessiz başladı.'}
      }else if(options.audio){
        try{stream=await capture(true)}
        catch(audioProblem:any){
          if(audioProblem?.name==='NotAllowedError')throw audioProblem
          silentReason='Bilgisayar sesi alınamadı, yayın sessiz başladı.'
          stream=await capture(false)
        }
      }else stream=await capture(false)
      const track=stream.getVideoTracks()[0]
      if(!track){setError('Ekran paylaşımı başlatılamadı.');return}
      track.contentHint=options.fps>=30?'motion':'detail'
      await track.applyConstraints({frameRate:{ideal:options.fps,max:options.fps}}).catch(()=>{})
      screenTrack.current=track
      shareRef.current={...options,height:track.getSettings().height||preset.height}
      track.onended=()=>{void stopScreen()}
      await replaceVideo(track)
      setLocalVideo(new MediaStream([track]))
      if(options.audio)await applyShareAudio(shareAudioStream.current||stream)
      for(const[userId,peer]of peers.current.entries())await applyEncoding(peer,viewerLevels.current.get(userId)||'auto')
      setScreenOn(true)
      // Çizim kaplaması yalnızca ekran paylaşımında açılır; pencere paylaşımında konum hizalanamaz.
      if(options.sourceId.startsWith('screen:'))window.p2?.overlay?.show(options.sourceId)
      setShare({quality:options.quality,fps:options.fps,audio:Boolean(shareAudioStream.current||stream.getAudioTracks().length)})
      setError(silentReason)
      playCue('streamStart')
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,screen:true})
    }catch(problem:any){
      setError(problem?.name==='NotAllowedError'?'Ekran paylaşımı iptal edildi.':'Ekran paylaşımı başlatılamadı.')
    }
  },[applyEncoding,applyShareAudio,replaceVideo,stopScreen])

  // Yayın sürerken çözünürlük ve kare hızı değiştirilebilir; yeniden paylaşmaya gerek yok.
  const updateShare=useCallback(async(next:{quality?:QualityId;fps?:number})=>{
    const current=shareRef.current,track=screenTrack.current
    if(!current||!track)return
    const merged={...current,...next}
    const preset=quality(merged.quality)
    try{await track.applyConstraints({width:{ideal:preset.width},height:{ideal:preset.height},frameRate:{ideal:merged.fps,max:merged.fps}})}catch{}
    track.contentHint=merged.fps>=30?'motion':'detail'
    shareRef.current={...merged,height:track.getSettings().height||preset.height}
    setShare({quality:merged.quality,fps:merged.fps,audio:Boolean(shareMix.current)})
    for(const[userId,peer]of peers.current.entries())await applyEncoding(peer,viewerLevels.current.get(userId)||'auto')
  },[applyEncoding])

  const toggleScreen=useCallback(async()=>{
    if(screenOn){await stopScreen();return}
    await startScreen({sourceId:'',quality:'1080p',fps:30,audio:false})
  },[screenOn,startScreen,stopScreen])

  // Kalem çizimleri: kendi çizgimi hemen gösterip aramadaki herkese yollarım.
  const sendStroke=useCallback((message:Omit<StrokeMessage,'from'>)=>{
    publishStroke({...message,from:me})
    if(message.target===me&&screenTrack.current)window.p2?.overlay?.paint({kind:'stroke',...message,from:me})
    const active=callRef.current
    if(!active)return
    for(const participant of active.participants){
      if(participant.userId===me)continue
      emitRealtime('call:signal',{callId:active.id,to:participant.userId,data:{draw:message}})
    }
  },[me])

  // Kalem imleci: kendi imlecimi kendime göndermem gerekmez, yalnızca diğerlerine gider.
  const sendPointer=useCallback((message:Omit<PointerMessage,'from'>)=>{
    const active=callRef.current
    if(!active)return
    for(const participant of active.participants){
      if(participant.userId===me)continue
      emitRealtime('call:signal',{callId:active.id,to:participant.userId,data:{cursor:message}})
    }
  },[me])

  // İzleyici, yayıncıdan kendisi için daha düşük ya da tam kalite ister.
  const requestQuality=useCallback((userId:string,level:ViewerQuality)=>{
    setViewerQuality(current=>({...current,[userId]:level}))
    if(callRef.current)emitRealtime('call:signal',{callId:callRef.current.id,to:userId,data:{quality:level}})
  },[])

  const toggleDeafen=useCallback(()=>{
    setDeafened(current=>{
      const next=!current
      const stream=localRef.current
      if(next&&stream){stream.getAudioTracks().forEach(track=>{track.enabled=false});setMuted(true)}
      // Kulaklık kapalıyken karşı tarafta kulaklık ikonu, yalnızca mikrofon kapalıyken mikrofon ikonu görünür.
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,deafened:next,...(next?{muted:true}:{})})
      return next
    })
  },[])

  const toggleLocalMute=useCallback((userId:string)=>setLocallyMuted(current=>{
    const next=new Set(current)
    if(next.has(userId))next.delete(userId);else next.add(userId)
    return next
  }),[])

  const setUserVolume=useCallback((userId:string,volume:number)=>setUserVolumes(current=>({...current,[userId]:Math.max(0,Math.min(200,volume))})),[])

  const togglePeerDeafen=useCallback((userId:string)=>{
    const peer=peers.current.get(userId)
    if(!peer)return
    setDeafenedFor(current=>{
      const next=new Set(current),blocked=!next.has(userId)
      if(blocked)next.add(userId);else next.delete(userId)
      void peer.audioSender.replaceTrack(blocked?null:localRef.current?.getAudioTracks()[0]||null)
      return next
    })
  },[])

  const muteForEveryone=useCallback((userId:string)=>{
    if(callRef.current)emitRealtime('call:moderate',{callId:callRef.current.id,userId,muted:true})
  },[])

  const api:CallApi={me,call,incoming,joining,error,muted,cameraOn,screenOn,deafened,startedAt,localStream,localVideo,remote,speaking,links,micReady,watching,watch,sendStroke,sendPointer,audioSuspended,suspendAudio,levels,locallyMuted,userVolumes,deafenedFor,voiceRooms,connectedAt,ringing:Boolean(call&&!connectedAt),
    startDirect,joinChannel,accept,decline,leave,invite,toggleMute,toggleCamera,toggleScreen,startScreen,stopScreen,updateShare,share,viewerQuality,requestQuality,toggleDeafen,toggleLocalMute,setUserVolume,togglePeerDeafen,muteForEveryone}
  return <CallContext.Provider value={api}>{children}</CallContext.Provider>
}
