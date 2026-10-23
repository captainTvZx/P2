import Modal from './ui/Modal'

// Uygulamanın kendi onay penceresi: tarayıcının gri kutusu yerine aynı görsel dil.
export default function ConfirmDialog({title,body,confirmLabel='Tamam',danger,onConfirm,onCancel}:{
  title:string
  body?:string
  confirmLabel?:string
  danger?:boolean
  onConfirm:()=>void
  onCancel:()=>void
}){
  return <Modal size="sm" title={title} subtitle={body} onClose={onCancel} className="confirm-modal" footer={<>
    <button type="button" className="p2-btn secondary" onClick={onCancel}>Vazgeç</button>
    <button type="button" className={danger?'p2-btn danger':'p2-btn primary'} onClick={onConfirm}>{confirmLabel}</button>
  </>}>{null}</Modal>
}
