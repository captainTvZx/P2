import type { ReactElement, ReactNode } from 'react'
import type { OverlayToolId } from './overlayTypes'

// Overlay simgeleri: 24×24, çizgi stili (stroke=currentColor). Araç çubuğu ve pencere başlıkları kullanır.
type IconProps = { size?: number; className?: string }
function Svg({ size = 20, className, children }: IconProps & { children: ReactNode }) {
  return <svg className={className} width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>
}

export const IconMic = (p: IconProps) => <Svg {...p}><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3M9 21h6"/></Svg>
export const IconTeam = (p: IconProps) => <Svg {...p}><circle cx="9" cy="8" r="3.2"/><path d="M3.5 19a5.5 5.5 0 0 1 11 0"/><circle cx="17" cy="9" r="2.4"/><path d="M15.5 14.2a4.6 4.6 0 0 1 5 4.3"/></Svg>
export const IconChat = (p: IconProps) => <Svg {...p}><path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v7a2.5 2.5 0 0 1-2.5 2.5H10l-4.5 3.5V16A2.5 2.5 0 0 1 4 13.5z"/><path d="M8 9h8M8 12.5h5"/></Svg>
export const IconNotes = (p: IconProps) => <Svg {...p}><path d="M6 3.5h9l4 4V20a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1z"/><path d="M14.5 3.5V8H19M8.5 12h7M8.5 15.5h7"/></Svg>
export const IconTasks = (p: IconProps) => <Svg {...p}><rect x="4" y="4" width="16" height="16" rx="3"/><path d="m8 12.2 2.6 2.6L16.5 9"/></Svg>
export const IconCapture = (p: IconProps) => <Svg {...p}><path d="M4 8.5A1.5 1.5 0 0 1 5.5 7H8l1.4-2h5.2L16 7h2.5A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"/><circle cx="12" cy="13" r="3.4"/></Svg>
export const IconGallery = (p: IconProps) => <Svg {...p}><rect x="3.5" y="5" width="17" height="14" rx="2.5"/><path d="m3.5 16 4.6-4.6a1 1 0 0 1 1.4 0L13 15l2.3-2.3a1 1 0 0 1 1.4 0L20.5 16"/><circle cx="15.5" cy="9" r="1.4"/></Svg>
export const IconSummary = (p: IconProps) => <Svg {...p}><rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8 15.5v-3M12 15.5v-6M16 15.5v-4.5"/></Svg>
export const IconBrowser = (p: IconProps) => <Svg {...p}><circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.6 2.6 3.9 5.4 3.9 8.5s-1.3 5.9-3.9 8.5c-2.6-2.6-3.9-5.4-3.9-8.5s1.3-5.9 3.9-8.5z"/></Svg>
export const IconPerf = (p: IconProps) => <Svg {...p}><path d="M4 16.5a8 8 0 1 1 16 0"/><path d="m12 15.5 4-5"/><circle cx="12" cy="16" r="1.4" fill="currentColor" stroke="none"/><path d="M4 19.5h16"/></Svg>
export const IconLibrary = (p: IconProps) => <Svg {...p}><rect x="4" y="4" width="7" height="7" rx="1.6"/><rect x="13" y="4" width="7" height="7" rx="1.6"/><rect x="4" y="13" width="7" height="7" rx="1.6"/><rect x="13" y="13" width="7" height="7" rx="1.6"/></Svg>

export const IconPin = (p: IconProps) => <Svg {...p}><path d="M14.5 3.5 20.5 9.5l-2.2.6-3.4 3.4.3 3.6-2 2-4.3-4.3L4 19.9l-.1-.1 4.9-4.9-4.3-4.3 2-2 3.6.3 3.4-3.4z"/></Svg>
export const IconMinimize = (p: IconProps) => <Svg {...p}><path d="M6 17h12"/></Svg>
export const IconMaximize = (p: IconProps) => <Svg {...p}><rect x="5" y="5" width="14" height="14" rx="2"/></Svg>
export const IconRestore = (p: IconProps) => <Svg {...p}><rect x="4" y="8" width="12" height="12" rx="2"/><path d="M8 8V6a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2"/></Svg>
export const IconClose = (p: IconProps) => <Svg {...p}><path d="m6 6 12 12M18 6 6 18"/></Svg>
export const IconSettings = (p: IconProps) => <Svg {...p}><circle cx="12" cy="12" r="3"/><path d="M12 3.5v2.3M12 18.2v2.3M3.5 12h2.3M18.2 12h2.3M6 6l1.6 1.6M16.4 16.4 18 18M6 18l1.6-1.6M16.4 7.6 18 6"/></Svg>
export const IconPower = (p: IconProps) => <Svg {...p}><path d="M12 3.5v8"/><path d="M7 6.5a8 8 0 1 0 10 0"/></Svg>

export const TOOL_ICONS: Record<OverlayToolId, (p: IconProps) => ReactElement> = {
  voice: IconMic, team: IconTeam, chat: IconChat, notes: IconNotes, tasks: IconTasks,
  capture: IconGallery, summary: IconSummary, browser: IconBrowser, perf: IconPerf, library: IconSettings,
}
