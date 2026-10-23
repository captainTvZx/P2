import { Suspense, lazy, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import RichInput, { type RichFormatState, type RichHandle } from './RichComposer'
import { AttachmentPicker, AttachmentQueue, attachmentsFromContent, useAttachments, withoutAttachments } from './attachments'
import { ComposerIcon } from './ComposerIcons'
import { useEscape } from './useEscape'

const EmojiPanel = lazy(() => import('./EmojiPanel'))

// Sürüm notları / aktivite gönderisi editörü: mesaj yazma kutusunun aynısı
// (kalın, italik, bağlantı, emoji, görsel/dosya ekleme). Dışarıya mesajlarla aynı
// biçim verilir: markdown metin + satır başına bir ek işareti; MessageRichText okur.
// Yazı alanı sağ alt köşeden (eski forum kutuları gibi) boyutlandırılabilir; emoji
// paneli gövdeye sabit konumla çizilir, böylece pencere içinde taşma/kırpma olmaz.
export default function NotesEditor({ token, value, onChange, placeholder, autoFocus, onSubmit, submitLabel, busy, compact }: {
  token: string; value: string; onChange: (markdown: string) => void; placeholder: string
  autoFocus?: boolean; onSubmit?: () => void; submitLabel?: string; busy?: boolean; compact?: boolean
}) {
  const editor = useRef<RichHandle>(null)
  const emojiButton = useRef<HTMLButtonElement>(null)
  const popover = useRef<HTMLDivElement>(null)
  const [text, setText] = useState(() => withoutAttachments(value))
  const attachments = useAttachments(token, attachmentsFromContent(value))
  const [format, setFormat] = useState<RichFormatState>({ bold: false, italic: false, link: false })
  const [emoji, setEmoji] = useState<{ left: number; top: number } | null>(null)
  // Yazı alanı yüksekliği: sağ alttaki tutamaçla sürüklenir (eski forum kutuları gibi).
  const [height, setHeight] = useState<number | null>(null)
  const emitted = useRef(value)
  function startResize(e: React.MouseEvent) {
    e.preventDefault()
    const node = e.currentTarget.closest('.library-notes-editor')?.querySelector<HTMLElement>('.composer-editor')
    const startY = e.clientY, startH = node?.offsetHeight || height || (compact ? 40 : 150)
    const move = (ev: MouseEvent) => setHeight(Math.max(compact ? 40 : 72, Math.min(window.innerHeight * 0.7, startH + ev.clientY - startY)))
    const up = () => { window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); document.body.style.cursor = '' }
    document.body.style.cursor = 'ns-resize'
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up)
  }
  useEscape(Boolean(emoji), () => setEmoji(null))

  // Dışarıdan sıfırlanınca (gönderi yollandı, başka sürüm açıldı) kutu da sıfırlanır.
  useEffect(() => {
    if (value === emitted.current) return
    emitted.current = value
    setText(withoutAttachments(value))
    attachments.clear()
  }, [value])

  useEffect(() => {
    const next = attachments.appendTo(text)
    if (next === emitted.current) return
    emitted.current = next
    onChange(next)
  }, [text, attachments.files])

  useEffect(() => { if (autoFocus) editor.current?.focus() }, [autoFocus])

  // Emoji paneli: düğmenin altına (yer yoksa üstüne) sabit konumla; dışarı tıklayınca kapanır.
  function toggleEmoji() {
    if (emoji) { setEmoji(null); return }
    const box = emojiButton.current?.getBoundingClientRect(); if (!box) return
    const width = 330, height = 360
    const left = Math.max(8, Math.min(box.left, window.innerWidth - width - 8))
    const top = box.bottom + 6 + height <= window.innerHeight ? box.bottom + 6 : Math.max(8, box.top - 6 - height)
    setEmoji({ left, top })
  }
  useEffect(() => {
    if (!emoji) return
    const away = (e: PointerEvent) => { const t = e.target as Node; if (popover.current?.contains(t) || emojiButton.current?.contains(t)) return; setEmoji(null) }
    document.addEventListener('pointerdown', away, true)
    return () => document.removeEventListener('pointerdown', away, true)
  }, [emoji])

  return <div className={`message-composer slack-composer library-notes-editor${compact ? ' compact' : ''}`} style={height ? ({ '--notes-h': `${height}px` } as React.CSSProperties) : undefined}>
    <AttachmentQueue state={attachments} />
    <div className="composer-input-row">
      <AttachmentPicker state={attachments} />
      <RichInput ref={editor} value={text} onChange={next => setText(next.slice(0, 4000))} onEnter={() => { if (onSubmit) onSubmit(); else document.execCommand('insertLineBreak') }} onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey && !onSubmit) { e.preventDefault(); document.execCommand('insertLineBreak') } }} onStateChange={setFormat} onFiles={files => void attachments.add(files)} placeholder={placeholder} />
      <span className="library-resize-grip" role="separator" aria-orientation="horizontal" aria-label="Yazı alanı yüksekliği" title="Sürükleyerek büyüt/küçült" onMouseDown={startResize}><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M11 1L1 11M11 6L6 11" /></svg></span>
    </div>
    <div className="composer-toolbar"><span>
      <button type="button" className={format.bold ? 'active' : undefined} title="Kalın (Ctrl+B)" onMouseDown={e => e.preventDefault()} onClick={() => editor.current?.format('bold')}><ComposerIcon name="bold" /></button>
      <button type="button" className={format.italic ? 'active' : undefined} title="İtalik (Ctrl+I)" onMouseDown={e => e.preventDefault()} onClick={() => editor.current?.format('italic')}><ComposerIcon name="italic" /></button>
      <button type="button" className={format.link ? 'active' : undefined} title="Bağlantı ekle" onMouseDown={e => e.preventDefault()} onClick={() => editor.current?.openLink()}><ComposerIcon name="link" /></button><i />
      <button ref={emojiButton} type="button" className={`emoji-toggle${emoji ? ' active' : ''}`} title="Emoji seç" onMouseDown={e => e.preventDefault()} onClick={toggleEmoji}><ComposerIcon name="emoji" /></button>
      <button type="button" title="Görsel veya dosya ekle" onMouseDown={e => e.preventDefault()} onClick={() => attachments.input.current?.click()}><ComposerIcon name="plus" /></button>
    </span>
      {onSubmit && <button type="button" className="composer-send library-notes-send" title={submitLabel || 'Gönder'} disabled={busy || attachments.busy || (!text.trim() && !attachments.files.length)} onClick={onSubmit}>{submitLabel ? <span>{submitLabel}</span> : null}<ComposerIcon name="send" /></button>}
    </div>
    {emoji && createPortal(<div ref={popover} className="emoji-picker-popover library-emoji-popover" style={{ position: 'fixed', left: emoji.left, top: emoji.top, right: 'auto', bottom: 'auto', zIndex: 400 }}><Suspense fallback={<div className="emoji-loading">Emojiler hazırlanıyor…</div>}><EmojiPanel onDismiss={() => setEmoji(null)} onSelect={item => editor.current?.insert(item)} /></Suspense></div>, document.body)}
  </div>
}
