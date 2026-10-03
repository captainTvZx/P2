import { expect, test } from 'vitest'

// apiBase penceresiz çalışmaz; en küçük sahte pencere yeterli.
;(globalThis as any).window = {
  location: { search: '', origin: 'http://yerel' },
  localStorage: { getItem: () => null, setItem() {} },
}

const { attachmentsFromContent, attachmentPath, attachmentUrl } = await import('./attachments')
const API = 'http://yerel/api'

// 0.5.7'de ekler mesaja göreli yolla yazılmaya başlandı ama mesajı çizen desen
// hâlâ "http" ile başlayan adres istiyordu: ekler düz metin olarak görünüyordu.
test('göreli yollu ek tanınır ve okuyanın sunucusuna bağlanır', () => {
  const [file] = attachmentsFromContent('[[p2-file|Lambda.jpg|image%2Fjpeg|230233|/uploads/b6884eaf.jpg]]')
  expect(file).toBeTruthy()
  expect(file.name).toBe('Lambda.jpg')
  expect(file.type).toBe('image/jpeg')
  expect(file.size).toBe(230233)
  expect(file.url).toBe(`${API}/uploads/b6884eaf.jpg`)
})

test('eski mesajdaki başka makinenin adresi kendi sunucumuza çevrilir', () => {
  const [file] = attachmentsFromContent('[[p2-file|Eski.png|image%2Fpng|12|http://192.168.1.50:4000/uploads/xxxx.png]]')
  expect(file.url).toBe(`${API}/uploads/xxxx.png`)
})

test('mesaja yalnızca yol yazılır, dış bağlantılar korunur', () => {
  // Tarayıcı modunda API "/api" altında sunulur; yazılan yol yine sade kalmalı,
  // yoksa okunurken "/api/api/uploads/…" oluşur.
  expect(attachmentPath(`${API}/uploads/cccc.png`)).toBe('/uploads/cccc.png')
  expect(attachmentUrl('/uploads/cccc.png')).toBe(`${API}/uploads/cccc.png`)
  expect(attachmentUrl('http://192.168.1.50/api/uploads/dddd.png')).toBe(`${API}/uploads/dddd.png`)
  expect(attachmentUrl('https://ornek.com/resim.png')).toBe('https://ornek.com/resim.png')
})

test('marker olmayan satırlar ek sayılmaz', () => {
  expect(attachmentsFromContent('merhaba dünya')).toHaveLength(0)
  expect(attachmentsFromContent('[[p2-file|bozuk|image%2Fpng|abc|/uploads/x.png]]')).toHaveLength(0)
})

test('açıklama metni ile ekler ayrılır', async () => {
  const { withoutAttachments } = await import('./attachments')
  const icerik = 'Kabul kriterleri:\n- ses açılmalı\n[[p2-file|a.png|image%2Fpng|10|/uploads/a.png]]'
  expect(withoutAttachments(icerik)).toBe('Kabul kriterleri:\n- ses açılmalı')
  expect(attachmentsFromContent(icerik)).toHaveLength(1)
})

test('önizleme türü boş veya yanlış MIME değerinde uzantıdan bulunur', async () => {
  const { attachmentPreviewKind } = await import('./attachments')
  expect(attachmentPreviewKind({ name: 'ses.mp3', type: 'application/octet-stream' })).toBe('audio')
  expect(attachmentPreviewKind({ name: 'çizim.svg', type: 'application/octet-stream' })).toBe('svg')
  expect(attachmentPreviewKind({ name: 'film.mp4', type: '' })).toBe('video')
})

test('belge, arşiv ve bilinmeyen dosyalar ayrılır', async () => {
  const { attachmentPreviewKind } = await import('./attachments')
  expect(attachmentPreviewKind({ name: 'rapor.docx', type: '' })).toBe('docx')
  expect(attachmentPreviewKind({ name: 'tablo.xlsx', type: '' })).toBe('sheet')
  expect(attachmentPreviewKind({ name: 'paket.7z', type: '' })).toBe('archive')
  expect(attachmentPreviewKind({ name: 'oyun.bin', type: '' })).toBe('binary')
})
