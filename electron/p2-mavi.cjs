// P2 Mavi — Electron tarafı pencereler (açılış splash, çağrı baloncuğu, oyun başlatma kartı, sunucu konsolu)
// renderer CSS'ini yüklemez; bu modül aynı tasarım dilinin sabit değerli kopyasını verir.
// Değerler src/p2-theme.css ile aynıdır: orayı değiştirirsen burayı da güncelle.
const fs = require('node:fs')
const path = require('node:path')

const P2_MAVI_CSS = `
:root{--p2-bg-0:#070b14;--p2-bg-1:#0a1020;--p2-bg-2:#0e1626;--p2-bg-3:#121c30;--p2-bg-hover:#182440;--p2-line:#1f2f52;
--p2-text:#d6e4ff;--p2-text-strong:#eef5ff;--p2-text-2:rgba(214,228,255,.78);--p2-text-3:#7d93b8;--p2-text-4:#4d6690;--p2-text-accent:#7fb6ff;
--p2-accent:#2c7bff;--p2-accent-hover:#4f9dff;--p2-accent-grad:linear-gradient(135deg,#4f9dff,#2c7bff);--p2-accent-bar:linear-gradient(90deg,#4f9dff,#8fd0ff);
--p2-accent-soft:rgba(44,123,255,.16);--p2-border:rgba(120,170,255,.18);--p2-border-strong:rgba(96,170,255,.38);
--p2-glass:linear-gradient(135deg,rgba(14,22,38,.96),rgba(8,12,22,.94));--p2-glass-strong:linear-gradient(160deg,#131d31 0%,#0e1626 48%,#0a1020 100%);
--p2-inset-top:inset 0 1px 0 rgba(255,255,255,.08);--p2-shadow-lg:0 18px 44px rgba(0,0,0,.55),0 0 0 1px rgba(0,0,0,.35),0 0 34px rgba(40,120,255,.18),inset 0 1px 0 rgba(255,255,255,.08);
--p2-success:#3ddc84;--p2-danger:#ff5c6c;--p2-danger-soft:rgba(255,92,108,.16);--p2-danger-border:rgba(255,92,108,.42);--p2-danger-text:#ff9aa4;
--p2-r-sm:8px;--p2-r-md:11px;--p2-r-lg:14px;--ease-elastic:cubic-bezier(.2,1.5,.35,1);--ease-pop:cubic-bezier(.2,1.4,.4,1);--ease-out:cubic-bezier(.2,.8,.2,1)}
*{box-sizing:border-box}
body{margin:0;color:var(--p2-text);font-family:Inter,"Segoe UI",system-ui,sans-serif;-webkit-font-smoothing:antialiased}
.p2-card{position:relative;overflow:hidden;border:1px solid var(--p2-border-strong);border-radius:var(--p2-r-lg);background:var(--p2-glass);box-shadow:var(--p2-shadow-lg);color:#fff}
.p2-card.shine::after{content:"";position:absolute;top:-20%;bottom:-20%;left:0;width:38%;pointer-events:none;background:linear-gradient(90deg,transparent,rgba(120,180,255,.14),transparent);animation:p2-shine 1.1s ease-out .25s both}
.p2-kicker{font-size:9.5px;font-weight:800;letter-spacing:.18em;text-transform:uppercase;color:var(--p2-text-accent)}
.p2-btn{display:inline-flex;align-items:center;justify-content:center;gap:6px;height:32px;padding:0 12px;border:1px solid var(--p2-border);border-radius:var(--p2-r-sm);background:var(--p2-bg-3);color:var(--p2-text);font:650 12px Inter,"Segoe UI",sans-serif;cursor:pointer;transition:background .14s,border-color .14s,color .14s,transform .14s}
.p2-btn:hover{background:var(--p2-bg-hover);border-color:var(--p2-border-strong);color:var(--p2-text-strong)}
.p2-btn:active{transform:scale(.96)}
.p2-btn.primary{background:var(--p2-accent-grad);border-color:transparent;color:#fff;box-shadow:0 6px 18px rgba(20,90,220,.35),var(--p2-inset-top)}
.p2-btn.danger{background:var(--p2-danger-soft);border-color:var(--p2-danger-border);color:var(--p2-danger-text)}
.p2-btn.danger:hover{background:var(--p2-danger);border-color:var(--p2-danger);color:#fff}
.p2-btn.icon{width:32px;padding:0}
.p2-btn svg{width:17px;height:17px;fill:currentColor}
.p2-logo{display:block;border-radius:25%;object-fit:cover;box-shadow:0 6px 18px rgba(20,90,220,.45),0 0 0 1px rgba(120,180,255,.25)}
.p2-logo.pop{animation:p2-logo-pop .55s var(--ease-pop) .12s both}
.p2-logo-fallback{display:grid;place-items:center;border-radius:25%;background:linear-gradient(135deg,#2c7bff,#173064);color:#eef5ff;font-weight:900}
.p2-bar{position:relative;height:5px;overflow:hidden;border-radius:99px;background:rgba(120,170,255,.14)}
.p2-bar i{position:absolute;top:0;bottom:0;left:0;width:0;border-radius:99px;background:var(--p2-accent-bar);transition:width .36s var(--ease-out)}
.p2-bar.indeterminate i{width:38%;animation:p2-run 1.3s ease-in-out infinite}
.p2-in{animation:p2-pop-elastic .62s var(--ease-elastic) both}
@keyframes p2-run{0%{left:-40%}100%{left:105%}}
@keyframes p2-shine{from{transform:translateX(-120%) skewX(-18deg)}to{transform:translateX(260%) skewX(-18deg)}}
@keyframes p2-logo-pop{0%{transform:scale(.4) rotate(-14deg);opacity:0}55%{transform:scale(1.14) rotate(3deg);opacity:1}100%{transform:none}}
@keyframes p2-pop-elastic{from{opacity:0;transform:scale(.86) translateY(14px)}to{opacity:1;transform:none}}
@keyframes p2-pulse{0%,100%{opacity:1}50%{opacity:.55}}
@media (prefers-reduced-motion:reduce){*,*::before,*::after{animation-duration:1ms!important;animation-delay:0ms!important;transition-duration:1ms!important}}
`

// Logo dosyasını (varsayılan icon.png; sunucu icon-server.png geçer) paketli/dev yerlerinden bulup data URL döner; yoksa ''.
function logoDataUrl(file = 'icon.png') {
  let app = null
  try { app = require('electron').app } catch {}
  const candidates = [
    path.join(process.resourcesPath || '', file),
    path.join(path.dirname(process.execPath), 'resources', file),
    app ? path.join(app.getAppPath(), 'build', file) : '',
    path.join(__dirname, '..', 'build', file),
  ].filter(Boolean)
  for (const candidate of candidates) {
    try { if (fs.existsSync(candidate)) return `data:image/png;base64,${fs.readFileSync(candidate).toString('base64')}` } catch {}
  }
  return ''
}

// Küçük sayfa iskeleti: ortak CSS + pencereye özel CSS + gövde.
function wrapHtml({ title = 'P2', css = '', body = '', script = '' }) {
  return `<!doctype html><html lang="tr"><head><meta charset="utf-8"><title>${title}</title><style>${P2_MAVI_CSS}${css}</style></head><body>${body}${script ? `<script>${script}</script>` : ''}</body></html>`
}

module.exports = { P2_MAVI_CSS, logoDataUrl, wrapHtml }
