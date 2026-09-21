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
      pickFolder(): Promise<{ ok: true; path: string; name: string; remoteHost?: string } | { ok: false; error: string } | null>
      repoSource(payload: { cwd: string }): Promise<{ ok: true; remoteHost?: string } | { ok: false; error: string }>
      clone(payload: { url: string; parent?: string; name?: string; requestId?: string }): Promise<{ ok: true; path: string; name: string; output?: string } | { ok: false; error: string } | null>
      historyStream(payload: { cwd: string; requestId: string; ref?: string }): Promise<{ ok: boolean; error?: string }>
      historyCancel(payload: { cwd: string }): Promise<any>
      operationCancel(payload: { requestId: string }): Promise<any>
      commitOp(payload: { cwd: string; action: 'checkout' | 'cherry-pick' | 'revert' | 'reset' | 'patch' | 'compare'; hash: string; mode?: 'soft' | 'mixed' | 'hard'; requestId?: string }): Promise<any>
      openWith(payload: { cwd: string; path?: string; app: 'explorer' | 'code' | 'default' | 'terminal' }): Promise<any>
      remote(payload: { cwd: string; action: 'add' | 'set-url' | 'rename' | 'remove'; name: string; url?: string; newName?: string }): Promise<any>
      submodule(payload: { cwd: string; action: 'add' | 'update'; url?: string; path?: string }): Promise<any>
      mergeState(payload: { cwd: string }): Promise<any>
      mergeResolve(payload: { cwd: string; path: string; content: string }): Promise<any>
      mergeFinish(payload: { cwd: string; action: 'commit' | 'abort'; message?: string }): Promise<any>
      worktree(payload: { cwd: string; action: 'list' | 'add'; path?: string; branch?: string }): Promise<any>
      onHistoryChunk(handler: (chunk: { requestId: string; rows: any[]; done: boolean; total: number; error?: string }) => void): () => void
      onOperationProgress(handler: (progress: { requestId: string; kind: string; phase: string; label: string; percent: number; current: number; total: number; remaining: number }) => void): () => void
      onCloneProgress(handler: (progress: { requestId: string; phase: string; label: string; percent: number; current: number; total: number; remaining: number }) => void): () => void
      cloneUrl(payload: { url: string; parent?: string; name?: string }): Promise<{ ok: true; path: string; name: string; connection?: any } | { ok: false; error: string; needsApp?: boolean } | null>
      detect(payload: { url: string }): Promise<{ ok: true; host: string; kind: string; connected: boolean } | { ok: false; error: string }>
      ensureAuth(payload: { url: string }): Promise<any>
      info(payload: { cwd: string }): Promise<any>
      status(payload: { cwd: string }): Promise<any>
      history?(payload: { cwd: string; limit?: number; ref?: string }): Promise<any>
      diff(payload: { cwd: string; path: string; staged?: boolean; untracked?: boolean }): Promise<any>
      commitDiff(payload: { cwd: string; hash: string; path?: string }): Promise<any>
      commitFiles(payload: { cwd: string; hash: string }): Promise<any>
      commitTree?(payload: { cwd: string; hash: string }): Promise<any>
      repositorySettings(payload: { cwd: string }): Promise<any>
      saveRepositorySettings(payload: { cwd: string; remoteUrl: string; ignored: string; userName: string; userEmail: string }): Promise<any>
      stage(payload: { cwd: string; paths: string[] }): Promise<any>
      unstage(payload: { cwd: string; paths: string[] }): Promise<any>
      discard(payload: { cwd: string; paths: string[]; untracked?: string[] }): Promise<any>
      ignore(payload: { cwd: string; patterns: string[] }): Promise<any>
      openPath(payload: { cwd: string; path: string }): Promise<any>
      savePatch(payload: { cwd: string; paths: string[]; staged?: boolean }): Promise<any>
      commit(payload: { cwd: string; message: string; amend?: boolean }): Promise<any>
      sync(payload: { cwd: string; action: 'push' | 'pull' | 'fetch'; remote?: string; branch?: string; setUpstream?: boolean; requestId?: string }): Promise<any>
      branch(payload: { cwd: string; action: 'create' | 'checkout' | 'delete' | 'merge' | 'list'; name?: string; from?: string; force?: boolean; requestId?: string }): Promise<any>
      branchOp(payload: { cwd: string; action: 'fast-forward' | 'merge' | 'rebase' | 'rename' | 'track' | 'tag' | 'delete-remote' | 'pull' | 'push'; ref?: string; name?: string; remote?: string; branch?: string }): Promise<any>
      stash(payload: { cwd: string; action: 'push' | 'pop' | 'list'; index?: number; paths?: string[] }): Promise<any>
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
