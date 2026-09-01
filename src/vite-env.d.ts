/// <reference types="vite/client" />

interface Window {
  p2?: {
    platform: string
    version: string
    apiUrl: string
    session: { save(token: string): Promise<boolean>; load(): Promise<string | null>; clear(): Promise<boolean> }
    database: {
      status(): Promise<boolean>
      install(config: { host: string; port: number; username: string; password: string; database: string; ssl: boolean }): Promise<{ ok: boolean; message?: string }>
    }
    notifications: {
      show(payload: { title: string; body: string; data?: { userId?: string; projectId?: string; channelId?: string } }): Promise<boolean>
      onActivate(handler: (data: { userId?: string; projectId?: string; channelId?: string } | null) => void): () => void
    }
    windowControls: {
      minimize(): void
      toggleMaximize(): void
      close(): void
      isMaximized(): Promise<boolean>
      setCloseAction?(action: 'ask' | 'tray' | 'minimize' | 'quit'): void
      onCloseAction?(handler: (action: 'tray' | 'minimize' | 'quit') => void): () => void
    }
    call?: {
      update(summary: { active: boolean; label: string; participants: number; muted: boolean; startedAt: number | null; connectedAt: number | null; ringing: boolean }): void
      onWidgetAction(handler: (action: 'mute' | 'leave' | 'open') => void): () => void
    }
  }
}
