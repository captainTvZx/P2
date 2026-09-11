const PHASES = {
  'Counting objects': { phase: 'counting', label: 'Nesneler sayılıyor' },
  'Compressing objects': { phase: 'compressing', label: 'Nesneler hazırlanıyor' },
  'Receiving objects': { phase: 'receiving', label: 'Dosyalar alınıyor' },
  'Resolving deltas': { phase: 'resolving', label: 'Değişiklikler işleniyor' },
  'Updating files': { phase: 'updating', label: 'Dosyalar hazırlanıyor' },
}

// `git clone --progress` ilerlemeyi stderr'e, aynı satırı \r ile yenileyerek yazar.
// Son tamamlanmış ilerleme parçasını bulup arayüzün kullanacağı güvenli sayılara çevirir.
function parseCloneProgress(raw) {
  const text = String(raw || '').replace(/\x1b\[[0-9;]*m/g, '')
  const matcher = /(Counting objects|Compressing objects|Receiving objects|Resolving deltas|Updating files):\s*(\d+)%\s*\(\s*(\d+)\s*\/\s*(\d+)\s*\)/g
  let match = null
  for (let found = matcher.exec(text); found; found = matcher.exec(text)) match = found
  if (!match) return null
  const percent = Math.max(0, Math.min(100, Number(match[2]) || 0))
  const current = Math.max(0, Number(match[3]) || 0)
  const total = Math.max(current, Number(match[4]) || 0)
  return { ...PHASES[match[1]], percent, current, total, remaining: Math.max(0, total - current) }
}

function createCloneProgressParser(onProgress) {
  let tail = ''
  let previous = ''
  return chunk => {
    tail = (tail + String(chunk || '')).slice(-8192)
    const progress = parseCloneProgress(tail)
    if (!progress) return
    const signature = `${progress.phase}:${progress.percent}:${progress.current}:${progress.total}`
    if (signature === previous) return
    previous = signature
    onProgress(progress)
  }
}

module.exports = { parseCloneProgress, createCloneProgressParser }
