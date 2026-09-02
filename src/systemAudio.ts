// Masaüstü sürümünde hoparlör sesi yerel WASAPI modülünden gelir; buradaki köprü
// gelen PCM parçalarını WebRTC'ye verilebilen bir MediaStreamTrack'e dönüştürür.
const WORKLET=`
class LoopbackProcessor extends AudioWorkletProcessor {
  constructor(){
    super()
    this.queue=[]
    this.offset=0
    this.port.onmessage=event=>{
      if(event.data==='clear'){this.queue=[];this.offset=0;return}
      this.queue.push(event.data)
      // Gecikme birikmesin: 400 ms'ten fazlası atılır.
      let frames=0
      for(const item of this.queue)frames+=item.length/2
      while(frames>sampleRate*0.4&&this.queue.length>1){frames-=this.queue[0].length/2;this.queue.shift();this.offset=0}
    }
  }
  process(inputs,outputs){
    const left=outputs[0][0], right=outputs[0][1]||outputs[0][0]
    for(let index=0;index<left.length;index++){
      let sampleLeft=0, sampleRight=0
      while(this.queue.length&&this.offset>=this.queue[0].length){this.queue.shift();this.offset=0}
      if(this.queue.length){
        const chunk=this.queue[0]
        sampleLeft=chunk[this.offset]
        sampleRight=chunk[this.offset+1]
        this.offset+=2
      }
      left[index]=sampleLeft
      right[index]=sampleRight
    }
    return true
  }
}
registerProcessor('loopback-processor',LoopbackProcessor)
`

export type SystemAudio={stream:MediaStream;stop:()=>void}

export function canUseSystemAudio(){return Boolean(window.p2?.systemAudio)}

// Geliştirmede otomatik testten çağırabilmek için köprüyü pencereye asıyoruz.
if(import.meta.env.DEV)(window as any).__p2systemAudio=()=>startSystemAudio()

// Yerel modülü açar ve sesi bir MediaStream olarak döndürür. Hata durumunda mesajla reddeder.
export async function startSystemAudio():Promise<SystemAudio>{
  const bridge=window.p2?.systemAudio
  if(!bridge)throw new Error('Sistem sesi yalnızca masaüstü sürümünde alınabilir.')
  const result=await bridge.start()
  if(!result?.ok)throw new Error(result?.message||'Sistem sesi başlatılamadı.')

  let context:AudioContext|null=null
  let node:AudioWorkletNode|null=null
  let destination:MediaStreamAudioDestinationNode|null=null
  let unsubscribe=()=>{}
  const ready=new Promise<MediaStream>((resolve,reject)=>{
    let settled=false
    const timer=window.setTimeout(()=>{if(!settled){settled=true;reject(new Error('Sistem sesinden veri gelmedi.'))}},4000)
    unsubscribe=bridge.onChunk(async chunk=>{
      try{
        if(!context){
          // Bağlam, cihazın örnekleme hızıyla açılır; böylece yeniden örnekleme gerekmez.
          context=new AudioContext({sampleRate:chunk.sampleRate})
          await context.audioWorklet.addModule(URL.createObjectURL(new Blob([WORKLET],{type:'text/javascript'})))
          node=new AudioWorkletNode(context,'loopback-processor',{numberOfInputs:0,outputChannelCount:[2]})
          destination=context.createMediaStreamDestination()
          node.connect(destination)
        }
        node?.port.postMessage(chunk.samples)
        if(!settled&&destination){settled=true;window.clearTimeout(timer);resolve(destination.stream)}
      }catch(problem:any){
        if(!settled){settled=true;window.clearTimeout(timer);reject(new Error(problem?.message||'Sistem sesi hazırlanamadı.'))}
      }
    })
  })

  try{
    const stream=await ready
    return{
      stream,
      stop:()=>{
        unsubscribe()
        bridge.stop()
        node?.disconnect()
        destination?.disconnect()
        void context?.close().catch(()=>{})
      },
    }
  }catch(problem){
    unsubscribe()
    bridge.stop()
    throw problem
  }
}
