// Masaüstünde adres preload'dan (ya da Ayarlar > Uygulama > Sunucu adresi) gelir.
// Tarayıcıda ise API, sayfayla aynı adres altından (Vite proxy'si ile /api) sunulur;
// böylece uzaktan erişimde tek bir HTTPS adresi yeterli olur.
const override=(()=>{
  try{
    const fromQuery=new URLSearchParams(window.location.search).get('api')
    if(fromQuery){window.localStorage.setItem('p2:apiUrl',fromQuery);return fromQuery}
    return window.localStorage.getItem('p2:apiUrl')||''
  }catch{return ''}
})()

const configured=(override||window.p2?.apiUrl||'').replace(/\/+$/,'')

export const API_BASE=configured||`${window.location.origin}/api`
// socket.io kök adrese bağlanır (yol /socket.io, ad alanı /stream). Adres bir proxy
// arkasındaysa (".../api") socket için o ek kaldırılır.
export const SOCKET_BASE=configured?configured.replace(/\/api$/,''):window.location.origin

// Oyun içi overlay penceresi (?overlay=1): aynı renderer, ama kendi oturumu yok.
// Erişim jetonu ana pencereden "kiralanır"; yenileme jetonu burada hiç bulunmaz.
// (settings.ts de aynı bayrağı dışa verir; döngüsel import olmasın diye burada yeniden türetilir.)
export const OVERLAY_MODE=(()=>{try{return new URLSearchParams(window.location.search).get('overlay')==='1'}catch{return false}})()
const LEASE_TIMEOUT_MS=10000

type SessionAuth={user:Record<string,unknown>;accessToken:string;refreshToken:string}
let activeAccessToken=''
let activeRefreshToken=''
let persistRefreshToken=true
let refreshing:Promise<SessionAuth>|null=null
// Overlay modunda bekleyen kiralama istekleri: ilk gelen `auth` push'u hepsini çözer.
let leaseWaiters:Array<(auth:SessionAuth)=>void>=[]

// API çağrılarının tamamı aynı yenileme kuyruğunu kullanır. Böylece 15 dakikalık
// erişim jetonu dolduğunda aynı anda gelen istekler refresh jetonunu sırayla
// tüketmez; tek yenileme yapılır ve bütün istekler yeni jetonla tekrarlanır.
export function setClientAuth(auth:{accessToken:string;refreshToken?:string}|null){
  activeAccessToken=auth?.accessToken||''
  activeRefreshToken=auth?.refreshToken||''
}

// "Beni hatırla" kapalıysa erişim jetonu uygulama açık olduğu sürece yenilenebilir,
// fakat dönen yenileme jetonu diske yazılmaz. Böylece pencere kapandığında oturum biter.
export function setSessionPersistence(persist:boolean){persistRefreshToken=persist}

// Overlay penceresi: ana pencerenin ittiği jetonu uygular ve bekleyen kiralama
// isteklerini çözer. Yenileme jetonu bilerek alınmaz (rotasyon ana pencerede kalır).
export function applyLeasedAuth(auth:{accessToken:string;user?:Record<string,unknown>|null}){
  if(!auth?.accessToken)return
  activeAccessToken=auth.accessToken
  activeRefreshToken=''
  const waiting=leaseWaiters
  leaseWaiters=[]
  const resolved:SessionAuth={user:auth.user||{},accessToken:auth.accessToken,refreshToken:''}
  waiting.forEach(resolve=>resolve(resolved))
}

// Overlay modunda /auth/refresh yerine ana pencereden jeton istenir (bkz. overlaySession.ts).
function leaseFromMainWindow():Promise<SessionAuth>{
  return new Promise<SessionAuth>((resolve,reject)=>{
    const timer=window.setTimeout(()=>{
      leaseWaiters=leaseWaiters.filter(item=>item!==settle)
      reject(new Error('SESSION_EXPIRED'))
    },LEASE_TIMEOUT_MS)
    const settle=(auth:SessionAuth)=>{window.clearTimeout(timer);resolve(auth)}
    leaseWaiters.push(settle)
    window.p2overlay?.send('auth-request',{reason:'expired'})
  })
}

export async function refreshClientSession(){
  if(refreshing)return refreshing
  refreshing=(async()=>{
    if(OVERLAY_MODE)return leaseFromMainWindow()
    const token=activeRefreshToken||await window.p2?.session.load()||''
    if(!token)throw new Error('SESSION_EXPIRED')
    const response=await fetch(API_BASE+'/auth/refresh',{method:'POST',headers:{'Content-Type':'application/json',...(window.p2?.version?{'X-P2-Client-Version':window.p2.version}:{})},body:JSON.stringify({refreshToken:token})})
    const data=await response.json().catch(()=>({})) as SessionAuth&{message?:string}
    if(!response.ok)throw data
    activeAccessToken=data.accessToken
    activeRefreshToken=data.refreshToken
    if(persistRefreshToken)await window.p2?.session.save(data.refreshToken)
    window.dispatchEvent(new CustomEvent('p2:auth-refreshed',{detail:data}))
    return data
  })().finally(()=>{refreshing=null})
  return refreshing
}

export async function p2Fetch(input:RequestInfo|URL,init:RequestInit={}){
  // Geliştirme iddiası: overlay penceresi yenileme zincirini asla kendisi döndürmez.
  if(OVERLAY_MODE&&import.meta.env.DEV&&String(input).includes('/auth/refresh'))throw new Error('Overlay penceresi /auth/refresh çağıramaz; jeton ana pencereden kiralanır.')
  const headers=new Headers(init.headers)
  if(window.p2?.version)headers.set('X-P2-Client-Version',window.p2.version)
  const supplied=headers.get('Authorization')
  if(supplied&&activeAccessToken)headers.set('Authorization',`Bearer ${activeAccessToken}`)
  let response=await fetch(input,{...init,headers})
  if(response.status!==401||!supplied||String(input).includes('/auth/refresh'))return response
  try{
    const auth=await refreshClientSession()
    headers.set('Authorization',`Bearer ${auth.accessToken}`)
    response=await fetch(input,{...init,headers})
  }catch{
    window.dispatchEvent(new CustomEvent('p2:session-expired'))
  }
  return response
}
