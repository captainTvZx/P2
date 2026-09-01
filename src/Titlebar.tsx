import { useEffect, useRef, useState } from 'react'
import { showCallPanel, useCallSummary } from './callBus'

function readPageTitle() {
  if (document.querySelector('.setup-page')) return 'Kurulum'
  if (document.querySelector('.auth-page')) return document.querySelector('.auth-card h2')?.textContent?.includes('oluştur') ? 'Hesap Oluştur' : 'Giriş'
  if (document.querySelector('.first-project-screen')) return 'İlk Proje'
  if (document.querySelector('.project-blank')) return document.querySelector('.project-blank')?.getAttribute('data-project-title') || 'Proje'
  if (document.querySelector('.org-page')) return 'Kuruluş'
  if (document.querySelector('.launcher')) return document.querySelector('.workspace > header h2')?.textContent?.trim() || 'Çalışma Alanı'
  return 'P2'
}

export default function Titlebar() {
  const call = useCallSummary()
  const [maximized, setMaximized] = useState(false)
  const [menuOpen, setMenuOpen] = useState(false)
  const [pageTitle, setPageTitle] = useState('P2')
  const menuRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const updateWindow = () => window.p2?.windowControls.isMaximized().then(setMaximized)
    const updateTitle = () => { const title = readPageTitle(); setPageTitle(title); document.title = `${title} · P2` }
    const observer = new MutationObserver(updateTitle)
    observer.observe(document.getElementById('root')!, { childList: true, subtree: true, characterData: true })
    const closeMenu = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false) }
    updateWindow(); updateTitle()
    window.addEventListener('resize', updateWindow)
    document.addEventListener('mousedown', closeMenu)
    return () => { observer.disconnect(); window.removeEventListener('resize', updateWindow); document.removeEventListener('mousedown', closeMenu) }
  }, [])
  return <header className="titlebar" onDoubleClick={() => window.p2?.windowControls.toggleMaximize()}>
    <div className="titlebar-navigation" ref={menuRef} onDoubleClick={event => event.stopPropagation()}>
      <button className={`menu-trigger${menuOpen ? ' open' : ''}${call.active ? ' in-call' : ''}`} aria-label="Uygulama menüsü" title="Menü" onClick={() => setMenuOpen(open => !open)}><i/><i/><i/></button>
      <span className="page-title">{pageTitle}</span>
      {menuOpen && <div className="app-menu"><header><b>P2 Desktop</b><small>v{window.p2?.version || '0.2.1'}</small></header>{call.active && <button className="menu-call" onClick={() => { setMenuOpen(false); showCallPanel() }}><i/>{call.label} · {call.participants} kişi</button>}<button onClick={() => { setMenuOpen(false); window.location.reload() }}>Sayfayı yenile</button><button className="menu-quit" onClick={() => window.p2?.windowControls.close()}>Uygulamadan çık</button></div>}
    </div>
    <div className="window-controls" onDoubleClick={event => event.stopPropagation()}>
      <button aria-label="Küçült" title="Küçült" onClick={() => window.p2?.windowControls.minimize()}><i className="minimize-icon" /></button>
      <button aria-label={maximized ? 'Geri yükle' : 'Büyüt'} title={maximized ? 'Geri yükle' : 'Büyüt'} onClick={() => window.p2?.windowControls.toggleMaximize()}><i className={maximized ? 'restore-icon' : 'maximize-icon'} /></button>
      <button className="close-window" aria-label="Kapat" title="Kapat" onClick={() => window.p2?.windowControls.close()}><i className="close-icon" /></button>
    </div>
  </header>
}
