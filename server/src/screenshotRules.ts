// Proje ekran görüntüleri için saf kurallar (vitest ile test edilir; Prisma/Nest bağımlılığı yok).
// Dosya yükleme iki adımlıdır: istemci önce POST /uploads ile JPEG'i yükler, sonra buraya yalnız
// "/uploads/<id>.jpg" yolunu bildirir. Sunucu yolu biçim olarak doğrular, meta dosyasından sahibi ve
// türü denetler; satırı silmek dosyayı silmez (mesajlarda paylaşılmış olabilir).

export const MAX_DIMENSION = 7680
export const MAX_CAPTION = 500
export const UPLOAD_PATH_RE = /^\/uploads\/[a-f0-9-]{36}\.(png|jpe?g|webp)$/i
const IMAGE_TYPES = new Set(['image/png', 'image/jpeg', 'image/webp'])

export function validUploadPath(value: unknown): value is string {
  return typeof value === 'string' && UPLOAD_PATH_RE.test(value)
}

// "/uploads/<id>.jpg" → "<id>.jpg" (disk üzerindeki dosya adı; meta dosyası "<id>.jpg.json").
export function uploadFileId(uploadPath: string) {
  return uploadPath.slice('/uploads/'.length)
}

// Meta dosyası: yükleyen bu kullanıcı mı ve içerik gerçekten görsel mi (sniffType sonucu).
export function uploadOwnedImage(meta: { userId?: unknown; type?: unknown } | null | undefined, userId: string) {
  if (!meta || typeof meta !== 'object') return false
  if (String(meta.userId || '') !== userId) return false
  return IMAGE_TYPES.has(String(meta.type || '').toLowerCase())
}

// Sahip her zaman; projeyi yöneten (channel.manage vekili) başkalarının görüntülerini de düzenler/siler.
export function canManageScreenshot(shot: { userId: string }, userId: string, canManage: boolean) {
  return shot.userId === userId || canManage
}

// Başlık: boşluklar kırpılır, satır sonları tek boşluğa iner, MAX_CAPTION ile sınırlanır; boş → null.
export function normalizeCaption(value: unknown): string | null {
  if (value === null || value === undefined) return null
  const text = String(value).replace(/\s+/g, ' ').trim().slice(0, MAX_CAPTION)
  return text || null
}

// Genişlik/yükseklik: 1..MAX_DIMENSION tam sayı; aksi hâlde null (istek reddedilir).
export function clampDimension(value: unknown): number | null {
  const n = Math.floor(Number(value))
  if (!Number.isFinite(n) || n < 1 || n > MAX_DIMENSION) return null
  return n
}

// Çekim zamanı: istemcinin ISO değeri geçerliyse ve geleceğe/çok eskiye kaçmıyorsa; değilse şimdi.
export function normalizeTakenAt(value: unknown, now = Date.now()): Date {
  if (typeof value !== 'string' || !value) return new Date(now)
  const at = new Date(value).getTime()
  if (!Number.isFinite(at)) return new Date(now)
  if (at > now + 5 * 60_000 || at < now - 365 * 86_400_000) return new Date(now)
  return new Date(at)
}
