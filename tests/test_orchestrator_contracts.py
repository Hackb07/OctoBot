import os

from backend.octobot_orchestrator.contracts import CodingTaskRequest, RepositoryRef
from backend.octobot_orchestrator.env import load_dotenv
from backend.octobot_orchestrator.providers import GroqProvider, ModelRouter


def test_coding_task_request_defaults_to_dry_run():
    request = CodingTaskRequest(goal="add auth", repository=RepositoryRef(path="."))

    assert request.dry_run is True
    assert request.max_iterations == 5


def test_model_router_registers_groq_provider():
    router = ModelRouter()

    assert isinstance(router.provider_for("groq"), GroqProvider)


def test_load_dotenv_reads_provider_keys(tmp_path, monkeypatch):
    monkeypatch.delenv("OPENAI_API_KEY", raising=False)
    monkeypatch.setenv("ANTHROPIC_API_KEY", "from-shell")
    env_file = tmp_path / ".env"
    env_file.write_text(
        "\n".join(
            [
                "OPENAI_API_KEY='from-dotenv'",
                "ANTHROPIC_API_KEY=from-dotenv",
                "OCTOBOT_GROQ_API_KEY=\"groq-dotenv\"",
            ]
        ),
        encoding="utf-8",
    )

    load_dotenv(env_file)

    assert os.environ["OPENAI_API_KEY"] == "from-dotenv"
    assert os.environ["ANTHROPIC_API_KEY"] == "from-shell"
    assert os.environ["OCTOBOT_GROQ_API_KEY"] == "groq-dotenv"
