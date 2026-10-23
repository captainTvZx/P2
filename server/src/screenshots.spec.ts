import { describe, expect, it } from 'vitest'
import { MAX_CAPTION, canManageScreenshot, clampDimension, normalizeCaption, normalizeTakenAt, uploadFileId, uploadOwnedImage, validUploadPath } from './screenshotRules'

describe('validUploadPath', () => {
  const id = '3f2a1c9e-1b2c-4d5e-8f90-123456789abc'
  it('yalnız /uploads/<uuid>.(png|jpg|jpeg|webp) kabul eder', () => {
    expect(validUploadPath(`/uploads/${id}.jpg`)).toBe(true)
    expect(validUploadPath(`/uploads/${id}.jpeg`)).toBe(true)
    expect(validUploadPath(`/uploads/${id}.PNG`)).toBe(true)
    expect(validUploadPath(`/uploads/${id}.webp`)).toBe(true)
  })
  it('başka türleri, tam adresleri ve yol kaçışlarını reddeder', () => {
    expect(validUploadPath(`/uploads/${id}.gif`)).toBe(false)
    expect(validUploadPath(`/uploads/${id}.jpg.json`)).toBe(false)
    expect(validUploadPath(`http://x/uploads/${id}.jpg`)).toBe(false)
    expect(validUploadPath('/uploads/../x.jpg')).toBe(false)
    expect(validUploadPath('')).toBe(false)
    expect(validUploadPath(undefined)).toBe(false)
  })
  it('uploadFileId diskteki dosya adını verir', () => {
    expect(uploadFileId(`/uploads/${id}.jpg`)).toBe(`${id}.jpg`)
  })
})

describe('uploadOwnedImage', () => {
  it('meta sahibi ve görsel türü eşleşmeli', () => {
    expect(uploadOwnedImage({ userId: 'u1', type: 'image/jpeg' }, 'u1')).toBe(true)
    expect(uploadOwnedImage({ userId: 'u1', type: 'image/png' }, 'u1')).toBe(true)
    expect(uploadOwnedImage({ userId: 'u2', type: 'image/jpeg' }, 'u1')).toBe(false)
    expect(uploadOwnedImage({ userId: 'u1', type: 'application/pdf' }, 'u1')).toBe(false)
    expect(uploadOwnedImage(null, 'u1')).toBe(false)
  })
})

describe('canManageScreenshot', () => {
  it('sahip her zaman, yönetici başkalarınınkini de', () => {
    expect(canManageScreenshot({ userId: 'a' }, 'a', false)).toBe(true)
    expect(canManageScreenshot({ userId: 'a' }, 'b', true)).toBe(true)
    expect(canManageScreenshot({ userId: 'a' }, 'b', false)).toBe(false)
  })
})

describe('normalizeCaption', () => {
  it('kırpar, satır sonlarını tek boşluğa indirir, boşu null yapar', () => {
    expect(normalizeCaption('  Boss  \n kapısı ')).toBe('Boss kapısı')
    expect(normalizeCaption('   ')).toBeNull()
    expect(normalizeCaption(undefined)).toBeNull()
    expect(normalizeCaption(null)).toBeNull()
  })
  it('uzunluğu sınırlar', () => {
    expect(normalizeCaption('x'.repeat(MAX_CAPTION + 50))?.length).toBe(MAX_CAPTION)
  })
})

describe('clampDimension', () => {
  it('1..7680 tam sayı, aksi hâlde null', () => {
    expect(clampDimension(1920)).toBe(1920)
    expect(clampDimension('2160')).toBe(2160)
    expect(clampDimension(1079.9)).toBe(1079)
    expect(clampDimension(0)).toBeNull()
    expect(clampDimension(7681)).toBeNull()
    expect(clampDimension('abc')).toBeNull()
  })
})

describe('normalizeTakenAt', () => {
  const now = Date.parse('2026-10-23T12:00:00Z')
  it('geçerli ve yakın tarihi korur', () => {
    expect(normalizeTakenAt('2026-10-23T11:58:00Z', now).toISOString()).toBe('2026-10-23T11:58:00.000Z')
  })
  it('geçersiz, gelecekteki ya da çok eski tarihte şimdiyi döner', () => {
    expect(normalizeTakenAt('bozuk', now).getTime()).toBe(now)
    expect(normalizeTakenAt('2026-10-23T13:00:00Z', now).getTime()).toBe(now)
    expect(normalizeTakenAt('2020-01-01T00:00:00Z', now).getTime()).toBe(now)
    expect(normalizeTakenAt(undefined, now).getTime()).toBe(now)
  })
})
