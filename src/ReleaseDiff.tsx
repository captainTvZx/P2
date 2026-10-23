import { formatBytes, type ReleaseDiff } from './library'

// Önceki paketle fark paneli (Steam'in "bu güncellemede ne değişti" özeti):
// hem yükleyicinin "hazır" ekranında hem sürüm kartlarında kullanılır.
export default function ReleaseDiffPanel({ diff, loading, compact }: { diff: ReleaseDiff | null; loading?: boolean; compact?: boolean }) {
  if (loading) return <div className="library-diff"><p className="library-hint">Önceki paketle karşılaştırılıyor…</p></div>
  if (!diff) return <div className="library-diff"><p className="library-hint">Fark hesaplanamadı.</p></div>
  if (!diff.against) return <div className="library-diff"><p className="library-hint">Karşılaştırılacak önceki paket yok; bu, kategorinin ilk sürümü. {diff.added} dosya · {formatBytes(diff.addedBytes)}</p></div>
  const nothing = !diff.added && !diff.changed && !diff.removed
  return <div className="library-diff">
    <div className="library-diff-head">
      <span><b>{diff.against.version}</b> <small>({diff.against.channel})</small> sürümüne göre</span>
      <span className="library-diff-download" title="Önceki sürümü kurmuş biri yalnız bu kadar indirir"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 3v12m0 0l-4-4m4 4l4-4M5 21h14" /></svg>Güncelleme indirmesi <b>{formatBytes(diff.downloadBytes)}</b></span>
    </div>
    {nothing ? <p className="library-hint">Dosyalar önceki sürümle birebir aynı.</p> : <div className="library-diff-stats">
      <div className="library-diff-stat add"><small>Yeni</small><b>{diff.added}</b><span>{formatBytes(diff.addedBytes)}</span></div>
      <div className="library-diff-stat change"><small>Değişen</small><b>{diff.changed}</b><span>{formatBytes(diff.changedBytes)}</span></div>
      <div className="library-diff-stat remove"><small>Silinen</small><b>{diff.removed}</b><span>{formatBytes(diff.removedBytes)}</span></div>
      <div className="library-diff-stat"><small>Aynı</small><b>{diff.unchanged}</b><span>dosya</span></div>
    </div>}
    {!compact && !nothing && <div className="library-diff-files">
      <FileGroup label="Yeni dosyalar" tone="add" rows={diff.files.added.map(file => ({ path: file.path, meta: formatBytes(file.size) }))} />
      <FileGroup label="Değişen dosyalar" tone="change" rows={diff.files.changed.map(file => ({ path: file.path, meta: `${formatBytes(file.previousSize)} → ${formatBytes(file.size)}` }))} />
      <FileGroup label="Silinen dosyalar" tone="remove" rows={diff.files.removed.map(file => ({ path: file.path, meta: formatBytes(file.size) }))} />
      {diff.truncated && <p className="library-hint">Liste kısaltıldı; sayılar tam, dosya adlarının yalnız ilk 300'ü gösteriliyor.</p>}
    </div>}
  </div>
}

function FileGroup({ label, tone, rows }: { label: string; tone: 'add' | 'change' | 'remove'; rows: Array<{ path: string; meta: string }> }) {
  if (!rows.length) return null
  return <details className={`library-diff-group ${tone}`}>
    <summary><i />{label} <em>{rows.length}</em></summary>
    <ul>{rows.map(row => <li key={row.path}><code>{row.path}</code><small>{row.meta}</small></li>)}</ul>
  </details>
}
