import { useEffect, useMemo, useRef, useState } from 'react'
import type { Organization } from './App'
type Auth={user:{id:string;username:string;displayName:string;avatarUrl?:string|null};accessToken:string}
import NotesEditor from './NotesEditor'
import ReleaseDiffPanel from './ReleaseDiff'
import { channelLabel, formatBytes, nextVersion, releaseRequest, type ReleaseDiff, type ReleaseView } from './library'
import { avatarHue } from './initials'
import { useEscape } from './useEscape'

type Project = { id: string; name: string }
// Açılır alanların sağındaki ok: tarayıcının yerleşik oku yerine temaya uyan ince şevron.
export const Chevron = () => <svg className="library-select-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg>

// Build klasörü yükleme sihirbazı: klasör → exe → kategori/sürüm/notlar → yükle → fark → yayınla.
export default function LibraryUploader({ auth, organization, project, latest, channels, defaultChannel, onClose, onDone }: {
  auth: Auth; organization: Organization; project: Project
  latest: Record<string, ReleaseView | null>
  channels: string[]
  defaultChannel?: string
  onClose: () => void
  onDone: (releaseId: string, publish: boolean) => Promise<void> | void
}) {
  useEscape(true, onClose)
  const requestId = useMemo(() => crypto.randomUUID(), [])
  const [picked, setPicked] = useState<{ path: string; executables: string[]; fileCount: number; totalSize: number; warnings: string[] } | null>(null)
  const [executable, setExecutable] = useState('')
  const [channel, setChannel] = useState(defaultChannel || '')
  const [version, setVersion] = useState('')
  const [notes, setNotes] = useState('')
  const [launchArgs, setLaunchArgs] = useState('')
  const [phase, setPhase] = useState<'form' | 'working' | 'ready'>('form')
  const [progress, setProgress] = useState<{ phase: string; percent: number; bytesDone?: number; bytesTotal?: number }>({ phase: '', percent: 0 })
  const [releaseId, setReleaseId] = useState('')
  const [diff, setDiff] = useState<{ loading: boolean; data: ReleaseDiff | null }>({ loading: false, data: null })
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const started = useRef(false)
  const cleanChannel = channel.trim().replace(/\s+/g, ' ')

  useEffect(() => setVersion(nextVersion(latest[cleanChannel]?.version)), [cleanChannel, latest])

  useEffect(() => {
    if (!window.p2?.library) return
    return window.p2.library.onEvent(event => {
      if (event.type !== 'upload' || event.requestId !== requestId) return
      if (event.releaseId) setReleaseId(event.releaseId)
      if (event.phase === 'ready') { setPhase('ready'); setProgress({ phase: 'ready', percent: 100 }) }
      else if (event.phase === 'failed') { setError(event.error || 'Yükleme başarısız.'); setPhase('form'); started.current = false }
      else if (event.phase === 'canceled') { setPhase('form'); started.current = false }
      else setProgress({ phase: event.phase, percent: event.percent || 0, bytesDone: event.bytesDone, bytesTotal: event.bytesTotal })
    })
  }, [requestId])

  // Yükleme bitince önceki paketle farkı getir (Steam'deki "ne değişti").
  useEffect(() => {
    if (phase !== 'ready' || !releaseId) return
    let alive = true
    setDiff({ loading: true, data: null })
    releaseRequest<ReleaseDiff>(`/organizations/${organization.id}/projects/${project.id}/releases/${releaseId}/diff`, auth.accessToken)
      .then(data => { if (alive) setDiff({ loading: false, data }) })
      .catch(() => { if (alive) setDiff({ loading: false, data: null }) })
    return () => { alive = false }
  }, [phase, releaseId, organization.id, project.id, auth.accessToken])

  async function pickFolder() {
    setError('')
    const result = await window.p2?.library?.pickBuildFolder()
    if (!result || !result.ok) { if (result && 'error' in result && result.error) setError(result.error); return }
    setPicked(result)
    setExecutable(result.executables[0] || '')
  }

  const canStart = Boolean(picked && executable && version.trim() && cleanChannel)

  async function start() {
    if (!canStart || started.current) return
    started.current = true
    setPhase('working'); setError('')
    const result = await window.p2!.library!.upload({ requestId, organizationId: organization.id, projectId: project.id, projectName: project.name, sourceDir: picked!.path, executable, launchArgs: launchArgs.trim() || undefined, channel: cleanChannel, version: version.trim(), notes: notes.trim() || undefined, overlay: true })
    if (!result.ok) { setError(result.code === 'RELEASE_VERSION_EXISTS' ? 'Bu kategoride bu sürüm numarası zaten var. Yeni bir numara ver.' : result.error || 'Yükleme başarısız.'); setPhase('form'); started.current = false }
  }

  async function cancel() {
    await window.p2?.library?.uploadCancel({ requestId, releaseId, organizationId: organization.id, projectId: project.id, discard: phase !== 'ready' })
    onClose()
  }

  async function finish(publish: boolean) {
    setBusy(true); setError('')
    try { await onDone(releaseId, publish); onClose() }
    catch (problem: any) { setError(problem?.message || 'İşlem başarısız.'); setBusy(false) }
  }

  return <div className="library-modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget && phase === 'form') onClose() }}>
    <section className="library-upload-modal">
      <header><div><p className="eyebrow">YENİ SÜRÜM</p><h2>{project.name} için build yükle</h2></div><button type="button" onClick={phase === 'form' ? onClose : cancel} aria-label="Kapat">×</button></header>

      {phase === 'form' && <div className="library-upload-body">
        <label className="library-field"><span>Build klasörü</span>
          {picked ? <button type="button" className="library-folder-pick chosen" onClick={pickFolder}><b>{picked.path}</b><small>{picked.fileCount} dosya · {formatBytes(picked.totalSize)}</small></button>
            : <button type="button" className="library-folder-pick" onClick={pickFolder}>Klasör seç…</button>}
        </label>
        {picked?.warnings?.map(warning => <p key={warning} className="library-warning">{warning}</p>)}
        {picked && <label className="library-field"><span>Başlatılacak dosya</span>
          <span className="library-select"><select value={executable} onChange={e => setExecutable(e.target.value)}>{picked.executables.map(exe => <option key={exe} value={exe}>{exe}</option>)}{!picked.executables.length && <option value="">(klasörde .exe yok)</option>}</select><Chevron /></span>
        </label>}
        <div className="library-field-row">
          <label className="library-field"><span>Kategori</span>
            <span className="library-select"><input list="library-channel-options" value={channel} onChange={e => setChannel(e.target.value)} placeholder="ör. alpha, beta, release" maxLength={24} /><Chevron /></span>
            <datalist id="library-channel-options">{channels.map(c => <option key={c} value={c} />)}</datalist>
          </label>
          <label className="library-field"><span>Sürüm</span><input value={version} onChange={e => setVersion(e.target.value)} placeholder="1.0.0" maxLength={40} /></label>
        </div>
        {channels.length > 0 && <div className="library-chip-row">{channels.map(c => <button type="button" key={c} className={`library-chip ${avatarHue('channel:' + c)}${cleanChannel === c ? ' active' : ''}`} onClick={() => setChannel(c)}><i />{channelLabel(c)}</button>)}</div>}
        <label className="library-field"><span>Başlatma argümanları (isteğe bağlı)</span><input value={launchArgs} onChange={e => setLaunchArgs(e.target.value)} placeholder="-windowed -log" maxLength={500} /></label>
        <div className="library-field"><span>Sürüm notları</span><NotesEditor token={auth.accessToken} value={notes} onChange={setNotes} placeholder="Bu sürümde neler değişti? Görsel ekleyebilir, kalın/italik yazabilirsin." /></div>
        {error && <div className="library-error">{error}</div>}
        <footer><button type="button" onClick={onClose}>Vazgeç</button><button type="button" className="library-primary" disabled={!canStart} onClick={start}>Yüklemeye başla</button></footer>
      </div>}

      {phase === 'working' && <div className="library-upload-progress">
        <div className="library-progress-bar"><i style={{ width: `${progress.percent}%` }} /></div>
        <p>{progress.phase === 'hashing' ? 'Dosyalar hazırlanıyor' : progress.phase === 'registering' ? 'Sunucuya kaydediliyor' : 'Parçalar yükleniyor'} · %{progress.percent}{progress.bytesTotal ? ` · ${formatBytes(progress.bytesDone || 0)} / ${formatBytes(progress.bytesTotal)}` : ''}</p>
        <button type="button" onClick={cancel}>İptal</button>
      </div>}

      {phase === 'ready' && <div className="library-upload-ready">
        <span className="library-ready-badge">✓</span>
        <h3>Yükleme tamamlandı</h3>
        <p>{version} sürümü <b>{channelLabel(cleanChannel)}</b> kategorisinde taslak olarak hazır. Yayınlarsan ekip görür ve kurulu oyunlar otomatik güncellenir.</p>
        <ReleaseDiffPanel diff={diff.data} loading={diff.loading} />
        {error && <div className="library-error">{error}</div>}
        <footer><button type="button" disabled={busy} onClick={() => finish(false)}>Taslak bırak</button><button type="button" className="library-primary" disabled={busy} onClick={() => finish(true)}>{busy ? 'Yayınlanıyor…' : 'Yayınla'}</button></footer>
      </div>}
    </section>
  </div>
}
