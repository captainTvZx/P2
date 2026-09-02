// Ekran paylaşımı kalite tanımları. Yayıncı yakalama çözünürlüğünü ve kare hızını
// seçer; izleyen taraf kendi için daha düşük (ya da tam) kalite isteyebilir.
export type QualityId='360p'|'480p'|'720p'|'1080p'|'1440p'|'2160p'
export type ViewerQuality=QualityId|'auto'

export type Quality={id:QualityId;label:string;width:number;height:number;bitrate:number}

export const QUALITIES:Quality[]=[
  {id:'2160p',label:'4K · 2160p',width:3840,height:2160,bitrate:16_000_000},
  {id:'1440p',label:'2K · 1440p',width:2560,height:1440,bitrate:9_000_000},
  {id:'1080p',label:'Full HD · 1080p',width:1920,height:1080,bitrate:4_500_000},
  {id:'720p',label:'HD · 720p',width:1280,height:720,bitrate:2_500_000},
  {id:'480p',label:'Düşük · 480p',width:854,height:480,bitrate:1_000_000},
  {id:'360p',label:'Çok düşük · 360p',width:640,height:360,bitrate:500_000},
]
export const FRAME_RATES=[5,15,30,60]

export function quality(id:QualityId):Quality{return QUALITIES.find(item=>item.id===id)||QUALITIES[2]}
// Kare hızı arttıkça aynı çözünürlük daha fazla bant genişliği ister.
export function bitrateFor(id:QualityId,fps:number){return Math.round(quality(id).bitrate*(fps>=60?1.5:fps>=30?1:0.7))}

export type ShareOptions={sourceId:string;quality:QualityId;fps:number;audio:boolean}
export type ShareInfo={quality:QualityId;fps:number;audio:boolean}
