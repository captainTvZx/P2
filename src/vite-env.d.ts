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
    git?: {
      pickFolder(): Promise<{ ok: true; path: string; name: string } | { ok: false; error: string } | null>
      clone(payload: { url: string; parent?: string; name?: string }): Promise<{ ok: true; path: string; name: string; output?: string } | { ok: false; error: string } | null>
      info(payload: { cwd: string }): Promise<any>
      status(payload: { cwd: string }): Promise<any>
      diff(payload: { cwd: string; path: string; staged?: boolean; untracked?: boolean }): Promise<any>
      commitDiff(payload: { cwd: string; hash: string }): Promise<any>
      stage(payload: { cwd: string; paths: string[] }): Promise<any>
      unstage(payload: { cwd: string; paths: string[] }): Promise<any>
      discard(payload: { cwd: string; paths: string[]; untracked?: string[] }): Promise<any>
      commit(payload: { cwd: string; message: string; amend?: boolean }): Promise<any>
      sync(payload: { cwd: string; action: 'push' | 'pull' | 'fetch'; remote?: string; branch?: string; setUpstream?: boolean }): Promise<any>
      branch(payload: { cwd: string; action: 'create' | 'checkout' | 'delete' | 'list'; name?: string; from?: string; force?: boolean }): Promise<any>
      stash(payload: { cwd: string; action: 'push' | 'pop' | 'list'; index?: number }): Promise<any>
      lfs(payload: { cwd: string; action: 'install' | 'track' | 'untrack' | 'status'; pattern?: string }): Promise<any>
      reveal(payload: { cwd: string; path?: string }): Promise<any>
      connections(): Promise<any[]>
      oauthApps(): Promise<any[]>
      saveApp(payload: { kind: string; host: string; clientId: string }): Promise<any>
      login(payload: { kind: string; host: string }): Promise<any>
      deviceStart(payload: { kind: string; host: string }): Promise<any>
      devicePoll(payload: { ticket: string }): Promise<any>
      connectToken(payload: { kind: string; host: string; token: string }): Promise<any>
      disconnect(payload: { id: string }): Promise<any>
      remoteRepos(payload: { id: string }): Promise<any>
    }
    call?: {
      update(summary: { active: boolean; label: string; participants: number; muted: boolean; deafened: boolean; startedAt: number | null; connectedAt: number | null; ringing: boolean }): void
      onWidgetAction(handler: (action: 'mute' | 'deafen' | 'leave' | 'open') => void): () => void
    }
  }
}
