// Proje notları için saf kurallar (vitest ile test edilir; Prisma/Nest bağımlılığı yok).
// PRIVATE not yalnız yazarına görünür; TEAM not proje erişimi olan herkese.
// Düzenle/sil: yazar her zaman; TEAM notlarda ayrıca proje yöneticisi (channel.manage vekili).

export type NoteVisibility = 'PRIVATE' | 'TEAM'
export type NoteRuleInput = { authorId: string; visibility: string }

export function normalizeVisibility(value: unknown, fallback: NoteVisibility = 'PRIVATE'): NoteVisibility {
  return value === 'TEAM' ? 'TEAM' : value === 'PRIVATE' ? 'PRIVATE' : fallback
}

export function noteVisibleTo(note: NoteRuleInput, userId: string) {
  return note.visibility === 'TEAM' || note.authorId === userId
}

export function canEditNote(note: NoteRuleInput, userId: string, canManage: boolean) {
  if (note.authorId === userId) return true
  return note.visibility === 'TEAM' && canManage
}

// İyimser kilit: istemci `version` gönderdiyse sunucudaki ile aynı olmalı. Göndermezse (eski istemci,
// sabitle/görünürlük gibi tek alanlı düğmeler) çakışma denetimi atlanır.
export function versionConflicts(current: number, sent: number | undefined | null) {
  if (sent === undefined || sent === null) return false
  return Number(sent) !== current
}

export function nextVersion(current: number) {
  return Math.max(1, Math.floor(current)) + 1
}
