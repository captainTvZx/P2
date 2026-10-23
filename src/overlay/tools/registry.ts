import type { ComponentType, ReactElement } from 'react'
import type { OverlayToolId, OverlayToolProps } from '../overlayTypes'
import { TOOL_META, TOOL_ORDER, type ToolBounds } from './toolMeta'
import { TOOL_ICONS } from '../icons'
import VoiceWindow from './VoiceWindow'
import TeamWindow from './TeamWindow'
import ChatWindow from './ChatWindow'
import TasksWindow from './TasksWindow'
import NotesWindow from './NotesWindow'
import BrowserWindow from './BrowserWindow'
import PerfWindow from './PerfWindow'
import OverviewWindow from './OverviewWindow'
import LibraryWindow from './LibraryWindow'
import CaptureWindow from './CaptureWindow'
import { makePlaceholder } from './Placeholder'

// Araç kaydı: araç çubuğu sırası, başlık, simge, varsayılan dikdörtgen, en küçük boyut, bileşen ve sabitlenebilirlik.
export type ToolDef = {
  id: OverlayToolId
  title: string
  icon: (props: { size?: number; className?: string }) => ReactElement
  defaultBounds: ToolBounds
  minSize: { w: number; h: number }
  component: ComponentType<OverlayToolProps>
  pinnable: boolean
}

const COMPONENTS: Record<OverlayToolId, ComponentType<OverlayToolProps>> = {
  voice: VoiceWindow,
  team: TeamWindow,
  chat: ChatWindow,
  notes: NotesWindow,
  tasks: TasksWindow,
  capture: CaptureWindow,
  summary: OverviewWindow,
  browser: BrowserWindow,
  perf: PerfWindow,
  library: LibraryWindow,
}

export const TOOLS: Record<OverlayToolId, ToolDef> = Object.fromEntries(
  TOOL_ORDER.map(id => [id, { ...TOOL_META[id], icon: TOOL_ICONS[id], component: COMPONENTS[id] }]),
) as Record<OverlayToolId, ToolDef>
export const TOOL_LIST: ToolDef[] = TOOL_ORDER.map(id => TOOLS[id])
export { TOOL_ORDER }
