import type { OverlayToolId } from '../overlayTypes'

// Araç pencerelerinin bileşen gerektirmeyen tanımı (düzen deposu bunu kullanır; bileşenler registry.ts'te).
// defaultBounds 1920×1080 referans görünüm alanına göredir; layoutStore gerçek ekrana oranlar.
export type ToolBounds = { x: number; y: number; w: number; h: number }
// ghostCompact: panel kapalıyken hayalet pencere içeriğe göre kısalır (ses: yalnız katılımcı listesi).
export type ToolMeta = { id: OverlayToolId; title: string; defaultBounds: ToolBounds; minSize: { w: number; h: number }; pinnable: boolean; ghostCompact?: boolean }

export const REFERENCE_VIEWPORT = { w: 1920, h: 1080 }
export const TOOLBAR_W = 56

export const TOOL_ORDER: OverlayToolId[] = ['voice', 'team', 'chat', 'notes', 'tasks', 'capture', 'summary', 'browser', 'perf', 'library']

export const TOOL_META: Record<OverlayToolId, ToolMeta> = {
  voice:   { id: 'voice',   title: 'Sesli oda',       defaultBounds: { x: 1540, y: 24,  w: 356, h: 420 }, minSize: { w: 260, h: 200 }, pinnable: true, ghostCompact: true },
  team:    { id: 'team',    title: 'Ekip',            defaultBounds: { x: 1540, y: 468, w: 356, h: 420 }, minSize: { w: 260, h: 200 }, pinnable: true },
  chat:    { id: 'chat',    title: 'Sohbet',          defaultBounds: { x: 420,  y: 80,  w: 720, h: 540 }, minSize: { w: 360, h: 260 }, pinnable: true },
  notes:   { id: 'notes',   title: 'Notlar',          defaultBounds: { x: 1100, y: 120, w: 420, h: 520 }, minSize: { w: 280, h: 220 }, pinnable: true },
  tasks:   { id: 'tasks',   title: 'Görevler',        defaultBounds: { x: 300,  y: 120, w: 640, h: 480 }, minSize: { w: 360, h: 260 }, pinnable: true },
  capture: { id: 'capture', title: 'Ekran görüntüsü', defaultBounds: { x: 360,  y: 140, w: 640, h: 460 }, minSize: { w: 320, h: 240 }, pinnable: true },
  summary: { id: 'summary', title: 'Oyun özeti',      defaultBounds: { x: 80,   y: 60,  w: 420, h: 560 }, minSize: { w: 300, h: 240 }, pinnable: true },
  browser: { id: 'browser', title: 'Tarayıcı',        defaultBounds: { x: 500,  y: 80,  w: 820, h: 560 }, minSize: { w: 420, h: 300 }, pinnable: false },
  perf:    { id: 'perf',    title: 'Performans',      defaultBounds: { x: 80,   y: 560, w: 400, h: 500 }, minSize: { w: 260, h: 200 }, pinnable: true },
  library: { id: 'library', title: 'Ayarlar',         defaultBounds: { x: 80,   y: 60,  w: 380, h: 440 }, minSize: { w: 280, h: 220 }, pinnable: false },
}
