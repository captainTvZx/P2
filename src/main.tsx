import { lazy, StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Titlebar from './Titlebar'
import Logo from './ui/Logo'
import { applyAppearance } from './theme'
import { getSettings, subscribeSettings } from './settings'
import { startOverlaySettingsSync } from './overlaySettingsSync'
import { activeLocale } from './i18n'
import '@fontsource-variable/inter'
import './p2-theme.css'
import './styles.css'
import './setup.css'
import './titlebar.css'
import './organization.css'
import './motion.css'
import './workspace.css'
import './first-project.css'
import './project-rail.css'
import './channel-content.css'
import './avatars.css'
import './direct.css'
import './settings.css'
import './call.css'
import './board.css'
import './library.css'
import './overlay.css'
import './documents.css'
import './dev-showcase.css'
import './ui.css'

const demoRequested=import.meta.env.DEV&&new URLSearchParams(window.location.search).has('ui-demo')
const overlayRequested=new URLSearchParams(window.location.search).has('overlay')
const DevShowcase=lazy(()=>import('./DevShowcase'))
const OverlayApp=lazy(()=>import('./OverlayApp'))

createRoot(document.getElementById('root')!).render(
  overlayRequested
  ? <Suspense fallback={<div/>}><OverlayApp/></Suspense>
  : <StrictMode>
    <div className="app-frame">
      <Titlebar />
      <div className="app-content">{demoRequested?<Suspense fallback={<main className="splash"><Logo size="xl" pop/><p>UI showroom hazırlanıyor…</p></main>}><DevShowcase/></Suspense>:<App />}</div>
    </div>
  </StrictMode>,
)

// Oyun içi overlay kısayolları ana süreçteki DLL tablosuna ayarlardan akar (yalnız ana pencere).
if(!overlayRequested)startOverlaySettingsSync()
// Görünüm ayarları açılışta ve her değişimde uygulanır.
applyAppearance(getSettings().appearance)
document.documentElement.lang=activeLocale()
subscribeSettings(()=>{
  applyAppearance(getSettings().appearance)
  document.documentElement.lang=activeLocale()
})
