/// <reference types="vite/client" />

interface Window {
  p2?: {
    platform: string
    version: string
    apiUrl: string
    suggestedServer?: string
    session: { save(token: string): Promise<boolean>; load(): Promise<string | null>; clear(): Promise<boolean> }
    database: {
      status(): Promise<boolean>
      install(config: { host: string; port: number; username: string; password: string; database: string; ssl: boolean }): Promise<{ ok: boolean; message?: string }>
    }
    files?: { saveAs(file: { url: string; name: string }): Promise<boolean> }
    systemAudio?: {
      start(): Promise<{ ok: boolean; message?: string }>
      stop(): void
      onChunk(handler: (chunk: { sampleRate: number; samples: Float32Array }) => void): () => void
    }
    overlay?: {
      show(sourceId: string): void
      hide(): void
      paint(message: Record<string, unknown>): void
    }
    screen?: {
      sources(): Promise<{ id: string; name: string; kind: 'screen' | 'window'; thumbnail: string; icon: string }[]>
      select(choice: { sourceId: string; audio: boolean }): void
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
      setFullScreen?(on: boolean): void
      setCloseAction?(action: 'ask' | 'tray' | 'minimize' | 'quit'): void
      onCloseAction?(handler: (action: 'tray' | 'minimize' | 'quit') => void): () => void
      openPopout?(query: string, title?: string): void
    }
    call?: {
      update(summary: { active: boolean; label: string; participants: number; muted: boolean; deafened: boolean; startedAt: number | null; connectedAt: number | null; ringing: boolean }): void
      onWidgetAction(handler: (action: 'mute' | 'deafen' | 'leave' | 'open') => void): () => void
    }
  }
}
