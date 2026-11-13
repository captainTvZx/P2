// HTML rapor: ekran başına Electron | Rust | fark, kaydırmalı üst üste karşılaştırma, bölge ve karo tablosu.
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c])

const STYLE = `
:root{color-scheme:dark;--bg:#0b0f17;--fg:#d6e4ff;--mut:#7d93b8;--line:#1f2f52;--bad:#ff5c6c;--ok:#3ddc84}
body{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:14px/1.5 'Segoe UI',system-ui,sans-serif}
h1{font-size:20px;margin:0 0 4px} h2{font-size:16px;margin:0} .mut{color:var(--mut)}
table{border-collapse:collapse;margin:12px 0} td,th{padding:4px 10px;border-bottom:1px solid var(--line);text-align:left}
.pct{font-variant-numeric:tabular-nums;font-weight:700} .bad{color:var(--bad)} .ok{color:var(--ok)}
section{margin:28px 0;padding-top:16px;border-top:1px solid var(--line)}
.imgs{display:flex;gap:8px;flex-wrap:wrap} .imgs figure{margin:0} figcaption{color:var(--mut);font-size:12px}
.imgs img{width:400px;border:1px solid var(--line);cursor:zoom-in}
.cmp{position:relative;width:1280px;max-width:100%;margin-top:12px;border:1px solid var(--line)}
.cmp img{display:block;width:100%} .cmp .top{position:absolute;inset:0;overflow:hidden}
.cmp .top img{width:1280px;max-width:none}
.err{color:var(--bad);white-space:pre-wrap;font:12px Consolas,monospace}
`
const slider = id => `<input type="range" min="0" max="100" value="50" oninput="document.getElementById('${id}').style.width=this.value+'%'" style="width:400px">`

export function renderReport(report) {
  const rows = report.results.map(r => `<tr><td><a href="#${esc(r.id)}-${esc(r.theme)}">${esc(r.id)}</a></td><td>${esc(r.theme)}</td><td class="pct ${r.mismatchPct > 5 ? 'bad' : 'ok'}">${r.mismatchPct ?? '—'}${r.mismatchPct !== undefined ? '%' : ''}</td><td>${r.meanAbsError ?? ''}</td><td class="mut">${esc((r.electronError || r.rustError || '').split('\n')[0])}</td></tr>`).join('')
  const sections = report.results.map(r => {
    const key = `${r.id}-${r.theme}`
    const f = r.files
    const imgs = ['electron', 'rust', 'diff'].filter(k => f[k]).map(k => `<figure><a href="${esc(f[k])}" target="_blank"><img src="${esc(f[k])}"></a><figcaption>${k === 'electron' ? 'Electron (referans)' : k === 'rust' ? 'Rust' : 'fark (kırmızı = uyumsuz)'}</figcaption></figure>`).join('')
    const regions = (r.regions || []).length ? `<table><tr><th>bölge</th><th>dikdörtgen</th><th>uyumsuz</th></tr>${r.regions.map(g => `<tr><td>${esc(g.name)}</td><td class="mut">${g.rect.join(', ')}</td><td class="pct ${g.mismatchPct > 5 ? 'bad' : 'ok'}">${g.mismatchPct}%</td></tr>`).join('')}</table>` : ''
    const tiles = (r.worstTiles || []).length ? `<p class="mut">En kötü 40×40 karolar: ${r.worstTiles.slice(0, 8).map(t => `(${t.x},${t.y}) %${t.pct}`).join(' · ')}</p>` : ''
    const cmp = f.electron && f.rust ? `<p class="mut">Üst üste: sol Electron, sağ Rust — kaydırıcıyla ayır.</p>${slider('top-' + key)}<div class="cmp"><img src="${esc(f.rust)}"><div class="top" id="top-${esc(key)}" style="width:50%"><img src="${esc(f.electron)}"></div></div>` : ''
    const errs = [r.electronError && 'Electron: ' + r.electronError, r.rustError && 'Rust: ' + r.rustError].filter(Boolean).map(e => `<div class="err">${esc(e)}</div>`).join('')
    return `<section id="${esc(key)}"><h2>${esc(r.title || r.id)} <span class="mut">· ${esc(r.id)} · ${esc(r.theme)}</span> <span class="pct ${r.mismatchPct > 5 ? 'bad' : 'ok'}">${r.mismatchPct !== undefined ? '%' + r.mismatchPct : ''}</span></h2>${errs}<div class="imgs">${imgs}</div>${regions}${tiles}${cmp}</section>`
  }).join('')
  return `<!doctype html><html lang="tr"><head><meta charset="utf-8"><title>P2 parite ${esc(report.run)}</title><style>${STYLE}</style></head><body>
<h1>P2 görsel parite — ${esc(report.run)}</h1><p class="mut">Sunucu ${esc(report.api)} · eşik ${report.threshold} (kanal farkı) · bekleme ${report.settle} ms · Electron referanstır.</p>
<table><tr><th>ekran</th><th>tema</th><th>uyumsuz</th><th>ort. fark</th><th>not</th></tr>${rows}</table>${sections}</body></html>`
}

export function renderIndex(history, latest) {
  const rows = Object.entries(history).sort().map(([k, list]) => {
    const last = list[list.length - 1], first = list[0]
    return `<tr><td>${esc(k)}</td><td class="pct ${last.mismatchPct > 5 ? 'bad' : 'ok'}">%${last.mismatchPct}</td><td class="mut">%${first.mismatchPct} (${esc(first.run)})</td><td class="mut">${list.length}</td><td><a href="runs/${esc(last.run)}/report.html#${esc(k)}">son</a></td></tr>`
  }).join('')
  return `<!doctype html><html lang="tr"><head><meta charset="utf-8"><title>P2 parite</title><style>${STYLE}</style></head><body>
<h1>P2 görsel parite</h1><p>Son koşu: <a href="runs/${esc(latest)}/report.html">${esc(latest)}</a></p>
<table><tr><th>ekran-tema</th><th>son</th><th>ilk (taban)</th><th>koşu</th><th></th></tr>${rows}</table></body></html>`
}
