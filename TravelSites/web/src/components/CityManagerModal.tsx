import { useState, useEffect } from 'react';

interface Props {
  cities: string[];
  onSave: (cities: string[]) => void;
  onClose: () => void;
}

export function CityManagerModal({ cities, onSave, onClose }: Props) {
  const [list, setList] = useState<string[]>([]);
  const [newCity, setNewCity] = useState('');
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [showDropdown, setShowDropdown] = useState(false);
  const [allGeoCities, setAllGeoCities] = useState<string[]>([]);
  const [msg, setMsg] = useState<{ text: string; err?: boolean } | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setList(cities);
    fetch('/api/geo/cities')
      .then((r) => r.json())
      .then((d) => setAllGeoCities(d.cities || []))
      .catch(() => {});
  }, [cities]);

  const handleAdd = () => {
    const city = newCity.trim();
    if (!city) return;
    if (list.includes(city)) {
      setMsg({ text: `「${city}」已在列表中`, err: true });
      return;
    }
    setList([...list, city]);
    setNewCity('');
    setSuggestions([]);
    setShowDropdown(false);
    setMsg(null);
  };

  const handleRemove = (city: string) => {
    setList(list.filter((c) => c !== city));
  };

  const handleSave = async () => {
    setSaving(true);
    setMsg(null);
    try {
      const res = await fetch('/api/admin/cities', {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${localStorage.getItem('travelsites_token')}`,
        },
        body: JSON.stringify({ cities: list }),
      });
      if (res.ok) {
        onSave(list);
        onClose();
      } else {
        setMsg({ text: '保存失败', err: true });
      }
    } catch {
      setMsg({ text: '保存失败', err: true });
    } finally {
      setSaving(false);
    }
  };

  const handleInputChange = (val: string) => {
    setNewCity(val);
    if (val.trim()) {
      const filtered = allGeoCities.filter((c) => c.includes(val)).slice(0, 8);
      setSuggestions(filtered);
      setShowDropdown(filtered.length > 0);
    } else {
      setSuggestions([]);
      setShowDropdown(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content city-manager-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>目标城市</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="modal-body">
          <div className="city-manager-grid">
            {list.length === 0 ? (
              <p className="city-empty">暂未添加任何城市</p>
            ) : (
              list.map((c) => (
                <div key={c} className="city-tag">
                  <span>{c}</span>
                  <button className="city-tag-remove" onClick={() => handleRemove(c)}>×</button>
                </div>
              ))
            )}
          </div>

          <div className="city-manager-add">
            <div className="city-combobox">
              <input
                placeholder="搜索并添加城市…"
                value={newCity}
                onChange={(e) => handleInputChange(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') { setShowDropdown(false); handleAdd(); }
                  if (e.key === 'Escape') setShowDropdown(false);
                }}
              />
              {showDropdown && suggestions.length > 0 && (
                <ul className="city-dropdown">
                  {suggestions.map((s) => (
                    <li key={s} onClick={() => { setNewCity(s); setShowDropdown(false); }}>{s}</li>
                  ))}
                </ul>
              )}
            </div>
            <button className="add-city-btn" onClick={handleAdd} disabled={!newCity.trim()}>+</button>
          </div>

          {msg && (
            <p className="save-msg" style={{ color: msg.err ? 'var(--danger)' : 'var(--success)' }}>
              {msg.text}
            </p>
          )}
        </div>

        <div className="modal-footer">
          <button className="btn btn-secondary" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
  );
}
