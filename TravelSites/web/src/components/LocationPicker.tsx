import { useState, useEffect, useMemo } from 'react';

interface City {
  name: string;
  counties: string[];
}

interface Region {
  province: string;
  cities: City[];
}

interface Props {
  onClose: () => void;
  onConfirm: (picked: { province: string; city: string; county: string }) => void;
  current: { province: string; city: string; county: string };
}

export function LocationPicker({ onClose, onConfirm, current }: Props) {
  const [regions, setRegions] = useState<Region[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  useEffect(() => {
    fetch('/api/regions')
      .then((r) => r.json())
      .then((data) => {
        setRegions(data.regions || []);
        setLoading(false);
      })
      .catch((e) => {
        setError('加载失败：' + e.message);
        setLoading(false);
      });
  }, []);

  const [province, setProvince] = useState(current.province || '北京市');
  const [city, setCity] = useState(current.city || '北京市');
  const [county, setCounty] = useState(current.county);

  // 当 regions 加载完后，如果当前值没匹配，自动重置为第一项
  useEffect(() => {
    if (regions.length > 0) {
      const prov = regions.find((r) => r.province === province);
      if (!prov) {
        const first = regions[0];
        setProvince(first.province);
        const firstCity = first.cities[0];
        if (firstCity) {
          setCity(firstCity.name);
          setCounty(firstCity.counties[0] || '');
        }
      } else {
        const cityObj = prov.cities.find((c) => c.name === city);
        if (!cityObj && prov.cities.length > 0) {
          const firstCity = prov.cities[0];
          setCity(firstCity.name);
          setCounty(firstCity.counties[0] || '');
        } else if (cityObj && !cityObj.counties.includes(county)) {
          setCounty(cityObj.counties[0] || '');
        }
      }
    }
  }, [regions]);

  const provinces = useMemo(() => regions.map((r) => r.province), [regions]);
  const cities = useMemo(
    () => regions.find((r) => r.province === province)?.cities || [],
    [regions, province]
  );
  const counties = useMemo(
    () => cities.find((c) => c.name === city)?.counties || [],
    [cities, city]
  );

  const handleProvinceChange = (val: string) => {
    setProvince(val);
    const prov = regions.find((r) => r.province === val);
    if (prov && prov.cities.length > 0) {
      const firstCity = prov.cities[0];
      setCity(firstCity.name);
      setCounty(firstCity.counties[0] || '');
    }
  };

  const handleCityChange = (val: string) => {
    setCity(val);
    const c = cities.find((c) => c.name === val);
    if (c) {
      setCounty(c.counties[0] || '');
    }
  };

  const handleConfirm = () => {
    onConfirm({ province, city, county });
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2>选择出发地</h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="modal-body">
          {loading && (
            <div className="loading">
              <div className="spinner" />
              <p style={{ marginTop: 12 }}>加载中...</p>
            </div>
          )}

          {error && (
            <div className="profile-helper" style={{ color: 'var(--danger)' }}>{error}</div>
          )}

          {!loading && !error && (
            <div className="picker-cascade">
              <div className="picker-row">
                <label className="picker-label">省 / 直辖市</label>
                <select
                  className="picker-select"
                  value={province}
                  onChange={(e) => handleProvinceChange(e.target.value)}
                >
                  {provinces.map((p) => (
                    <option key={p} value={p}>{p}</option>
                  ))}
                </select>
              </div>

              <div className="picker-row">
                <label className="picker-label">城市</label>
                <select
                  className="picker-select"
                  value={city}
                  onChange={(e) => handleCityChange(e.target.value)}
                >
                  {cities.map((c) => (
                    <option key={c.name} value={c.name}>{c.name}</option>
                  ))}
                </select>
              </div>

              <div className="picker-row">
                <label className="picker-label">县 / 区</label>
                <select
                  className="picker-select picker-select-bold"
                  value={county}
                  onChange={(e) => setCounty(e.target.value)}
                >
                  {counties.map((c) => (
                    <option key={c} value={c}>{c}</option>
                  ))}
                </select>
              </div>
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button className="btn btn-secondary" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={handleConfirm} disabled={loading}>
            确定
          </button>
        </div>
      </div>
    </div>
  );
}