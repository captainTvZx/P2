import { lazy, StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Titlebar from './Titlebar'
import { applyAppearance } from './theme'
import { getSettings, subscribeSettings } from './settings'
import { activeLocale } from './i18n'
import './theme-base.css'
import './styles.css'
import './setup.css'
import './titlebar.css'
import './organization.css'
import './interactions.css'
import './motion.css'
import './workspace.css'
import './first-project.css'
import './flat-buttons.css'
import './matte-theme.css'
import './project-rail.css'
import './channel-content.css'
import './avatars.css'
import './direct.css'
import './settings.css'
import './call.css'
import './git.css'
import './board.css'
import './dev-showcase.css'

const demoRequested=import.meta.env.DEV&&new URLSearchParams(window.location.search).has('ui-demo')
const DevShowcase=lazy(()=>import('./DevShowcase'))

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <div className="app-frame">
      <Titlebar />
      <div className="app-content">{demoRequested?<Suspense fallback={<main className="splash"><span className="logo">P2</span><p>UI showroom hazırlanıyor…</p></main>}><DevShowcase/></Suspense>:<App />}</div>
    </div>
  </StrictMode>,
)

// Görünüm ayarları açılışta ve her değişimde uygulanır.
applyAppearance(getSettings().appearance)
document.documentElement.lang=activeLocale()
subscribeSettings(()=>{
  applyAppearance(getSettings().appearance)
  document.documentElement.lang=activeLocale()
})
