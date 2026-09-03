import { useEscape } from './useEscape'

// Uygulamanın kendi onay penceresi: tarayıcının gri kutusu yerine aynı görsel dil.
export default function ConfirmDialog({title,body,confirmLabel='Tamam',danger,onConfirm,onCancel}:{
  title:string
  body?:string
  confirmLabel?:string
  danger?:boolean
  onConfirm:()=>void
  onCancel:()=>void
}){
  useEscape(true,onCancel)
  return <div className="share-layer" onMouseDown={event=>{if(event.target===event.currentTarget)onCancel()}}>
    <section className="share-modal confirm-modal">
      <header><div><b>{title}</b>{body&&<small>{body}</small>}</div><button type="button" onClick={onCancel} aria-label="Kapat">×</button></header>
      <footer>
        <button type="button" className="share-cancel" onClick={onCancel}>Vazgeç</button>
        <button type="button" className={danger?'share-start danger':'share-start'} onClick={onConfirm}>{confirmLabel}</button>
      </footer>
    </section>
  </div>
}
