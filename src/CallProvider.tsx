import { createContext, ReactNode, useCallback, useContext, useEffect, useRef, useState } from 'react'
import { audioConstraints, getSettings, useSettings, videoConstraints } from './settings'
import { emitRealtime, useRealtimeEvents } from './realtime'

export type CallParticipant={userId:string;displayName:string;muted:boolean;video:boolean;screen:boolean}
export type CallDisplayParticipant=CallParticipant&{pending?:boolean}
export type CallState={id:string;kind:'direct'|'channel';organizationId:string;channelId:string|null;caller?:string|null;callee?:string|null;participants:CallParticipant[];invited:string[]}
export type IncomingCall={call:CallState;from:{userId:string;displayName:string};silent?:boolean}
type RemoteMedia={stream:MediaStream;hasVideo:boolean}

type CallApi={
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
  toggleDeafen:()=>void
}

const CallContext=createContext<CallApi|null>(null)
export function useCall(){
  const value=useContext(CallContext)
  if(!value)throw new Error('useCall bir CallProvider içinde kullanılmalı')
  return value
}

type Peer={connection:RTCPeerConnection;audioSender:RTCRtpSender;videoSender:RTCRtpSender;pending:RTCIceCandidateInit[]}

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
  const[voiceRooms,setVoiceRooms]=useState<Record<string,CallParticipant[]>>({})

  const peers=useRef(new Map<string,Peer>())
  const localRef=useRef<MediaStream|null>(null)
  const cameraTrack=useRef<MediaStreamTrack|null>(null)
  const screenTrack=useRef<MediaStreamTrack|null>(null)
  const callRef=useRef<CallState|null>(null)
  const analysers=useRef(new Map<string,{context:AudioContext;stop:()=>void}>())
  const settings=useSettings()
  const holding=useRef(false)

  callRef.current=call
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
      let frame=0
      const tick=()=>{
        analyser.getByteTimeDomainData(data)
        let peak=0
        for(const value of data)peak=Math.max(peak,Math.abs(value-128)/128)
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
    if(userId){analysers.current.get(userId)?.stop();analysers.current.delete(userId);setSpeaking(current=>{const next={...current};delete next[userId];return next});return}
    analysers.current.forEach(item=>item.stop())
    analysers.current.clear()
    setSpeaking({})
  },[])

  // ---- yerel akış ----
  const ensureLocalStream=useCallback(async()=>{
    if(localRef.current)return localRef.current
    const stream=await navigator.mediaDevices.getUserMedia({audio:audioConstraints()})
    localRef.current=stream
    setLocalStream(stream)
    watchSpeaking(me,stream)
    return stream
  },[me,watchSpeaking])

  const teardown=useCallback(()=>{
    peers.current.forEach(peer=>peer.connection.close())
    peers.current.clear()
    cameraTrack.current?.stop();cameraTrack.current=null
    screenTrack.current?.stop();screenTrack.current=null
    localRef.current?.getTracks().forEach(track=>track.stop())
    localRef.current=null
    stopSpeaking()
    setLocalStream(null);setLocalVideo(null);setRemote({});setCall(null);setStartedAt(null);setConnectedAt(null)
    setMuted(false);setCameraOn(false);setScreenOn(false);setDeafened(false)
  },[stopSpeaking])

  // ---- eş bağlantıları ----
  const createPeer=useCallback(async(callId:string,otherId:string,polite:boolean)=>{
    const existing=peers.current.get(otherId)
    if(existing)return existing
    const stream=await ensureLocalStream()
    const connection=new RTCPeerConnection({iceServers:getSettings().stunUrl?[{urls:getSettings().stunUrl}]:[]})
    const audioSender=connection.addTrack(stream.getAudioTracks()[0],stream)
    const videoSender=connection.addTransceiver('video',{direction:'sendrecv'}).sender
    const peer:Peer={connection,audioSender,videoSender,pending:[]}
    peers.current.set(otherId,peer)

    connection.onicecandidate=event=>{
      if(event.candidate)emitRealtime('call:signal',{callId,to:otherId,data:{candidate:event.candidate.toJSON()}})
    }
    connection.ontrack=event=>{
      const[stream]=event.streams
      if(!stream)return
      setRemote(current=>({...current,[otherId]:{stream,hasVideo:stream.getVideoTracks().some(track=>track.readyState==='live')}}))
      if(event.track.kind==='audio')watchSpeaking(otherId,stream)
      event.track.onended=()=>setRemote(current=>({...current,[otherId]:{stream,hasVideo:stream.getVideoTracks().some(track=>track.readyState==='live')}}))
      stream.onremovetrack=()=>setRemote(current=>({...current,[otherId]:{stream,hasVideo:stream.getVideoTracks().some(track=>track.readyState==='live')}}))
    }
    connection.onconnectionstatechange=()=>{
      if(connection.connectionState==='failed'||connection.connectionState==='closed'){
        peers.current.delete(otherId)
        setRemote(current=>{const next={...current};delete next[otherId];return next})
        stopSpeaking(otherId)
      }
    }
    // Teklifi kimlik sırasına göre tek taraf başlatır (glare önlemi).
    if(!polite){
      const offer=await connection.createOffer()
      await connection.setLocalDescription(offer)
      emitRealtime('call:signal',{callId,to:otherId,data:{description:connection.localDescription}})
    }
    return peer
  },[ensureLocalStream,stopSpeaking,watchSpeaking])

  const syncPeers=useCallback(async(next:CallState)=>{
    const others=next.participants.map(participant=>participant.userId).filter(userId=>userId!==me)
    for(const otherId of others)await createPeer(next.id,otherId,me<otherId)
    for(const[userId,peer]of[...peers.current.entries()]){
      if(others.includes(userId))continue
      peer.connection.close()
      peers.current.delete(userId)
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
        setIncoming(current=>current&&current.call.id===payload.call.id?null:current)
        setCall(payload.call)
        setStartedAt(current=>current??Date.now())
        // Karşı taraf katılana kadar "çalıyor"; sesli odada katılır katılmaz bağlısın.
        if(payload.call.kind==='channel'||payload.call.participants.length>1)setConnectedAt(current=>current??Date.now())
        void syncPeers(payload.call)
      }else if(callRef.current?.id===payload.call.id){
        setCall(payload.call)
      }else if(incoming?.call.id===payload.call.id){
        setIncoming(current=>current?{...current,call:payload.call}:current)
      }
    },
    'call:signal':async(payload:{callId:string;from:string;data:any})=>{
      if(callRef.current?.id!==payload.callId)return
      const peer=await createPeer(payload.callId,payload.from,me<payload.from)
      const{connection}=peer
      if(payload.data?.description){
        const description=new RTCSessionDescription(payload.data.description)
        if(description.type==='offer'){
          await connection.setRemoteDescription(description)
          const answer=await connection.createAnswer()
          await connection.setLocalDescription(answer)
          emitRealtime('call:signal',{callId:payload.callId,to:payload.from,data:{description:connection.localDescription}})
        }else if(description.type==='answer'&&connection.signalingState!=='stable'){
          await connection.setRemoteDescription(description)
        }
        for(const candidate of peer.pending.splice(0))await connection.addIceCandidate(candidate).catch(()=>{})
      }else if(payload.data?.candidate){
        if(connection.remoteDescription)await connection.addIceCandidate(payload.data.candidate).catch(()=>{})
        else peer.pending.push(payload.data.candidate)
      }
    },
    'call:ended':(payload:{callId:string})=>{
      setIncoming(current=>current?.call.id===payload.callId?null:current)
      if(callRef.current?.id===payload.callId)teardown()
    },
    'call:declined':(payload:{callId:string;userId:string})=>{
      if(callRef.current?.id===payload.callId)setError('Arama reddedildi.')
      window.setTimeout(()=>setError(''),2600)
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
    }catch{setError('Mikrofon açılamadı. Ayarlardan cihazını kontrol et.')}
    finally{setJoining(false)}
  },[ensureLocalStream,organizationId])

  const joinChannel=useCallback(async(channelId:string,ring=false)=>{
    setError('');setJoining(true)
    try{
      await ensureLocalStream()
      emitRealtime('call:start',{organizationId,kind:'channel',channelId,ring})
    }catch{setError('Mikrofon açılamadı. Ayarlardan cihazını kontrol et.')}
    finally{setJoining(false)}
  },[ensureLocalStream,organizationId])

  const accept=useCallback(async()=>{
    if(!incoming)return
    setJoining(true)
    try{
      await ensureLocalStream()
      emitRealtime('call:accept',{callId:incoming.call.id})
      setIncoming(null)
    }catch{setError('Mikrofon açılamadı.')}
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
    for(const peer of peers.current.values())await peer.videoSender.replaceTrack(track).catch(()=>{})
  },[])

  const toggleCamera=useCallback(async()=>{
    if(cameraOn){
      cameraTrack.current?.stop();cameraTrack.current=null
      if(!screenOn){await replaceVideo(null);setLocalVideo(null)}
      setCameraOn(false)
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,video:false})
      return
    }
    try{
      const stream=await navigator.mediaDevices.getUserMedia({video:videoConstraints()})
      const track=stream.getVideoTracks()[0]
      cameraTrack.current=track
      if(!screenOn){await replaceVideo(track);setLocalVideo(new MediaStream([track]))}
      setCameraOn(true)
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,video:true})
    }catch{setError('Kamera açılamadı.')}
  },[cameraOn,replaceVideo,screenOn])

  const toggleScreen=useCallback(async()=>{
    if(screenOn){
      screenTrack.current?.stop();screenTrack.current=null
      await replaceVideo(cameraOn?cameraTrack.current:null)
      setLocalVideo(cameraOn&&cameraTrack.current?new MediaStream([cameraTrack.current]):null)
      setScreenOn(false)
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,screen:false})
      return
    }
    try{
      const stream=await navigator.mediaDevices.getDisplayMedia({video:true,audio:false})
      const track=stream.getVideoTracks()[0]
      screenTrack.current=track
      track.onended=()=>{screenTrack.current=null;setScreenOn(false);setLocalVideo(cameraOn&&cameraTrack.current?new MediaStream([cameraTrack.current]):null);void replaceVideo(cameraOn?cameraTrack.current:null);if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,screen:false})}
      await replaceVideo(track)
      setLocalVideo(new MediaStream([track]))
      setScreenOn(true)
      if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,screen:true})
    }catch{setError('Ekran paylaşımı başlatılamadı.')}
  },[cameraOn,replaceVideo,screenOn])

  const toggleDeafen=useCallback(()=>{
    setDeafened(current=>{
      const next=!current
      const stream=localRef.current
      if(next&&stream){stream.getAudioTracks().forEach(track=>{track.enabled=false});setMuted(true);if(callRef.current)emitRealtime('call:media',{callId:callRef.current.id,muted:true})}
      return next
    })
  },[])

  const api:CallApi={call,incoming,joining,error,muted,cameraOn,screenOn,deafened,startedAt,localStream,localVideo,remote,speaking,voiceRooms,connectedAt,ringing:Boolean(call&&!connectedAt),
    startDirect,joinChannel,accept,decline,leave,invite,toggleMute,toggleCamera,toggleScreen,toggleDeafen}
  return <CallContext.Provider value={api}>{children}</CallContext.Provider>
}
