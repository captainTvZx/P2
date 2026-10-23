import { useRef, useState } from 'react'
import { API_BASE, p2Fetch } from './apiBase'
import { userInitials } from './initials'
import { useEscape } from './useEscape'

// Proje vitrini ayarları: ray ikonu ve kütüphane kapağı (16:9). Haber/açıklama yok;
// ekip aktiviteyi Kütüphane'deki akışa elle yazar.
// Kütüphane ekranından ve proje rayının menüsünden aynı pencere açılır.
export type ProjectSummary = { id: string; name: string; description?: string | null; coverUrl?: string | null; iconUrl?: string | null }

// Sunucu görselleri "/uploads/<id>.png" olarak saklar; ekranda API adresiyle birleştirilir.
export const imageSrc = (url?: string | null) => url ? (url.startsWith('/') ? API_BASE + url : url) : ''

const readDataUrl = (file: File) => new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(reader.error); reader.readAsDataURL(file) })

export async function uploadImage(token: string, file: File) {
  if (!/^image\//.test(file.type)) throw new Error('Yalnız görsel dosyası yüklenebilir.')
  if (file.size > 8 * 1024 * 1024) throw new Error('Görsel 8 MB sınırını aşıyor.')
  const response = await p2Fetch(API_BASE + '/uploads', { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify({ name: file.name, type: file.type, data: await readDataUrl(file) }) })
  const data = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(data.message || 'Görsel yüklenemedi.')
  return String(data.url) // "/uploads/<id>.<uzantı>"
}

export async function patchProject(token: string, organizationId: string, projectId: string, body: Record<string, unknown>) {
  const response = await p2Fetch(`${API_BASE}/organizations/${organizationId}/projects/${projectId}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` }, body: JSON.stringify(body) })
  const data = await response.json().catch(() => ({}))
  if (!response.ok) throw new Error(data.message || 'Kaydedilemedi.')
  return data as ProjectSummary
}

export default function ProjectSettingsDialog({ token, organizationId, project, onClose, onSaved }: { token: string; organizationId: string; project: ProjectSummary; onClose: () => void; onSaved: (project: ProjectSummary) => void }) {
  useEscape(true, onClose)
  const [iconUrl, setIconUrl] = useState<string | null>(project.iconUrl || null)
  const [coverUrl, setCoverUrl] = useState<string | null>(project.coverUrl || null)
  const [busy, setBusy] = useState<'' | 'icon' | 'cover' | 'save'>('')
  const [error, setError] = useState('')
  const iconInput = useRef<HTMLInputElement>(null)
  const coverInput = useRef<HTMLInputElement>(null)

  async function pick(kind: 'icon' | 'cover', file: File | undefined) {
    if (!file) return
    setBusy(kind); setError('')
    try { const url = await uploadImage(token, file); if (kind === 'icon') setIconUrl(url); else setCoverUrl(url) }
    catch (problem: any) { setError(problem?.message || 'Görsel yüklenemedi.') }
    finally { setBusy('') }
  }

  async function save() {
    setBusy('save'); setError('')
    const body: Record<string, unknown> = {}
    if ((iconUrl || null) !== (project.iconUrl || null)) body.iconUrl = iconUrl
    if ((coverUrl || null) !== (project.coverUrl || null)) body.coverUrl = coverUrl
    try {
      const saved = Object.keys(body).length ? await patchProject(token, organizationId, project.id, body) : project
      onSaved(saved)
    } catch (problem: any) { setError(problem?.message || 'Kaydedilemedi.'); setBusy('') }
  }

  return <div className="library-modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}>
    <section className="library-upload-modal library-project-modal">
      <header><div><p className="eyebrow">PROJE AYARLARI</p><h2>{project.name}</h2></div><button type="button" onClick={onClose} aria-label="Kapat">×</button></header>
      <div className="library-upload-body">
        <div className="library-field"><span>Proje ikonu</span>
          <div className="library-project-identity">
            <button type="button" className="library-project-icon" onClick={() => iconInput.current?.click()} title="İkonu değiştir" disabled={busy === 'icon'}>{iconUrl ? <img src={imageSrc(iconUrl)} alt="" /> : userInitials(project.name)}</button>
            <div className="library-image-actions">
              <button type="button" onClick={() => iconInput.current?.click()} disabled={busy === 'icon'}>{busy === 'icon' ? 'Yükleniyor…' : iconUrl ? 'İkonu değiştir' : 'İkon yükle'}</button>
              {iconUrl && <button type="button" onClick={() => setIconUrl(null)}>Kaldır</button>}
              <small className="library-hint">Kare görsel; rayda ve kütüphanede görünür.</small>
            </div>
            <input ref={iconInput} type="file" accept="image/*" hidden onChange={e => { void pick('icon', e.target.files?.[0]); e.target.value = '' }} />
          </div>
        </div>
        <div className="library-field"><span>Kütüphane kapağı</span>
          <button type="button" className="library-cover-preview" onClick={() => coverInput.current?.click()} disabled={busy === 'cover'}>{coverUrl ? <img src={imageSrc(coverUrl)} alt="" /> : <span>{busy === 'cover' ? 'Yükleniyor…' : 'Kapak seç (16:9 önerilir)'}</span>}</button>
          <div className="library-image-actions">
            <button type="button" onClick={() => coverInput.current?.click()} disabled={busy === 'cover'}>{coverUrl ? 'Kapağı değiştir' : 'Kapak yükle'}</button>
            {coverUrl && <button type="button" onClick={() => setCoverUrl(null)}>Kaldır</button>}
          </div>
          <input ref={coverInput} type="file" accept="image/*" hidden onChange={e => { void pick('cover', e.target.files?.[0]); e.target.value = '' }} />
        </div>
        {error && <div className="library-error">{error}</div>}
        <footer><button type="button" onClick={onClose}>Vazgeç</button><button type="button" className="library-primary" disabled={busy !== ''} onClick={save}>{busy === 'save' ? 'Kaydediliyor…' : 'Kaydet'}</button></footer>
      </div>
    </section>
  </div>
}
