import type { Meta } from '../types'

export function shortName(meta: Meta | null | undefined): string {
  if (!meta) return '动物园'
  return meta.short_name || meta.name.replace(/森林动物园$/, '').replace(/动物园$/, '') || '动物园'
}
