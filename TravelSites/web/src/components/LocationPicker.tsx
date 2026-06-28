import { useState, useMemo } from 'react';
import regionsData from '../../public/regions.json';

interface Region {
  county: string;
  city: string;
  province: string;
  lat: number;
  lon: number;
}

interface Props {
  onClose: () => void;
  onConfirm: (picked: { province: string; city: string; county: string }) => void;
  current: { province: string; city: string; county: string };
}

export function LocationPicker({ onClose, onConfirm, current }: Props) {
  const regions = regionsData as Region[];

  const initialMatch = regions.find((r) => r.county === current.county);
  const [province, setProvince] = useState(current.province || initialMatch?.province || '北京市');
  const [city, setCity] = useState(current.city || initialMatch?.city || '北京市');
  const [county, setCounty] = useState(current.county);

  const provinces = useMemo(
    () => Array.from(new Set(regions.map((r) => r.province))).sort(),
    [regions]
  );

  const cities = useMemo(
    () => Array.from(new Set(
      regions.filter((r) => r.province === province).map((r) => r.city)
    )).sort(),
    [regions, province]
  );

  const counties = useMemo(
    () => regions
      .filter((r) => r.province === province && r.city === city)
      .map((r) => r.county)
      .sort(),
    [regions, province, city]
  );

  const handleProvinceChange = (val: string) => {
    setProvince(val);
    const newCities = Array.from(new Set(
      regions.filter((r) => r.province === val).map((r) => r.city)
    ));
    if (newCities.length > 0) {
      const firstCity = newCities[0];
      setCity(firstCity);
      const newCounties = regions.filter(
        (r) => r.province === val && r.city === firstCity
      );
      if (newCounties.length > 0) {
        setCounty(newCounties[0].county);
      }
    }
  };

  const handleCityChange = (val: string) => {
    setCity(val);
    const newCounties = regions.filter(
      (r) => r.province === province && r.city === val
    );
    if (newCounties.length > 0) {
      setCounty(newCounties[0].county);
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
                  <option key={c} value={c}>{c}</option>
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
        </div>

        <div className="modal-footer">
          <button className="btn btn-secondary" onClick={onClose}>取消</button>
          <button className="btn btn-primary" onClick={handleConfirm}>确定</button>
        </div>
      </div>
    </div>
  );
}