import { describe, expect, it } from 'vitest'
import { canEditNote, nextVersion, noteVisibleTo, normalizeVisibility, versionConflicts } from './notesRules'

describe('noteVisibleTo', () => {
  it('özel not yalnız yazarına görünür', () => {
    expect(noteVisibleTo({ authorId: 'a', visibility: 'PRIVATE' }, 'a')).toBe(true)
    expect(noteVisibleTo({ authorId: 'a', visibility: 'PRIVATE' }, 'b')).toBe(false)
  })
  it('ekip notu herkese görünür', () => {
    expect(noteVisibleTo({ authorId: 'a', visibility: 'TEAM' }, 'b')).toBe(true)
  })
})

describe('canEditNote', () => {
  it('yazar her notu düzenler', () => {
    expect(canEditNote({ authorId: 'a', visibility: 'PRIVATE' }, 'a', false)).toBe(true)
    expect(canEditNote({ authorId: 'a', visibility: 'TEAM' }, 'a', false)).toBe(true)
  })
  it('yönetici yalnız ekip notlarını düzenler; başkasının özel notuna dokunamaz', () => {
    expect(canEditNote({ authorId: 'a', visibility: 'TEAM' }, 'b', true)).toBe(true)
    expect(canEditNote({ authorId: 'a', visibility: 'PRIVATE' }, 'b', true)).toBe(false)
    expect(canEditNote({ authorId: 'a', visibility: 'TEAM' }, 'b', false)).toBe(false)
  })
})

describe('sürüm (iyimser kilit)', () => {
  it('version gönderilmezse çakışma yok', () => {
    expect(versionConflicts(3, undefined)).toBe(false)
    expect(versionConflicts(3, null)).toBe(false)
  })
  it('farklı sürüm çakışır, aynı sürüm geçer', () => {
    expect(versionConflicts(3, 2)).toBe(true)
    expect(versionConflicts(3, 3)).toBe(false)
  })
  it('nextVersion bir artırır ve en az 2 döner', () => {
    expect(nextVersion(1)).toBe(2)
    expect(nextVersion(7)).toBe(8)
    expect(nextVersion(0)).toBe(2)
  })
})

describe('normalizeVisibility', () => {
  it('yalnız PRIVATE/TEAM kabul eder, diğerleri varsayılana düşer', () => {
    expect(normalizeVisibility('TEAM')).toBe('TEAM')
    expect(normalizeVisibility('PRIVATE')).toBe('PRIVATE')
    expect(normalizeVisibility('public')).toBe('PRIVATE')
    expect(normalizeVisibility(undefined, 'TEAM')).toBe('TEAM')
  })
})
