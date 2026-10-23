import Logo from '../../ui/Logo'
import { TOOL_LIST } from '../tools/registry'
import { toggleWindow, useLayout } from '../layoutStore'
import { bridge } from '../overlayBridge'
import { IconCapture, IconClose, IconSettings } from '../icons'
import { useSettings } from '../../settings'
import { formatHotkeyKeys } from '../../hotkeys'

// Sol dikey araç çubuğu (56 px): araç düğmeleri (açık = aktif, sabitli = nokta), altta ekran görüntüsü, ayarlar, kapat.
// İpuçları CSS ile (hover/focus) görünür; kısayolu olan eylemlerde tuş çipi gösterilir.
function Keys({ hotkey }: { hotkey: string }) {
  const keys = formatHotkeyKeys(hotkey)
  if (!keys.length) return null
  return <span className="p2-keys">{keys.map((key, index) => <span key={index}>{index > 0 && <i>+</i>}<kbd>{key}</kbd></span>)}</span>
}

export default function Toolbar({ projectName }: { projectName: string }) {
  const layout = useLayout()
  const settings = useSettings()
  const hotkeys = settings.overlayHotkeys
  return <nav className="ow-toolbar" aria-label="Overlay araçları">
    <div className="ow-toolbar-logo" title={`P2 · ${projectName}`}><Logo size="sm" /><span className="ow-tip"><b>P2 Overlay</b><small>{projectName}</small></span></div>
    <div className="ow-toolbar-tools">
      {TOOL_LIST.filter(tool => tool.id !== 'library').map(tool => {
        const win = layout.windows[tool.id]
        const open = !!win?.open
        const Icon = tool.icon
        const hotkey = tool.id === 'voice' ? hotkeys.mute : tool.id === 'capture' ? hotkeys.screenshot : ''
        return <button key={tool.id} type="button" className={`ow-tool${open ? ' active' : ''}${win?.pinned && open ? ' pinned' : ''}${layout.focus === tool.id && open ? ' focused' : ''}`} aria-pressed={open} aria-label={tool.title} onClick={() => toggleWindow(tool.id)}>
          <Icon size={21} />
          <i className="ow-dot" />
          <span className="ow-tip"><b>{tool.title}</b>{hotkey && <small>{tool.id === 'voice' ? 'Sustur' : 'Çek'} <Keys hotkey={hotkey} /></small>}</span>
        </button>
      })}
    </div>
    <div className="ow-toolbar-bottom">
      <button type="button" className="ow-tool" aria-label="Ekran görüntüsü al" onClick={() => { void bridge.screenshot() }}>
        <IconCapture size={21} /><span className="ow-tip"><b>Ekran görüntüsü al</b>{hotkeys.screenshot && <small><Keys hotkey={hotkeys.screenshot} /></small>}</span>
      </button>
      <button type="button" className="ow-tool" aria-label="Overlay ayarları" onClick={() => toggleWindow('library')}>
        <IconSettings size={21} /><span className="ow-tip"><b>Ayarlar</b><small>P2 penceresinde açılır</small></span>
      </button>
      <button type="button" className="ow-tool ow-tool-close" aria-label="Paneli kapat" onClick={() => bridge.setVisible(false)}>
        <IconClose size={21} /><span className="ow-tip"><b>Paneli kapat</b><small>{hotkeys.panel ? <Keys hotkey={hotkeys.panel} /> : <kbd className="p2-kbd">Esc</kbd>}</small></span>
      </button>
    </div>
  </nav>
}
