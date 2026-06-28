export interface MatrixCell {
  start_offset: number;
  duration: number;
  start_date: string;
  end_date: string;
  score: number | null;
  recommendation: string | null;
  weather_summary: string | null;
  success: boolean;
  error: string | null;
  full_result: object | null;
}

export interface CityMatrix {
  city: string;
  generated_at: string;
  cells: MatrixCell[];
  total: number;
  success_count: number;
}

export interface CityList {
  cities: string[];
  count: number;
}

export interface Health {
  status: string;
  refresh_enabled: boolean;
  seed_cities: string[];
}

export interface RefreshStatus {
  is_running: boolean;
  last_run: string | null;
  cities_completed: number;
  cities_total: number;
}

export interface TripPlan {
  city: string;
  score: number;
  score_breakdown: {
    days_match: number;
    weather: number;
    attraction_density: number;
    transport: number;
  };
  recommendation: string;
  weather_summary: string;
  weather_strategy: string;
  top_attractions: string[];
  key_highlights: string;
}

export interface SearchResult {
  items: SearchResultItem[];
  total: number;
  generated_at: string;
}

export interface SearchResultItem {
  city: string;
  start_date: string;
  end_date: string;
  duration_days: number;
  score: number;
  recommendation: string;
  weather_summary: string;
  weather_desc: string;
  top_attractions: string[];
  key_highlights: string;
  score_breakdown: {
    days_match: number;
    weather: number;
    attraction_density: number;
    transport: number;
  };
  daily_plan: DailyPlan[];
  preference_score?: number;
}

export interface DailyPlan {
  day: number;
  date: string;
  theme: string;
  weather_hint: string;
  routes: Route[];
}

export interface Route {
  route_id: string;
  tags: string[];
  activities: Activity[];
  total_hours: number;
}

export interface Activity {
  attraction: string;
  time_slot: string;
  hours: number;
  notes: string;
}
