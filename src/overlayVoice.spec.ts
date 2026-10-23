import { expect, test } from 'vitest'

// overlaySession → apiBase penceresiz çalışmaz; en küçük sahte pencere yeterli (dönüşüm saf).
;(globalThis as any).window = {
  location: { search: '', origin: 'http://yerel' },
  localStorage: { getItem: () => null, setItem() {} },
}

const { toOverlayVoiceState, voiceLabel } = await import('./overlay/useOverlayVoice')
const { EMPTY_VOICE_STATE } = await import('./overlay/overlayTypes')

const me = { userId: 'me', displayName: 'Ben', muted: false, deafened: false, video: false, screen: false }
const ayse = { userId: 'u1', displayName: 'Ayşe', avatarUrl: 'a.png', muted: true, deafened: false, video: false, screen: true }
const people = [{ id: 'u1', displayName: 'Ayşe' }, { id: 'u2', displayName: 'Mert', avatarUrl: 'm.png' }, { id: 'u3', displayName: 'Ece' }]
const base = {
  me: 'me', call: null, incoming: null, muted: false, deafened: false, micReady: true, connectedAt: null, ringing: false,
  speaking: {}, levels: {}, userVolumes: {}, locallyMuted: new Set<string>(), voiceRooms: {},
}

test('aramada değilken boş durum (me ve odalar hariç EMPTY_VOICE_STATE ile aynı)', () => {
  const state = toOverlayVoiceState({ ...base, voiceRooms: { ch1: [ayse], ch2: [] } }, people)
  expect(state).toEqual({ ...EMPTY_VOICE_STATE, me: 'me', voiceRooms: { ch1: [{ userId: 'u1', displayName: 'Ayşe', avatarUrl: 'a.png' }] } })
})

test('sesli odada: konuşan/seviye/kişi sesi/yerel susturma eşlenir, davetliler "çalıyor" olarak eklenir', () => {
  const call = { id: 'c1', kind: 'channel' as const, organizationId: 'o', channelId: 'ch1', participants: [me, ayse, { ...me, userId: 'u2', displayName: 'Mert' }], invited: ['u3', 'me'] }
  const state = toOverlayVoiceState({
    ...base, call, connectedAt: 1000, speaking: { u1: true, u2: true }, levels: { u1: 0.9, u2: 0.4567 }, userVolumes: { u2: 80 }, locallyMuted: new Set(['u2']),
  }, people)
  expect(state).toMatchObject({ active: true, kind: 'channel', callId: 'c1', channelId: 'ch1', label: 'Sesli oda', ringing: false, connectedAt: 1000, me: 'me' })
  expect(state.participants.map(p => p.userId)).toEqual(['me', 'u1', 'u2', 'u3'])
  // Susturulmuş katılımcı konuşuyor sayılmaz ve seviyesi sıfırlanır.
  expect(state.participants[1]).toMatchObject({ displayName: 'Ayşe', avatarUrl: 'a.png', muted: true, screen: true, speaking: false, level: 0, volume: 100, locallyMuted: false })
  expect(state.participants[2]).toMatchObject({ displayName: 'Mert', avatarUrl: 'm.png', speaking: true, level: 0.46, volume: 80, locallyMuted: true })
  expect(state.participants[3]).toMatchObject({ displayName: 'Ece', pending: true, speaking: false, level: 0 })
  expect(state.participants[0].pending).toBeUndefined()
})

test('bire bir arama: etiket karşı tarafın adı, çalıyor durumu ve gelen arama eşlenir', () => {
  const call = { id: 'c2', kind: 'direct' as const, organizationId: 'o', channelId: null, caller: 'me', callee: 'u1', participants: [me], invited: ['u1'] }
  expect(voiceLabel(call, 'me', people)).toBe('Ayşe')
  const ringing = toOverlayVoiceState({ ...base, call, ringing: true }, people)
  expect(ringing).toMatchObject({ active: true, kind: 'direct', label: 'Ayşe', ringing: true, connectedAt: null })
  const incoming = toOverlayVoiceState({ ...base, incoming: { call: { ...call, id: 'c3', caller: 'u1', callee: 'me' }, from: { userId: 'u1', displayName: 'Ayşe' }, silent: true } }, people)
  expect(incoming.incoming).toEqual({ callId: 'c3', from: { userId: 'u1', displayName: 'Ayşe' }, kind: 'direct', silent: true })
  expect(incoming.active).toBe(false)
})

test('locallyMuted dizi olarak da verilebilir (IPC üzerinden Set gelmez)', () => {
  const call = { id: 'c1', kind: 'group' as const, organizationId: 'o', channelId: null, participants: [me, ayse], invited: [] }
  const state = toOverlayVoiceState({ ...base, call, locallyMuted: ['u1'] }, people)
  expect(state.label).toBe('Grup araması')
  expect(state.participants[1].locallyMuted).toBe(true)
})
