import type { Meta } from '../types'

const DEFAULT_VENUE_EMOJIS: Record<string, string> = {
  panda: '🐼', koala: '🐨', gorilla: '🦍', tiger: '🐯',
  china_cat: '🐆', cat_planet: '🐱', giraffe: '🦒', asian_elephant: '🐘',
  orangutan: '🦧', asian_primates: '🐒', red_panda: '🐾', kangaroo: '🦘',
  lemur: '🦝', rhino: '🦏', hornbill: '🦜', crane: '🦢',
  wolf: '🐺', bear: '🐻', monkey_mountain: '🐵', meerkat: '🦡',
  tangjiahe: '🏞️', gonwana: '🦎', dazhuangguange: '🏛️',
}

export function venueEmoji(venueId: string, meta?: Meta | null): string {
  if (meta?.venue_emojis && meta.venue_emojis[venueId]) {
    return meta.venue_emojis[venueId]
  }
  return DEFAULT_VENUE_EMOJIS[venueId] || '🏠'
}
