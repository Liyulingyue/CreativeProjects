"""The daily digest, and the schedule that writes it.

A digest is written either by the model or as a plain list of file
names -- the second happens when the model was busy at the time. A
plain list is a poor substitute, so the scheduler tries again once the
model has a chance; it must not try again every minute.
"""
from datetime import datetime

import pytest

from app.auto_digest import AutoDigestScheduler, RETRY_FALLBACK_SECONDS


@pytest.fixture
def scheduler():
    s = AutoDigestScheduler()
    s.reset()
    return s


class TestTheSchedule:
    def test_nothing_happens_while_the_digest_is_switched_off(self, scheduler, monkeypatch):
        from app.deps import state

        settings = state.get_settings()
        monkeypatch.setattr(settings, "auto_digest_enabled", False, raising=False)

        called = []
        monkeypatch.setattr(
            "app.auto_digest.digest_service.generate",
            lambda date: called.append(date) or {"success": True},
        )

        scheduler._tick()

        assert called == []

    def test_it_writes_a_digest_when_the_hour_has_come(self, scheduler, monkeypatch):
        from app.deps import state

        settings = state.get_settings()
        monkeypatch.setattr(settings, "auto_digest_enabled", True, raising=False)
        monkeypatch.setattr(settings, "auto_digest_mode", "scheduled", raising=False)
        monkeypatch.setattr(settings, "auto_digest_hour", 0, raising=False)

        called = []
        monkeypatch.setattr(
            "app.auto_digest.digest_service.generate",
            lambda date: called.append(date) or {"success": True, "generated_by": "llm"},
        )
        monkeypatch.setattr(
            "app.auto_digest.digest_service.get",
            lambda date: None,
        )

        scheduler._tick()

        assert called == [datetime.now().strftime("%Y-%m-%d")]


class TestADigestThatIsOnlyAList:
    """The model was busy when this one was written."""

    def _turn_the_digest_on(self, monkeypatch):
        from app.deps import state

        settings = state.get_settings()
        monkeypatch.setattr(settings, "auto_digest_enabled", True, raising=False)
        monkeypatch.setattr(settings, "auto_digest_mode", "scheduled", raising=False)
        monkeypatch.setattr(settings, "auto_digest_hour", 0, raising=False)

    def _fallback(self, monkeypatch):
        monkeypatch.setattr(
            "app.auto_digest.digest_service.get",
            lambda date: {"date": date, "generated_by": "fallback", "content": "只是清单"},
        )

    def test_it_is_written_again_once_the_model_is_free(self, scheduler, monkeypatch):
        self._turn_the_digest_on(monkeypatch)
        self._fallback(monkeypatch)
        results = []

        def generate(date):
            results.append(date)
            return {"success": True, "generated_by": "llm"}

        monkeypatch.setattr("app.auto_digest.digest_service.generate", generate)

        scheduler._tick()

        assert len(results) == 1, "应该再写一次"

    def test_but_not_every_minute(self, scheduler, monkeypatch):
        self._turn_the_digest_on(monkeypatch)
        self._fallback(monkeypatch)
        results = []

        def generate(date):
            results.append(date)
            return {"success": True, "generated_by": "llm"}

        monkeypatch.setattr("app.auto_digest.digest_service.generate", generate)

        scheduler._tick()
        scheduler._tick()
        scheduler._tick()

        assert len(results) == 1, f"写得太勤了: {len(results)} 次"

    def test_and_the_wait_is_bounded(self):
        assert RETRY_FALLBACK_SECONDS >= 5 * 60
