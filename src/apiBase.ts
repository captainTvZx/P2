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
