from app.data_loader import get_meta, get_all_venue_dicts, get_venue_dict_by_id, get_all_facilities


def test_get_meta_has_required_keys():
    meta = get_meta()
    assert "name" in meta
    assert "short_name" in meta
    assert "open_time" in meta
    assert "close_time" in meta


def test_get_meta_gates():
    meta = get_meta()
    gates = meta.get("gates", {})
    assert isinstance(gates, dict)
    assert len(gates) >= 1
    for key, val in gates.items():
        assert "lat" in val
        assert "lon" in val


def test_get_all_venue_dicts():
    venues = get_all_venue_dicts()
    assert isinstance(venues, list)
    assert len(venues) >= 1
    v = venues[0]
    assert "id" in v
    assert "name" in v
    assert "lat" in v
    assert "lon" in v


def test_get_venue_dict_by_id():
    v = get_venue_dict_by_id("panda")
    assert v is not None
    assert v["id"] == "panda"
    assert v["name"] == "大熊猫馆"


def test_get_venue_dict_by_id_not_found():
    v = get_venue_dict_by_id("nonexistent_venue_xyz")
    assert v is None


def test_get_all_facilities():
    facs = get_all_facilities()
    assert isinstance(facs, list)


def test_meta_interest_map():
    meta = get_meta()
    im = meta.get("interest_map", {})
    assert isinstance(im, dict)
    assert len(im) >= 1


def test_meta_entity_map():
    meta = get_meta()
    em = meta.get("entity_map", {})
    assert isinstance(em, dict)
    assert len(em) >= 1
