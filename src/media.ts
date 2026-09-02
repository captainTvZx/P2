// Tarayıcılar mikrofon/kamerayı yalnızca güvenli bağlamda (https:// veya localhost) verir.
// Güvensiz bir adreste `navigator.mediaDevices` tanımsızdır; bu yüzden her kullanım öncesi
// buradan geçilir, aksi hâlde arayüz çöker.
export function mediaDevices():MediaDevices|null{
  return typeof navigator!=='undefined'&&navigator.mediaDevices?navigator.mediaDevices:null
}
export function canUseMedia(){return Boolean(mediaDevices()?.getUserMedia)}
export function mediaBlockReason(){
  if(canUseMedia())return ''
  if(typeof window!=='undefined'&&!window.isSecureContext)
    return 'Mikrofon ve kamera yalnızca güvenli bağlantıda çalışır. Bu sayfa http:// üzerinden açıldığı için tarayıcı erişimi engelliyor; uygulamayı https:// adresinden ya da masaüstü sürümünden aç.'
  return 'Bu tarayıcı mikrofon ve kamera erişimini desteklemiyor.'
}
