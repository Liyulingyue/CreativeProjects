from app.walking import haversine_m, get_inter_venue_minutes, get_entry_venue_minutes, build_walking_matrix


def test_haversine_same_point():
    assert haversine_m(32.1, 118.8, 32.1, 118.8) == 0


def test_haversine_known_distance():
    d = haversine_m(32.0, 118.0, 32.0, 119.0)
    assert 90000 < d < 110000


def test_get_inter_venue_minutes_same():
    assert get_inter_venue_minutes("panda", "panda") == 0


def test_get_inter_venue_minutes_positive():
    m = get_inter_venue_minutes("panda", "koala")
    assert m >= 1


def test_get_entry_venue_minutes():
    m = get_entry_venue_minutes("north", "panda")
    assert m >= 1


def test_build_walking_matrix():
    ids = ["panda", "koala"]
    m = build_walking_matrix(ids)
    assert "panda" in m
    assert "koala" in m
    assert m["panda"]["panda"] == 0
    assert m["panda"]["koala"] == m["koala"]["panda"]
