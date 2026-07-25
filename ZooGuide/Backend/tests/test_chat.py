from app.chat import (
    ENTITY_MAP,
    SIMPLE_RULES,
    _extract_entity,
    _rule_based_reply,
    _load_system_prompt,
)


def test_entity_map_loaded():
    assert len(ENTITY_MAP) >= 1
    assert "熊猫" in ENTITY_MAP or "大熊猫馆" in ENTITY_MAP


def test_simple_rules_loaded():
    assert len(SIMPLE_RULES) >= 1


def test_extract_entity():
    result = _extract_entity("我想看熊猫")
    assert result is not None
    assert result["venue_id"] == "panda"


def test_extract_entity_no_match():
    result = _extract_entity("今天天气真好")
    assert result is None


def test_rule_based_reply_tired():
    result = _rule_based_reply("好累啊走不动了")
    assert result is not None
    assert "route_action" in result


def test_rule_based_reply_greeting():
    result = _rule_based_reply("你好")
    assert result is not None


def test_rule_based_reply_no_match():
    result = _rule_based_reply("今天天气不错")
    assert result is None


def test_load_system_prompt():
    prompt = _load_system_prompt()
    assert len(prompt) > 10
    assert "规则" in prompt or "Agent" in prompt or "动物园" in prompt
