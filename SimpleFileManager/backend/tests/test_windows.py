"""Tests for the windows: the text indexer's and the picture indexer's."""

from datetime import datetime

import pytest

from app.indexer import _in_active_hours, _parse_window


def _at(hour, minute=0):
    return datetime(2026, 11, 14, hour, minute)


def test_a_plain_window():
    assert _in_active_hours("09:00-17:00", _at(12)) is True
    assert _in_active_hours("09:00-17:00", _at(8)) is False
    assert _in_active_hours("09:00-17:00", _at(18)) is False


def test_a_window_that_wraps_past_midnight():
    assert _in_active_hours("23:00-07:00", _at(2)) is True
    assert _in_active_hours("23:00-07:00", _at(12)) is False


def test_a_window_written_with_twenty_four_hours():
    """A natural way to write "from midnight to six".

    It used to mean the hours between midnight and six in the morning,
    and it has to keep meaning that: treating it as "all day" would turn
    a night-time window into a round-the-clock one.
    """
    assert _in_active_hours("24:00-06:00", _at(2)) is True
    assert _in_active_hours("24:00-06:00", _at(12)) is False


def test_something_that_is_not_a_window_is_all_day():
    """What this has always done: a broken window does not stop the work."""
    assert _in_active_hours("not-a-window", _at(12)) is True
    assert _in_active_hours("", _at(12)) is True
    assert _in_active_hours("all", _at(12)) is True


def test_the_picture_window_insists_on_real_hours():
    """The picture index cannot start a build at hour 24."""
    from app.image_indexer import _sane_window

    assert _sane_window("09:00-09:30") is True
    assert _sane_window("24:00-06:00") is False
    assert _sane_window("99:99-99:99") is False
    assert _sane_window("all") is False
    assert _sane_window("nonsense") is False


def test_a_window_of_the_same_length_is_all_day():
    assert _in_active_hours("09:00-09:00", _at(12)) is True


def test_the_parser_returns_minutes_since_midnight():
    assert _parse_window("09:30-17:45") == (9 * 60 + 30, 17 * 60 + 45)
    assert _parse_window("all") is None
