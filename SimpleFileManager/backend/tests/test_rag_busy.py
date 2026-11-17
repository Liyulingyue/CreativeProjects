"""What the RAG endpoints say when the big cores are taken.

The model and the indexer share the big cores, and when they cannot
both have them the answer is "busy, try again in a moment" -- a 503
with a sentence a person can understand. It is not a 500 with a stack
trace, and it is not a NameError.
"""

from conftest import client

from app.cpu_pool import CpuPoolBusy
from app.deps import state


def _busy():
    """A rag service whose query always says the cores are taken."""
    class Busy:
        def query(self, *args, **kwargs):
            raise CpuPoolBusy("CPU 池在 5 秒内没有被释放")

    return Busy()


class TestTheRagQueryWhenTheCoresAreTaken:
    def test_a_busy_pool_is_a_503_and_not_a_crash(self):
        """The pool being busy is a normal thing, not a broken program.

        The handler that catches it names the exception. If that name
        is not in scope where the handler is, the handler itself
        raises -- and the person asking a question gets a stack trace
        instead of "the model is busy".
        """
        original = state.get_rag_service
        state.get_rag_service = _busy
        try:
            res = client.post("/api/rag/query", json={"question": "在吗"})
        finally:
            state.get_rag_service = original
        assert res.status_code == 503, res.text
        assert "忙" in res.json()["detail"]
        # And nothing English leaked out.
        assert "CpuPoolBusy" not in res.text
        assert "NameError" not in res.text

    def test_the_same_holds_for_the_other_errors(self):
        """Any other failure still gets the same treatment."""
        original = state.get_rag_service

        class Broken:
            def query(self, *args, **kwargs):
                raise RuntimeError("the model exploded")

        state.get_rag_service = lambda: Broken()
        try:
            res = client.post("/api/rag/query", json={"question": "在吗"})
        finally:
            state.get_rag_service = original
        assert res.status_code == 500
        assert "CpuPoolBusy" not in res.text
        assert "NameError" not in res.text
