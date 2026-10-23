import { describe, expect, it } from 'vitest'
import { CHUNK_SIZE, diffManifests, type Manifest } from './releaseManifest'

const hash = (n: number) => n.toString(16).padStart(64, '0')
const manifest = (files: Array<[string, number, string[]]>): Manifest => ({
  version: 1, chunkSize: CHUNK_SIZE, platform: 'win-x64', executable: 'game.exe',
  files: files.map(([path, size, chunks]) => ({ path, size, chunks })),
  totalSize: files.reduce((sum, file) => sum + file[1], 0), fileCount: files.length, chunkCount: new Set(files.flatMap(file => file[2])).size,
})

describe('diffManifests', () => {
  it('ilk sürümde her dosya yenidir ve tüm baytlar indirilir', () => {
    const diff = diffManifests(null, manifest([['game.exe', 10, [hash(1)]], ['data.pak', 5, [hash(2)]]]))
    expect(diff).toMatchObject({ added: 2, removed: 0, changed: 0, unchanged: 0, addedBytes: 15, downloadBytes: 15, truncated: false })
    expect(diff.files.added.map(file => file.path)).toEqual(['game.exe', 'data.pak'])
  })

  it('değişen, silinen ve aynı dosyaları ayırır; indirme yalnız önceki pakette olmayan parçalardır', () => {
    const previous = manifest([['game.exe', 10, [hash(1)]], ['data.pak', 5, [hash(2)]], ['old.bin', 3, [hash(3)]]])
    // data.pak içeriği değişti (yeni parça), new.txt eklendi ama parçası (hash 2) zaten sunucuda var.
    const next = manifest([['game.exe', 10, [hash(1)]], ['data.pak', 7, [hash(4)]], ['new.txt', 2, [hash(2)]]])
    const diff = diffManifests(previous, next)
    expect(diff).toMatchObject({ added: 1, removed: 1, changed: 1, unchanged: 1, addedBytes: 2, removedBytes: 3, changedBytes: 7, downloadBytes: 7 })
    expect(diff.files.changed).toEqual([{ path: 'data.pak', size: 7, previousSize: 5 }])
    expect(diff.files.removed).toEqual([{ path: 'old.bin', size: 3 }])
  })

  it('aynı parça birden çok dosyada geçse de indirme baytı bir kez sayılır', () => {
    const next = manifest([['a.bin', 4, [hash(9)]], ['b.bin', 4, [hash(9)]]])
    expect(diffManifests(null, next).downloadBytes).toBe(4)
  })

  it('liste sınırı aşılınca truncated işaretlenir, sayaçlar tam kalır', () => {
    const next = manifest([['a', 1, [hash(1)]], ['b', 1, [hash(2)]], ['c', 1, [hash(3)]]])
    const diff = diffManifests(null, next, 1)
    expect(diff.added).toBe(3)
    expect(diff.files.added).toHaveLength(1)
    expect(diff.truncated).toBe(true)
  })
})
