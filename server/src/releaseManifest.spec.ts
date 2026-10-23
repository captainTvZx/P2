import { describe, expect, it } from 'vitest'
import { createRequire } from 'node:module'
import { safeRelativePath, validateManifestInput, sameManifest, diffManifests, CHUNK_SIZE, type Manifest } from './releaseManifest'

const require = createRequire(import.meta.url)
const client = require('../../electron/patcherManifest.cjs')

const hash = (n: number) => String(n).padStart(64, '0').replace(/[^0-9a-f]/g, '0')

describe('safeRelativePath', () => {
  const bad = ['', '/etc/passwd', 'C:/Windows', '\\\\server\\share', '../secret', 'a/../b', './x', 'a/b/', 'con', 'PRN.txt', 'file.', 'name ', 'x\u0000y', 'a<b', 'q?']
  for (const value of bad) it(`rejects ${JSON.stringify(value)}`, () => expect(safeRelativePath(value)).toBeNull())
  it('accepts nested path and normalizes backslashes', () => expect(safeRelativePath('Game\\Bin\\app.exe')).toBe('Game/Bin/app.exe'))
  it('rejects depth over 64', () => expect(safeRelativePath(Array(66).fill('a').join('/'))).toBeNull())
  it('agrees with the client copy on every fixture', () => {
    for (const value of [...bad, 'Game/Bin/app.exe', 'a/b/c.pak', 'Game\\x.dll']) expect(client.safeRelativePath(value)).toBe(safeRelativePath(value))
  })
})

describe('validateManifestInput', () => {
  const file = (path: string, size: number, chunks: string[]) => ({ path, size, chunks })
  it('builds a manifest and unique hash list', () => {
    const result = validateManifestInput({ executable: 'Game.exe', files: [file('Game.exe', 10, [hash(1)]), file('data.pak', CHUNK_SIZE + 5, [hash(2), hash(3)]), file('shared.bin', 3, [hash(1)])] })
    expect('manifest' in result).toBe(true)
    if ('manifest' in result) { expect(result.manifest.fileCount).toBe(3); expect(result.manifest.chunkCount).toBe(3); expect(result.uniqueHashes.sort()).toEqual([hash(1), hash(2), hash(3)].sort()) }
  })
  it('rejects wrong chunk count for size', () => { const r = validateManifestInput({ executable: 'Game.exe', files: [file('Game.exe', CHUNK_SIZE + 1, [hash(1)])] }); expect('error' in r && r.code).toBe('MANIFEST_CHUNKS') })
  it('rejects executable not in files', () => { const r = validateManifestInput({ executable: 'Missing.exe', files: [file('Game.exe', 1, [hash(1)])] }); expect('error' in r && r.code).toBe('MANIFEST_EXECUTABLE_MISSING') })
  it('rejects non-exe executable', () => { const r = validateManifestInput({ executable: 'Game.bin', files: [file('Game.bin', 1, [hash(1)])] }); expect('error' in r && r.code).toBe('MANIFEST_EXECUTABLE') })
  it('rejects duplicate paths case-insensitively', () => { const r = validateManifestInput({ executable: 'Game.exe', files: [file('Game.exe', 1, [hash(1)]), file('game.EXE', 1, [hash(2)])] }); expect('error' in r && r.code).toBe('MANIFEST_DUPLICATE') })
  it('rejects bad hashes', () => { const r = validateManifestInput({ executable: 'Game.exe', files: [file('Game.exe', 1, ['nothex'])] }); expect('error' in r && r.code).toBe('MANIFEST_HASH') })
  it('handles empty files (no chunks)', () => { const r = validateManifestInput({ executable: 'Game.exe', files: [file('Game.exe', 1, [hash(1)]), file('empty.txt', 0, [])] }); expect('manifest' in r).toBe(true) })
})

describe('planInstall (client)', () => {
  const m = (files: { path: string; size: number; chunks: string[] }[]): Manifest => ({ version: 1, chunkSize: CHUNK_SIZE, platform: 'win-x64', executable: 'Game.exe', files, totalSize: files.reduce((s, f) => s + f.size, 0), fileCount: files.length, chunkCount: new Set(files.flatMap(f => f.chunks)).size })
  it('fresh install downloads everything', () => {
    const plan = client.planInstall(null, m([{ path: 'Game.exe', size: 5, chunks: [hash(1)] }, { path: 'a.pak', size: 5, chunks: [hash(2)] }]))
    expect(plan.downloads.sort()).toEqual([hash(1), hash(2)].sort())
    expect(plan.builds).toHaveLength(2)
  })
  it('skips unchanged files', () => {
    const installed = m([{ path: 'Game.exe', size: 5, chunks: [hash(1)] }])
    const plan = client.planInstall(installed, installed)
    expect(plan.downloads).toHaveLength(0); expect(plan.builds).toHaveLength(0)
  })
  it('one changed chunk downloads one, reuses the rest locally', () => {
    const installed = m([{ path: 'big', size: CHUNK_SIZE * 2, chunks: [hash(1), hash(2)] }])
    const target = m([{ path: 'big', size: CHUNK_SIZE * 2, chunks: [hash(1), hash(9)] }])
    const plan = client.planInstall(installed, target)
    expect(plan.downloads).toEqual([hash(9)])
    const build = plan.builds.find((b: any) => b.path === 'big')
    expect(build.chunks[0].from).toBe('self')
    expect(build.chunks[1].from).toBe('net')
  })
  it('reuses moved content across paths', () => {
    const installed = m([{ path: 'old', size: 5, chunks: [hash(7)] }])
    const target = m([{ path: 'new', size: 5, chunks: [hash(7)] }])
    const plan = client.planInstall(installed, target)
    expect(plan.downloads).toHaveLength(0)
    expect(plan.builds[0].chunks[0].from).toBe('local')
    expect(plan.deletes).toEqual(['old'])
  })
})

describe('içerik tanımlı parçalar (lengths)', () => {
  const h = (n: number) => n.toString(16).padStart(64, '0')
  const base = { executable: 'Game.exe' }
  it('lengths toplamı boyuta eşit ve her parça 1..4 MB olmalı', () => {
    const ok = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_000, chunks: [h(1), h(2)], lengths: [3_000_000, 2_000_000] }] })
    expect('manifest' in ok).toBe(true)
    if ('manifest' in ok) { expect(ok.manifest.files[0].lengths).toEqual([3_000_000, 2_000_000]); expect(ok.manifest.chunkCount).toBe(2) }
    const badSum = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_000, chunks: [h(1), h(2)], lengths: [3_000_000, 1_000_000] }] })
    expect('error' in badSum && badSum.code).toBe('MANIFEST_CHUNKS')
    const tooBig = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_000, chunks: [h(1)], lengths: [5_000_000] }] })
    expect('error' in tooBig && tooBig.code).toBe('MANIFEST_CHUNKS')
    const mismatch = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_000, chunks: [h(1), h(2)], lengths: [5_000_000] }] })
    expect('error' in mismatch && mismatch.code).toBe('MANIFEST_CHUNKS')
  })
  it('lengths yokken eski sabit düzen geçerliliğini korur', () => {
    const ok = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_000, chunks: [h(1), h(2)] }] })
    expect('manifest' in ok).toBe(true)
    if ('manifest' in ok) expect(ok.manifest.files[0].lengths).toBeUndefined()
  })
  it('diff indirme baytını gerçek parça uzunluklarından sayar', () => {
    const prev = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_000, chunks: [h(1), h(2)], lengths: [3_000_000, 2_000_000] }] })
    const next = validateManifestInput({ ...base, files: [{ path: 'Game.exe', size: 5_000_100, chunks: [h(9), h(2)], lengths: [3_000_100, 2_000_000] }] })
    if (!('manifest' in prev) || !('manifest' in next)) throw new Error('beklenmedik')
    const diff = diffManifests(prev.manifest, next.manifest)
    expect(diff.downloadBytes).toBe(3_000_100)
    expect(diff.changed).toBe(1)
  })
})
