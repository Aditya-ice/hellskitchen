"""The HTTP surface, including how it behaves with no credentials.

The POS must work whether or not this service is configured, so the
unconfigured and failing paths matter as much as the happy one.
"""

from fastapi.testclient import TestClient

from brain import app as app_module
from brain.agent import AgentAnswer

client = TestClient(app_module.app)


class TestHealth:
    def test_reports_the_pinned_model(self, monkeypatch):
        monkeypatch.setattr(app_module, "_has_credentials", lambda: True)
        body = client.get("/health").json()

        assert body["ok"] is True
        assert body["model"] == "claude-opus-5"
        assert body["configured"] is True

    def test_admits_when_it_has_no_credentials(self, monkeypatch):
        monkeypatch.setattr(app_module, "_has_credentials", lambda: False)
        assert client.get("/health").json()["configured"] is False


class TestAsk:
    def test_says_so_plainly_when_unconfigured(self, monkeypatch):
        monkeypatch.setattr(app_module, "_has_credentials", lambda: False)
        body = client.post("/ask", json={"question": "who is waiting?"}).json()

        assert body["configured"] is False
        assert "not configured" in body["answer"]
        assert "ANTHROPIC_API_KEY" in body["answer"]

    def test_returns_the_agent_answer(self, monkeypatch):
        monkeypatch.setattr(app_module, "_has_credentials", lambda: True)

        class StubAgent:
            async def ask(self, question, max_tokens=4096):
                assert question == "who is waiting?"
                return AgentAnswer(text="Priya, 47 minutes.", tools_used=["query_floor"])

        monkeypatch.setattr(app_module, "_agent", lambda: StubAgent())
        body = client.post("/ask", json={"question": "who is waiting?"}).json()

        assert body["answer"] == "Priya, 47 minutes."
        assert body["tools_used"] == ["query_floor"]

    def test_a_failure_does_not_become_an_http_error(self, monkeypatch):
        # ember-server treats a non-200 as the brain being down. A crash here
        # is an unavailable agent, not a broken POS, and it must read that way.
        monkeypatch.setattr(app_module, "_has_credentials", lambda: True)

        class ExplodingAgent:
            async def ask(self, question, max_tokens=4096):
                raise RuntimeError("upstream on fire")

        monkeypatch.setattr(app_module, "_agent", lambda: ExplodingAgent())
        response = client.post("/ask", json={"question": "anything"})

        assert response.status_code == 200
        assert "unavailable" in response.json()["answer"]
        assert "POS is unaffected" in response.json()["answer"]

    def test_rejects_an_empty_question(self):
        assert client.post("/ask", json={"question": ""}).status_code == 422

    def test_rejects_an_essay(self):
        # A bounded question keeps the token cost of a single ask predictable.
        assert client.post("/ask", json={"question": "x" * 2001}).status_code == 422


class TestCredentialDetection:
    def test_an_api_key_counts(self, monkeypatch):
        monkeypatch.setenv("ANTHROPIC_API_KEY", "sk-test")
        assert app_module._has_credentials() is True

    def test_an_auth_token_counts(self, monkeypatch):
        # An unset ANTHROPIC_API_KEY does not mean there is no credential.
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
        monkeypatch.setenv("ANTHROPIC_AUTH_TOKEN", "token")
        assert app_module._has_credentials() is True

    def test_nothing_configured_reads_as_unconfigured(self, monkeypatch, tmp_path):
        monkeypatch.delenv("ANTHROPIC_API_KEY", raising=False)
        monkeypatch.delenv("ANTHROPIC_AUTH_TOKEN", raising=False)
        monkeypatch.setattr(
            app_module.os.path, "expanduser", lambda _: str(tmp_path / "nothing-here")
        )
        assert app_module._has_credentials() is False
