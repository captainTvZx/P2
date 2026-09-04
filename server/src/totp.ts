import { createHmac, randomBytes } from 'node:crypto'

// Google Authenticator uyumlu TOTP (RFC 6238, SHA-1, 6 hane, 30 sn).
const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'

export function generateSecret(bytes = 20) {
  const raw = randomBytes(bytes)
  let bits = '', out = ''
  for (const byte of raw) bits += byte.toString(2).padStart(8, '0')
  for (let index = 0; index + 5 <= bits.length; index += 5) out += ALPHABET[parseInt(bits.slice(index, index + 5), 2)]
  return out
}

function decode(secret: string) {
  const clean = secret.toUpperCase().replace(/[^A-Z2-7]/g, '')
  let bits = ''
  for (const char of clean) bits += ALPHABET.indexOf(char).toString(2).padStart(5, '0')
  const bytes: number[] = []
  for (let index = 0; index + 8 <= bits.length; index += 8) bytes.push(parseInt(bits.slice(index, index + 8), 2))
  return Buffer.from(bytes)
}

export function codeFor(secret: string, counter: number) {
  const buffer = Buffer.alloc(8)
  buffer.writeUInt32BE(Math.floor(counter / 2 ** 32), 0)
  buffer.writeUInt32BE(counter >>> 0, 4)
  const digest = createHmac('sha1', decode(secret)).update(buffer).digest()
  const offset = digest[digest.length - 1] & 0x0f
  const value = ((digest[offset] & 0x7f) << 24) | (digest[offset + 1] << 16) | (digest[offset + 2] << 8) | digest[offset + 3]
  return String(value % 1_000_000).padStart(6, '0')
}

// Saat kaymasına karşı bir adım öncesi ve sonrası da kabul edilir.
export function verifyCode(secret: string, code: string, window = 1) {
  const clean = String(code || '').replace(/\D/g, '')
  if (clean.length !== 6) return false
  const counter = Math.floor(Date.now() / 30000)
  for (let step = -window; step <= window; step++) if (codeFor(secret, counter + step) === clean) return true
  return false
}

export function otpauthUri(secret: string, account: string, issuer = 'P2') {
  return `otpauth://totp/${encodeURIComponent(issuer)}:${encodeURIComponent(account)}?secret=${secret}&issuer=${encodeURIComponent(issuer)}&algorithm=SHA1&digits=6&period=30`
}
