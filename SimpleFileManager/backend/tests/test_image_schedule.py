"""Tests for the picture index window."""

from datetime import datetime, timedelta

import pytest

from app.image_indexer import ImageIndexScheduler, _window_end


class FakeIndexer:
    def __init__(self):
        self.starts = 0
        self.stops = 0
        self.running = True

    def start(self, *a, **k):
        self.starts += 1
        self.running = True
        return {"started": True, "running": True}

    def stop(self, *a, **k):
        self.stops += 1
        self.running = False

    def status(self):
        return {"running": self.running}


@pytest.fixture
def scheduler(monkeypatch):
    fake = FakeIndexer()
    monkeypatch.setattr("app.image_indexer.image_indexer", fake)
    monkeypatch.setattr("app.image_indexer.datetime", _FrozenClock(datetime(2026, 11, 14, 9, 5)))
    sch = ImageIndexScheduler()
    sch.fake = fake
    return sch


def _settings(enabled=True, window="09:00-09:30"):
    class S:
        pass
    s = S()
    s.image_index_enabled = enabled
    s.image_index_window = window
    return s


def test_the_window_closes_thirty_minutes_later():
    assert _window_end(datetime(2026, 11, 14, 9, 0), "09:00-09:30") == \
        datetime(2026, 11, 14, 9, 30)


def test_a_window_that_wraps_past_midnight():
    assert _window_end(datetime(2026, 11, 14, 23, 30), "23:00-07:00") == \
        datetime(2026, 11, 15, 7, 0)


def test_the_build_starts_when_the_window_opens(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="09:00-09:30"))
    monkeypatch.setattr("app.image_indexer.datetime", _FrozenClock(datetime(2026, 11, 14, 9, 5)))
    scheduler._tick()
    assert scheduler.fake.starts == 1
    assert scheduler.fake.stops == 0


def test_nothing_happens_outside_the_window(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="09:00-09:30"))
    monkeypatch.setattr("app.image_indexer.datetime", _FrozenClock(datetime(2026, 11, 14, 8, 30)))
    scheduler._tick()
    assert scheduler.fake.starts == 0


def test_the_build_stops_when_the_window_closes(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="09:00-09:30"))
    clock = _FrozenClock(datetime(2026, 11, 14, 9, 5))
    monkeypatch.setattr("app.image_indexer.datetime", clock)
    scheduler._tick()
    assert scheduler.fake.starts == 1

    clock.move_to(datetime(2026, 11, 14, 9, 31))
    scheduler._tick()
    assert scheduler.fake.stops == 1


def test_the_build_is_not_started_twice_in_one_window(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="09:00-09:30"))
    clock = _FrozenClock(datetime(2026, 11, 14, 9, 5))
    monkeypatch.setattr("app.image_indexer.datetime", clock)
    scheduler._tick()
    scheduler._tick()
    scheduler._tick()
    assert scheduler.fake.starts == 1


def test_the_next_window_starts_a_new_build(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="09:00-09:30"))
    clock = _FrozenClock(datetime(2026, 11, 14, 9, 5))
    monkeypatch.setattr("app.image_indexer.datetime", clock)
    scheduler._tick()
    clock.move_to(datetime(2026, 11, 14, 9, 31))
    scheduler._tick()
    clock.move_to(datetime(2026, 11, 15, 9, 5))
    scheduler._tick()
    assert scheduler.fake.starts == 2
    assert scheduler.fake.stops == 1


def test_a_disabled_schedule_does_nothing(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(enabled=False))
    monkeypatch.setattr("app.image_indexer.datetime", _FrozenClock(datetime(2026, 11, 14, 9, 5)))
    scheduler._tick()
    assert scheduler.fake.starts == 0


def test_an_all_day_window_never_stops_the_build(scheduler, monkeypatch):
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="all"))
    clock = _FrozenClock(datetime(2026, 11, 14, 9, 5))
    monkeypatch.setattr("app.image_indexer.datetime", clock)
    scheduler._tick()
    clock.move_to(datetime(2026, 11, 14, 23, 59))
    scheduler._tick()
    assert scheduler.fake.starts == 1
    assert scheduler.fake.stops == 0


def test_an_all_day_window_picks_up_new_pictures_later(scheduler, monkeypatch):
    """Once the build is done, an all-day schedule looks for more."""
    monkeypatch.setattr("app.deps.state.get_settings",
                        lambda: _settings(window="all"))
    clock = _FrozenClock(datetime(2026, 11, 14, 9, 5))
    monkeypatch.setattr("app.image_indexer.datetime", clock)
    scheduler._tick()
    assert scheduler.fake.starts == 1

    # The build is running; nothing happens yet.
    clock.move_to(datetime(2026, 11, 14, 9, 10))
    scheduler._tick()
    assert scheduler.fake.starts == 1

    # The build is over and the cooldown has passed.
    scheduler.fake.running = False
    clock.move_to(datetime(2026, 11, 14, 10, 0))
    scheduler._tick()
    assert scheduler.fake.starts == 2


class _FrozenClock:
    """A datetime that stands still until the test moves it."""

    def __init__(self, now):
        self._now = now

    def now(self):
        return self._now

    def __call__(self, *a, **k):
        return self._now

    def move_to(self, when):
        self._now = when
