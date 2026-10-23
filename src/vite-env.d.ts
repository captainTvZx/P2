/// <reference types="vite/client" />

type UpdateState = { status: 'idle' | 'checking' | 'downloading' | 'ready'; version: string; percent: number; error: string; feed?: string; current?: string; startup?: boolean; text?: string }

interface Window {
  p2?: {
    platform: string
    version: string
    apiUrl: string
    suggestedServer?: string
    session: { save(token: string): Promise<boolean>; load(): Promise<string | null>; clear(): Promise<boolean> }
    database: {
      status(): Promise<boolean>
      install(config: { host: string; port: number; username: string; password: string; database: string; ssl: boolean; installService?: boolean }): Promise<{ ok: boolean; message?: string }>
      reset?(): Promise<boolean>
      serverStatus?(): Promise<{ alive: boolean; configured: boolean; openAtLogin: boolean; port: number }>
      serverStart?(): Promise<{ ok: boolean }>
      serverLoginItem?(enabled: boolean): Promise<boolean>
      serviceStatus?(): Promise<{ registered: boolean; alive: boolean; script: string }>
      installService?(): Promise<{ ok: boolean }>
      serverLog?(): Promise<string>
      serverWait?(): Promise<{ ok: boolean; reason?: string; configured?: boolean; log?: string }>
      openServerFolder?(): Promise<boolean>
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
      onRestored?(handler: () => void): () => void
      minimize(): void
      toggleMaximize(): void
      close(): void
      isMaximized(): Promise<boolean>
      setFullScreen?(on: boolean): void
      setCloseAction?(action: 'ask' | 'tray' | 'minimize' | 'quit'): void
      onCloseAction?(handler: (action: 'tray' | 'minimize' | 'quit') => void): () => void
      openPopout?(query: string, title?: string): void
    }
    updates?: {
      configure(apiBase: string): Promise<{ ok: boolean; reason?: string; feed?: string }>
      check(): Promise<{ ok: boolean; status?: string }>
      install(): Promise<{ ok: boolean }>
      state(): Promise<UpdateState>
      onState(handler: (state: UpdateState) => void): () => void
    }
    media?: {
      openVideo(id: string): Promise<boolean>
    }
    library?: {
      configure(payload: { apiBase: string; accessToken: string; refreshToken: string }): Promise<{ ok: boolean }>
      state(): Promise<LibraryState>
      chooseRoot(): Promise<{ ok: boolean; root?: string; free?: number; error?: string }>
      setChannel(payload: { projectId: string; channel: string }): Promise<{ ok: boolean }>
      setLimit(payload: { mbps: number }): Promise<{ ok: boolean }>
      setArgs(payload: { projectId: string; userArgs: string }): Promise<{ ok: boolean }>
      install(payload: { organizationId: string; projectId: string; projectName: string; releaseId: string; channel: string; pinned?: boolean }): Promise<{ ok: boolean; error?: string; code?: string }>
      pause(payload: { projectId: string }): Promise<{ ok: boolean }>
      resume(payload: { projectId: string }): Promise<{ ok: boolean }>
      cancel(payload: { projectId: string }): Promise<{ ok: boolean }>
      launch(payload: { organizationId: string; projectId: string; projectName: string; overlay?: boolean; iconUrl?: string }): Promise<{ ok: boolean; error?: string }>
      stop(payload: { projectId: string }): Promise<{ ok: boolean; error?: string }>
      uninstall(payload: { organizationId: string; projectId: string; projectName: string }): Promise<{ ok: boolean; error?: string }>
      verify(payload: { organizationId: string; projectId: string; projectName: string }): Promise<{ ok: boolean; repaired?: number }>
      openFolder(payload: { projectId: string; projectName: string }): Promise<{ ok: boolean }>
      createShortcut(payload: { projectId: string; projectName: string; iconUrl?: string }): Promise<{ ok: boolean; error?: string }>
      pickBuildFolder(): Promise<{ ok: true; path: string; executables: string[]; fileCount: number; totalSize: number; warnings: string[] } | { ok: false; error?: string } | null>
      upload(payload: { requestId: string; organizationId: string; projectId: string; projectName: string; sourceDir: string; executable: string; launchArgs?: string; channel: string; version: string; notes?: string; overlay?: boolean }): Promise<{ ok: boolean; error?: string; code?: string }>
      uploadCancel(payload: { requestId: string; releaseId?: string; organizationId?: string; projectId?: string; discard?: boolean }): Promise<{ ok: boolean }>
      uploads(): Promise<Array<{ releaseId: string; projectId: string; projectName: string; channel: string; version: string; pending: number; total: number }>>
      uploadResume(payload: { requestId: string; releaseId: string }): Promise<{ ok: boolean; error?: string }>
      uploadForget(payload: { releaseId: string }): Promise<{ ok: boolean }>
      onEvent(handler: (event: LibraryEvent) => void): () => void
      onPlay(handler: (payload: { projectId: string }) => void): () => void
    }
    call?: {
      update(summary: { active: boolean; label: string; participants: number; muted: boolean; deafened: boolean; startedAt: number | null; connectedAt: number | null; ringing: boolean }): void
      onWidgetAction(handler: (action: 'mute' | 'deafen' | 'leave' | 'open' | 'accept' | 'decline') => void): () => void
      widgetVisible?(): Promise<boolean>
      setWidgetVisible?(visible: boolean): Promise<boolean>
    }
    // Oyun içi overlay köprüsü (ana pencere tarafı); sözleşme src/overlay/overlayTypes.ts
    gameOverlay?: import('./overlay/overlayTypes').GameOverlayApi
  }
  // Oyun içi overlay penceresinin kendi köprüsü (electron/overlay-ui-preload.cjs)
  p2overlay?: import('./overlay/overlayTypes').P2OverlayBridge
}

type LibraryInstalled = { folder: string; channel: string; releaseId: string; version: string; installedAt: number; pinned: boolean; executable: string; launchArgs: string; userArgs?: string; playSeconds?: number; lastPlayedAt?: number }
type LibraryJob = { projectId: string; releaseId: string; active: boolean; phase: 'preparing' | 'downloading' | 'waiting-for-exit' | 'installing' | 'verifying' | 'paused' | 'done' | 'failed' | 'canceled'; done: number; total: number; bytesDone: number; bytesTotal: number; bytesWritten?: number; bytesToWrite?: number; speed?: number; diskSpeed?: number; eta?: number; error?: string; code?: string; paused?: boolean }
type LibraryState = { root: string; deviceId: string; projects: Record<string, LibraryInstalled>; jobs: Record<string, LibraryJob>; running: string[] }
type LibraryEvent =
  | ({ type: 'job' } & LibraryJob)
  | { type: 'upload'; requestId: string; releaseId?: string; projectId: string; channel: string; version: string; phase: string; percent?: number; done?: number; total?: number; bytesDone?: number; bytesTotal?: number; error?: string; code?: string }
  | { type: 'running'; projectId: string; running: boolean }
  | { type: 'played'; projectId: string; seconds: number }
  | { type: 'crash'; projectId: string; projectName: string; code: number; seconds: number; log?: string | null }
  | { type: 'auth' | 'auth-required' | 'state' }
