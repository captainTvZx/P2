// Görüntü karşılaştırma: uyumsuzluk yüzdesi, fark görüntüsü, yan yana görüntü, karo/bölge analizi.
import { createRequire } from 'node:module'
import { readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { REPO } from './common.mjs'

const { PNG } = createRequire(join(REPO, 'package.json'))('pngjs')

export const readPng = file => PNG.sync.read(readFileSync(file))
const writePng = (file, png) => writeFileSync(file, PNG.sync.write(png, { colorType: 6 }))

/**
 * a = Electron (referans), b = Rust. Boyut farklıysa büyük olana göre hizalanır (eksik alan "uyumsuz" sayılır).
 * threshold: kanal başına en büyük fark (0-255) bunu aşarsa piksel uyumsuz. 24 ≈ yazı kenarı yumuşatma
 * farklarını tolere eder ama renk/konum farklarını yakalar.
 */
export function compare(fileA, fileB, { threshold = 24, tile = 40, regions = [] } = {}) {
  const a = readPng(fileA), b = readPng(fileB)
  const w = Math.max(a.width, b.width), h = Math.max(a.height, b.height)
  const diff = new PNG({ width: w, height: h })
  let bad = 0, sumAbs = 0
  const tilesX = Math.ceil(w / tile), tilesY = Math.ceil(h / tile)
  const tileBad = new Uint32Array(tilesX * tilesY)
  const regionBad = regions.map(() => 0)
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4
      const inA = x < a.width && y < a.height, inB = x < b.width && y < b.height
      let d = 255, lum = 0
      if (inA && inB) {
        const ia = (y * a.width + x) * 4, ib = (y * b.width + x) * 4
        const dr = Math.abs(a.data[ia] - b.data[ib]), dg = Math.abs(a.data[ia + 1] - b.data[ib + 1]), db = Math.abs(a.data[ia + 2] - b.data[ib + 2])
        d = Math.max(dr, dg, db)
        sumAbs += (dr + dg + db) / 3
        lum = (a.data[ia] * 0.3 + a.data[ia + 1] * 0.59 + a.data[ia + 2] * 0.11)
      } else sumAbs += 255
      const mismatch = d > threshold
      if (mismatch) {
        bad++
        tileBad[Math.floor(y / tile) * tilesX + Math.floor(x / tile)]++
        for (let r = 0; r < regions.length; r++) {
          const [rx, ry, rw, rh] = regions[r].rect
          if (x >= rx && x < rx + rw && y >= ry && y < ry + rh) regionBad[r]++
        }
        // Kırmızı: fark büyüklüğüne göre parlaklık
        diff.data[o] = 255; diff.data[o + 1] = Math.max(0, 80 - d / 3) | 0; diff.data[o + 2] = 60; diff.data[o + 3] = 255
      } else {
        // Referansın soluk gri kopyası
        const g = 20 + lum * 0.25
        diff.data[o] = g; diff.data[o + 1] = g; diff.data[o + 2] = g; diff.data[o + 3] = 255
      }
    }
  }
  const tiles = []
  for (let ty = 0; ty < tilesY; ty++) for (let tx = 0; tx < tilesX; tx++) {
    const tw = Math.min(tile, w - tx * tile), th = Math.min(tile, h - ty * tile)
    const pct = tileBad[ty * tilesX + tx] / (tw * th) * 100
    if (pct > 0) tiles.push({ x: tx * tile, y: ty * tile, w: tw, h: th, pct: +pct.toFixed(1) })
  }
  tiles.sort((p, q) => q.pct - p.pct)
  return {
    width: w, height: h,
    sizeA: [a.width, a.height], sizeB: [b.width, b.height],
    mismatchPct: +(bad / (w * h) * 100).toFixed(2),
    meanAbsError: +(sumAbs / (w * h)).toFixed(2),
    regions: regions.map((r, i) => ({ name: r.name, rect: r.rect, mismatchPct: +(regionBad[i] / (r.rect[2] * r.rect[3]) * 100).toFixed(2) })),
    worstTiles: tiles.slice(0, 12),
    a, b, diff,
  }
}

/** Electron | Rust | fark — 8 px aralıkla tek görüntü. */
export function writeOutputs(result, { diffFile, sideFile }) {
  writePng(diffFile, result.diff)
  const { a, b, diff } = result
  const gap = 8
  const W = a.width + b.width + diff.width + gap * 2, H = Math.max(a.height, b.height, diff.height)
  const out = new PNG({ width: W, height: H })
  out.data.fill(0)
  for (let i = 3; i < out.data.length; i += 4) { out.data[i] = 255; out.data[i - 1] = 60; out.data[i - 2] = 30; out.data[i - 3] = 30 }
  let ox = 0
  for (const img of [a, b, diff]) { PNG.bitblt(img, out, 0, 0, img.width, img.height, ox, 0); ox += img.width + gap }
  writePng(sideFile, out)
}
