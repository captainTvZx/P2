// Parite aracı — sahte medya (P2_PARITY_FAKE_MEDIA=1): arama ekranlarının Electron referansı GERÇEK ses/görüntü
// aygıtı açılmadan çizilsin diye sayfa betiklerinden ÖNCE ana dünyaya yerleştirilir (session.registerPreloadScript).
//
//  - navigator.mediaDevices.getUserMedia / getDisplayMedia / enumerateDevices: aygıtsız sentetik akışlar
//    (görüntü: tuval captureStream; ses: alıcı yönlü RTCPeerConnection izi — hiçbir aygıta bağlı değil).
//  - AudioContext / webkitAudioContext: tamamen JS sahte sınıf (çıkış aygıtı açılmaz; zil/bildirim sesi sessiz).
//  - HTMLMediaElement: srcObject'ten ses izleri çıkarılır, her öğe sessiz; <audio> hiç çalmaz; setSinkId etkisiz.
//  - window.__p2FakeMedia: çağrı sayaçları (kuru denetim için).
const { contextBridge } = require('electron')

function install() {
  if (window.__p2FakeMedia) return
  const stats = { getUserMedia: 0, getDisplayMedia: 0, enumerateDevices: 0, audioContext: 0, blockedAudio: 0, installedAt: performance.now() }
  Object.defineProperty(window, '__p2FakeMedia', { value: stats })
  const silentAudioTrack = () => {
    const pc = new RTCPeerConnection()
    return pc.addTransceiver('audio', { direction: 'recvonly' }).receiver.track
  }
  const canvasTrack = (label, width = 1280, height = 720) => {
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    const g = canvas.getContext('2d')
    let frame = 0
    const draw = () => {
      g.fillStyle = '#10223a'
      g.fillRect(0, 0, width, height)
      g.fillStyle = '#2f6fd6'
      g.fillRect((frame * 8) % width, height / 2 - 40, 80, 80)
      g.fillStyle = '#ffffff'
      g.font = '600 40px sans-serif'
      g.fillText(label, 40, 70)
      frame++
    }
    draw()
    setInterval(draw, 100)
    return canvas.captureStream(10).getVideoTracks()[0]
  }
  const devices = navigator.mediaDevices
  const define = (name, value) => Object.defineProperty(devices, name, { value, configurable: true, writable: true })
  define('getUserMedia', async (constraints = {}) => {
    stats.getUserMedia++
    const tracks = []
    if (constraints.audio) tracks.push(silentAudioTrack())
    if (constraints.video) tracks.push(canvasTrack('Sahte kamera', 640, 360))
    return new MediaStream(tracks)
  })
  define('getDisplayMedia', async (constraints = {}) => {
    stats.getDisplayMedia++
    const tracks = [canvasTrack('Sahte ekran')]
    if (constraints.audio) tracks.push(silentAudioTrack())
    return new MediaStream(tracks)
  })
  define('enumerateDevices', async () => {
    stats.enumerateDevices++
    return [
      { deviceId: 'default', kind: 'audioinput', label: 'Sahte mikrofon', groupId: 'sahte' },
      { deviceId: 'default', kind: 'audiooutput', label: 'Sahte hoparlör', groupId: 'sahte' },
      { deviceId: 'sahte-kamera', kind: 'videoinput', label: 'Sahte kamera', groupId: 'sahte' },
    ].map(d => ({ ...d, toJSON() { return d } }))
  })

  // ---- WebAudio: aygıtsız sahte bağlam
  class Param {
    constructor(v = 0) { this.value = v; this.defaultValue = v }
    setValueAtTime() { return this } linearRampToValueAtTime() { return this } exponentialRampToValueAtTime() { return this }
    setTargetAtTime() { return this } cancelScheduledValues() { return this } cancelAndHoldAtTime() { return this } setValueCurveAtTime() { return this }
  }
  class Node {
    constructor(context) { this.context = context; this.numberOfInputs = 1; this.numberOfOutputs = 1; this.channelCount = 2 }
    connect(target) { return target } disconnect() {} addEventListener() {} removeEventListener() {}
  }
  class FakeAudioContext {
    constructor(options = {}) {
      stats.audioContext++
      this.state = 'running'
      this.sampleRate = options.sampleRate || 48000
      this.baseLatency = 0.01
      this.outputLatency = 0.02
      this._t0 = performance.now()
      this.destination = new Node(this)
      this.destination.maxChannelCount = 2
      this.listener = {}
      this.onstatechange = null
    }
    get currentTime() { return (performance.now() - this._t0) / 1000 }
    createGain() { const n = new Node(this); n.gain = new Param(1); return n }
    createOscillator() { const n = new Node(this); n.frequency = new Param(440); n.detune = new Param(0); n.type = 'sine'; n.start = () => {}; n.stop = () => {}; n.onended = null; return n }
    createAnalyser() {
      const n = new Node(this)
      n.fftSize = 2048; n.frequencyBinCount = 1024; n.minDecibels = -100; n.maxDecibels = -30; n.smoothingTimeConstant = 0.8
      n.getByteFrequencyData = a => a.fill(0); n.getFloatFrequencyData = a => a.fill(-100)
      n.getByteTimeDomainData = a => a.fill(128); n.getFloatTimeDomainData = a => a.fill(0)
      return n
    }
    createMediaStreamSource(stream) { const n = new Node(this); n.mediaStream = stream; return n }
    createMediaStreamTrackSource(track) { const n = new Node(this); n.track = track; return n }
    createMediaStreamDestination() { const n = new Node(this); n.stream = new MediaStream([silentAudioTrack()]); return n }
    createMediaElementSource(el) { const n = new Node(this); n.mediaElement = el; return n }
    createBiquadFilter() { const n = new Node(this); n.frequency = new Param(350); n.Q = new Param(1); n.gain = new Param(0); n.detune = new Param(0); n.type = 'lowpass'; return n }
    createDynamicsCompressor() { const n = new Node(this); for (const k of ['threshold', 'knee', 'ratio', 'attack', 'release']) n[k] = new Param(0); n.reduction = 0; return n }
    createDelay() { const n = new Node(this); n.delayTime = new Param(0); return n }
    createStereoPanner() { const n = new Node(this); n.pan = new Param(0); return n }
    createChannelMerger() { return new Node(this) }
    createChannelSplitter() { return new Node(this) }
    createScriptProcessor() { const n = new Node(this); n.onaudioprocess = null; return n }
    createBuffer(channels, length, rate) { return { numberOfChannels: channels, length, sampleRate: rate, duration: length / rate, getChannelData: () => new Float32Array(length), copyToChannel() {}, copyFromChannel() {} } }
    createBufferSource() { const n = new Node(this); n.buffer = null; n.loop = false; n.playbackRate = new Param(1); n.start = () => {}; n.stop = () => {}; return n }
    decodeAudioData() { return Promise.resolve(this.createBuffer(1, 1, this.sampleRate)) }
    get audioWorklet() { return { addModule: () => Promise.resolve() } }
    resume() { this.state = 'running'; return Promise.resolve() }
    suspend() { this.state = 'suspended'; return Promise.resolve() }
    close() { this.state = 'closed'; return Promise.resolve() }
    setSinkId() { return Promise.resolve() }
    addEventListener() {} removeEventListener() {}
  }
  Object.defineProperty(window, 'AudioContext', { value: FakeAudioContext, configurable: true, writable: true })
  Object.defineProperty(window, 'webkitAudioContext', { value: FakeAudioContext, configurable: true, writable: true })
  if (typeof window.AudioWorkletNode === 'function') Object.defineProperty(window, 'AudioWorkletNode', { value: class extends Node {}, configurable: true, writable: true })

  // ---- Medya öğeleri: ses izi hiçbir zaman çalınmaz
  const proto = HTMLMediaElement.prototype
  const srcObject = Object.getOwnPropertyDescriptor(proto, 'srcObject')
  Object.defineProperty(proto, 'srcObject', {
    configurable: true,
    get() { return srcObject.get.call(this) },
    set(stream) {
      if (stream && typeof stream.getVideoTracks === 'function') {
        if (stream.getAudioTracks().length) stats.blockedAudio++
        const video = this instanceof HTMLVideoElement ? stream.getVideoTracks() : []
        stream = video.length ? new MediaStream(video) : null
      }
      srcObject.set.call(this, stream)
      try { this.muted = true; this.volume = 0 } catch {}
    },
  })
  const play = proto.play
  proto.play = function () {
    if (!(this instanceof HTMLVideoElement)) { stats.blockedAudio++; return Promise.resolve() }
    this.muted = true
    return play.call(this)
  }
  proto.setSinkId = function () { return Promise.resolve() }
  window.Audio = function () { stats.blockedAudio++; return document.createElement('span') }
}

try {
  if (process.contextIsolated && contextBridge.executeInMainWorld) contextBridge.executeInMainWorld({ func: install })
  else install()
} catch (error) {
  console.error('[parite] sahte medya yerleştirilemedi: ' + (error && error.message))
}
