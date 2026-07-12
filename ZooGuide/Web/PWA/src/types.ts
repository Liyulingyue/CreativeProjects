export type PartyType = 'solo' | 'couple' | 'family_young' | 'family_teen' | 'seniors'
export type Gate = 'north' | 'south' | 'east'
export type InterestTag =
  | 'panda'
  | 'ape'
  | 'cat'
  | 'bird'
  | 'australian'
  | 'african'
  | 'local'
  | 'exotic'
  | 'kids_favorite'

export interface UserPreference {
  available_hours: number
  party_type: PartyType
  with_kids: boolean
  kids_age?: number | null
  stamina: number
  sun_tolerance: number
  willing_to_hike: boolean
  animal_interests: InterestTag[]
  entry_gate: Gate
  start_time: string
  fast?: boolean
  strict_hours?: boolean
  style?: string
}

export interface RouteStop {
  venue_id: string
  venue_name: string
  arrive_time: string
  leave_time: string
  visit_minutes: number
  walk_to_next_minutes: number
  narration: string
  tips: string[]
  rest_here: boolean
}

export interface Route {
  id: string
  summary: string
  total_minutes: number
  total_walk_minutes: number
  stops: RouteStop[]
  warnings: string[]
  tips: string[]
  fallback: boolean
  llm_used?: boolean
  // echoes for replan
  _party_type?: PartyType
  _with_kids?: boolean
  _kids_age?: number | null
  _stamina?: number
  _sun_tolerance?: number
  _willing_to_hike?: boolean
  _animal_interests?: InterestTag[]
  _entry_gate?: Gate
  _start_time?: string
  _available_hours?: number
}

export interface Venue {
  id: string
  name: string
  area: string
  animals: string[]
  tags: string[]
  themes: string[]
  recommended_visit_minutes: number
  kid_friendly: number
  photo_op: number
  must_see: boolean
  shaded: boolean
  rest_spots: boolean
  description?: string
  open_time?: string
  close_time?: string
}

export interface QuizOption {
  value: string
  label: string
  icon?: string
  desc?: string
}

export interface QuizOptions {
  party_types: QuizOption[]
  interests: QuizOption[]
  gates: QuizOption[]
  stamina_descriptions: Record<string, string>
  sun_descriptions: Record<string, string>
}

export interface Meta {
  name: string
  name_en?: string
  address: string
  area_km2?: number
  open_time: string
  close_time: string
  ticket: string
  highlights: string[]
  gates: string[]
  gates_text?: string[]
  areas: Record<string, string>
}

export interface CheckinResponse {
  ok: boolean
  session_id: string
  total_checkins: number
  completion_rate: number
  venue_name: string
}

export interface NearestResult {
  id: string
  name: string
  distance_m: number
  area: string
  animals: string[]
}

export interface NearestResponse {
  lat: number
  lon: number
  in_park_estimate: boolean
  bbox: { min_lat: number; max_lat: number; min_lon: number; max_lon: number }
  results: NearestResult[]
}

export interface AutoCheckin {
  id: number
  venue_id: string
  venue_name: string
  ts: string
}

export interface PhotoEvaluation {
  evaluation_id: string
  animal_guess: string
  animal_confidence: number
  matched_venue_id: string
  matched_venue_name: string
  caption: string
  vibe_score: number
  vibe_label: string
  comment: string
  badge: string
  tips: string[]
  fallback: boolean
  fallback_reason?: string
  image_path?: string
  ts: string
  auto_checkin?: AutoCheckin
}

export interface Facility {
  id: string
  name: string
  category: string
  area: string
  near_venue_id?: string
  lat?: number
  lon?: number
  description: string
  tags: string[]
  open_time: string
  near_venue_name?: string
}