import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Titlebar from './Titlebar'
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

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <div className="app-frame">
      <Titlebar />
      <div className="app-content"><App /></div>
    </div>
  </StrictMode>,
)
