// OAuth akışları: sahte bir sağlayıcı ile PKCE kod değişimi, cihaz akışı,
// jeton yenileme ve jetonun arayüze sızmadığı sınanır.
import { createServer } from 'node:http'
import { createRequire } from 'node:module'
import Module from 'node:module'
import { createHash } from 'node:crypto'

const ok=[],bad=[]
const check=(n,c,e='')=>{(c?ok:bad).push(n);console.log((c?'✓ ':'✗ ')+n+(c?'':' → '+String(e).slice(0,200)))}

// electron modülünü sahtele: tarayıcı yerine isteği doğrudan sahte sağlayıcıya
// atıyoruz (beklemeden, yoksa geri çağrı sunucusuyla kilitlenir).
let opened=''
const original=Module._load
Module._load=function(request){
  if(request==='electron')return {shell:{openExternal:async(url)=>{opened=url;void fetch(url).catch(()=>{})}}}
  return original.apply(this,arguments)
}
const require=createRequire(import.meta.url)
const oauth=require('../electron/gitOAuth.cjs')

// Sunucunun tanıdığı istemciler: yerleşik adaylardan yalnızca ikincisi kabul
// ediliyor ki keşfin ilkini eleyip doğru olanı bulduğu görülsün.
const oauthModule=oauth
const KNOWN_CLIENTS=new Set(['CID',oauthModule.BUILTIN.gitea[1]])
const state={verifier:'',deviceApproved:false,deviceError:'',pollCount:0,refreshUsed:0}
const server=createServer(async(req,res)=>{
  const url=new URL(req.url,'http://127.0.0.1')
  const body=await new Promise(resolve=>{let raw='';req.on('data',c=>raw+=c);req.on('end',()=>resolve(new URLSearchParams(raw)))})
  res.setHeader('content-type','application/json')

  if(url.pathname==='/login/oauth/authorize'||url.pathname==='/oauth/authorize'){
    // Sağlayıcı kullanıcıyı onaylatıp redirect_uri'ye döner.
    state.challenge=url.searchParams.get('code_challenge')
    state.redirect=url.searchParams.get('redirect_uri')
    state.scope=url.searchParams.get('scope')
    const back=new URL(state.redirect)
    back.searchParams.set('code','KOD123')
    back.searchParams.set('state',url.searchParams.get('state'))
    await fetch(back).catch(()=>{})
    return res.end('{}')
  }
  if(url.pathname==='/login/oauth/access_token'||url.pathname==='/oauth/token'){
    // Gerçek sunucular tanımadıkları istemciyi böyle geri çevirir.
    if(!KNOWN_CLIENTS.has(body.get('client_id')))return res.end(JSON.stringify({error:'invalid_client',error_description:'client is not found'}))
    if(body.get('grant_type')==='refresh_token'){
      state.refreshUsed++
      return res.end(JSON.stringify({access_token:'YENI_JETON',refresh_token:'YENI_TAZELEME',expires_in:7200}))
    }
    if(body.get('grant_type')==='urn:ietf:params:oauth:grant-type:device_code'){
      state.pollCount++
      if(state.deviceError)return res.end(JSON.stringify({error:state.deviceError}))
      if(!state.deviceApproved)return res.end(JSON.stringify({error:'authorization_pending'}))
      return res.end(JSON.stringify({access_token:'CIHAZ_JETONU',expires_in:3600}))
    }
    // PKCE doğrulaması: verifier'ın özeti challenge'a eşit olmalı.
    state.verifier=body.get('code_verifier')
    const digest=createHash('sha256').update(state.verifier).digest('base64').replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'')
    if(digest!==state.challenge)return res.end(JSON.stringify({error:'invalid_grant',error_description:'PKCE eşleşmedi'}))
    if(body.get('code')!=='KOD123')return res.end(JSON.stringify({error:'invalid_grant'}))
    return res.end(JSON.stringify({access_token:'ERISIM_JETONU',refresh_token:'TAZELEME',expires_in:3600}))
  }
  if(url.pathname==='/api/v1/version')return res.end(JSON.stringify({version:'1.22.0'}))
  if(url.pathname==='/login/device/code'){
    state.deviceScope=body.get('scope')
    return res.end(JSON.stringify({device_code:'CIHAZ',user_code:'ABCD-1234',verification_uri:'http://127.0.0.1/onay',interval:1,expires_in:600}))
  }
  res.statusCode=404;res.end('{}')
})
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve))
const host=`http://127.0.0.1:${server.address().port}`

// uç noktalar
check('gitea/github uçları login/oauth altında',oauth.endpoints('gitea',host).authorize===`${host}/login/oauth/authorize`)
check('gitlab uçları oauth altında',oauth.endpoints('gitlab',host).token===`${host}/oauth/token`)

// PKCE üretimi
const pair=oauth.pkce()
const digest=createHash('sha256').update(pair.verifier).digest('base64').replace(/\+/g,'-').replace(/\//g,'_').replace(/=+$/,'')
check('PKCE challenge doğru üretiliyor',digest===pair.challenge)
check('PKCE verifier url-güvenli',!/[+/=]/.test(pair.verifier))

// yetkilendirme kodu akışı (Gitea)
const granted=await oauth.loginWithCode({kind:'gitea',host,clientId:'CID'})
check('OAuth GİRİŞ: erişim jetonu alındı',granted.accessToken==='ERISIM_JETONU',JSON.stringify(granted))
check('tazeleme jetonu ve süre saklanıyor',granted.refreshToken==='TAZELEME'&&granted.expiresAt>Date.now(),JSON.stringify(granted))
check('tarayıcı yetkilendirme adresiyle açıldı',opened.startsWith(`${host}/login/oauth/authorize`)&&opened.includes('code_challenge_method=S256'),opened.slice(0,120))
check('geri dönüş adresi 127.0.0.1 döngüsü',/^http:\/\/127\.0\.0\.1:\d+\/callback$/.test(granted.redirectUri),granted.redirectUri)
check('istenen kapsam gönderildi',state.scope===oauth.SCOPES.gitea,state.scope)
check('sunucu PKCE doğrulamasını yaptı',state.verifier.length>20)

// cihaz akışı (GitHub) — gerçek fonksiyonlar sahte sunucuya yönlendirildi
const started=await oauth.startDeviceLogin({clientId:'CID',host})
check('cihaz kodu ve kullanıcı kodu alındı',started.userCode==='ABCD-1234'&&started.deviceCode==='CIHAZ',JSON.stringify(started))
check('cihaz akışında repo kapsamı istendi',state.deviceScope===oauth.SCOPES.github,state.deviceScope)
check('sorgu aralığı en az 5 sn',started.interval>=5,started.interval)

const poll=()=>oauth.pollDeviceLogin({clientId:'CID',deviceCode:'CIHAZ',host})
check('onay öncesi bekleme durumu dönüyor',(await poll()).status==='pending')
state.deviceError='slow_down'
check('slow_down durumu tanınıyor',(await poll()).status==='slow')
state.deviceError='expired_token'
check('süresi dolmuş kod tanınıyor',(await poll()).status==='expired')
state.deviceError='access_denied'
check('reddedilen giriş tanınıyor',(await poll()).status==='denied')
state.deviceError='';state.deviceApproved=true
const done=await poll()
check('CİHAZ AKIŞI: onaydan sonra jeton geliyor',done.status==='ok'&&done.accessToken==='CIHAZ_JETONU',JSON.stringify(done))

// jeton yenileme
const renewed=await oauth.refresh({kind:'gitea',host,clientId:'CID',refreshToken:'TAZELEME'})
check('OTURUM KALICI: jeton sessizce yenileniyor',renewed.accessToken==='YENI_JETON'&&renewed.refreshToken==='YENI_TAZELEME',JSON.stringify(renewed))
check('yenileme sunucuya gitti',state.refreshUsed===1)
check('tazeleme jetonu yoksa yenileme atlanıyor',(await oauth.refresh({kind:'gitea',host,clientId:'CID',refreshToken:''}))===null)

// jeton gizliliği: arayüze giden nesnede hiçbir sır bulunmamalı
const providers=require('../electron/gitProviders.cjs')
const inner=providers.__internals
const shown=inner.publicShape({id:'gitea:https://x',kind:'gitea',host:'https://x',login:'ali',name:'Ali',avatar:'',auth:'oauth',token:'GIZLI',refreshToken:'GIZLI2',clientId:'CID',expiresAt:1})
check('JETON GİZLİ: arayüze jeton gönderilmiyor',!JSON.stringify(shown).includes('GIZLI'),JSON.stringify(shown))
check('bağlantı kimliği ve kullanıcı yine de görünüyor',shown.login==='ali'&&shown.auth==='oauth'&&shown.host==='https://x')
check('uygulama kaydında yalnızca client id taşınıyor',!JSON.stringify(inner.publicApp({id:'a',kind:'gitea',host:'https://x',clientId:'CID',secret:'GIZLI'})).includes('GIZLI'))

// git ağ komutları: jeton uzak adrese değil isteğe özel başlığa gider
const header=inner.basicHeader({kind:'gitea',login:'ali',token:'JETON'})
check('git için Basic başlığı üretiliyor',header.startsWith('Authorization: Basic ')&&Buffer.from(header.split(' ').pop(),'base64').toString()==='ali:JETON',header)
check('oauth bağlantısı Bearer kullanıyor',inner.authHeaders({auth:'oauth',token:'J'}).Authorization==='Bearer J')
check('gitlab jetonu PRIVATE-TOKEN ile gidiyor',inner.authHeaders({auth:'pat',kind:'gitlab',token:'J'})['PRIVATE-TOKEN']==='J')
check('gitea jetonu token şemasıyla gidiyor',inner.authHeaders({auth:'pat',kind:'gitea',token:'J'}).Authorization==='token J')

// yerleşik istemci keşfi: kullanıcı hiçbir şey tanımlamadan giriş yapabilmeli
check('tanınmayan istemci eleniyor',!(await oauth.clientAccepted({kind:'gitea',host,clientId:'YOK'})))
check('tanınan istemci kabul ediliyor',await oauth.clientAccepted({kind:'gitea',host,clientId:'CID'}))
const discovered=await oauth.discoverClientId({kind:'gitea',host})
check('KURULUMSUZ GİRİŞ: sunucunun yerleşik kaydı bulunuyor',discovered===oauth.BUILTIN.gitea[1],discovered)
check('gitea için üç yerleşik aday tanımlı',oauth.BUILTIN.gitea.length===3&&oauth.BUILTIN.github.length>=1)

// adres tanıma: yapıştırılan linkten sunucu türü çıkarılıyor
check('gitea sürüm ucundan tanınıyor',await providers.detectKind(host)==='gitea')
check('github adresi doğrudan tanınıyor',await providers.detectKind('https://github.com')==='github')
const gitlabFake=createServer((req,res)=>{
  res.setHeader('content-type','application/json')
  if(req.url==='/api/v4/version'){res.statusCode=401;return res.end('{"message":"401 Unauthorized"}')}
  res.statusCode=404;res.end('{}')
})
await new Promise(resolve=>gitlabFake.listen(0,'127.0.0.1',resolve))
check('gitlab kimlik isteyen sürüm ucundan tanınıyor',await providers.detectKind(`http://127.0.0.1:${gitlabFake.address().port}`)==='gitlab')
const boşSunucu=createServer((req,res)=>{res.statusCode=404;res.end('')})
await new Promise(resolve=>boşSunucu.listen(0,'127.0.0.1',resolve))
check('git sunucusu olmayan adres ayırt ediliyor',await providers.detectKind(`http://127.0.0.1:${boşSunucu.address().port}`)==='')
await new Promise(resolve=>gitlabFake.close(resolve))
await new Promise(resolve=>boşSunucu.close(resolve))

await new Promise(resolve=>server.close(resolve))
Module._load=original
console.log(`\n${ok.length} geçti, ${bad.length} kaldı`)
process.exitCode=bad.length?1:0
