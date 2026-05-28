from backend.octobot_orchestrator.contracts import CodingTaskRequest, RepositoryRef
from backend.octobot_orchestrator.providers import GroqProvider, ModelRouter


def test_coding_task_request_defaults_to_dry_run():
    request = CodingTaskRequest(goal="add auth", repository=RepositoryRef(path="."))

    assert request.dry_run is True
    assert request.max_iterations == 5


def test_model_router_registers_groq_provider():
    router = ModelRouter()

    assert isinstance(router.provider_for("groq"), GroqProvider)
