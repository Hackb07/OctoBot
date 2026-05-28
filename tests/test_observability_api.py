import pytest

from backend.octobot_orchestrator.main import (
    metrics,
    observability_logs,
    observability_traces,
)


@pytest.mark.asyncio
async def test_observability_exports_metrics_and_traces():
    metrics_response = await metrics()
    traces = await observability_traces()
    logs = await observability_logs(x_correlation_id="test-corr")

    assert "octobot_tasks_total" in metrics_response
    assert "spans" in traces
    assert "logs" in logs
