// Mesaj listeleri için geriye dönük uyumlu sayfalama yardımcısı.
// `before` = ISO tarih imleci (istemcide görünen en eski mesajın createdAt'i); verilmezse ilk sayfa.
// `limit` 1..max aralığına kıstırılır; geçersiz/eksik değerlerde varsayılan sayfa boyutu kullanılır.
// Sorgu her zaman createdAt azalan sırada `take` kadar çekilip ters çevrilir; böylece hem ilk sayfa
// hem de "daha eski" sayfaları artan sırada döner ve ilk sayfa her zaman en yeni mesajlardır.

export type PageArgs = { take: number; where: { createdAt: { lt: Date } } | {} }

export function pageArgs(before?: string, limit?: string, defaults: { take: number; max: number } = { take: 100, max: 100 }): PageArgs {
  const max = Math.max(1, Math.floor(defaults.max))
  const fallback = Math.min(max, Math.max(1, Math.floor(defaults.take)))
  const parsed = limit === undefined || limit === null || String(limit).trim() === '' ? NaN : Number(limit)
  const take = Number.isFinite(parsed) ? Math.min(max, Math.max(1, Math.floor(parsed))) : fallback
  const cursor = before ? new Date(String(before)) : null
  const where = cursor && !Number.isNaN(cursor.getTime()) ? { createdAt: { lt: cursor } } : {}
  return { take, where }
}
