import React, { useEffect, useMemo, useState } from "react";
import { Activity, GitPullRequest, ListChecks, Radio, Search } from "lucide-react";
import { createRoot } from "react-dom/client";
import "./styles.css";

type Task = {
  id: string;
  status: string;
  request: { goal: string; repository: { path: string } };
  report?: {
    confidence?: number;
    validation_gate?: { passed: boolean; explanation: string };
    modified_files?: string[];
    pr_summary?: string;
    tools_run?: { name: string; status: string }[];
    failure_classifications?: { tool: string; classification: string; summary: string }[];
  };
};

const API_BASE = import.meta.env.VITE_OCTOBOT_API ?? "http://127.0.0.1:8787";

function App() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selected, setSelected] = useState<Task | null>(null);
  const [events, setEvents] = useState<string[]>([]);
  const [activePanel, setActivePanel] = useState("monitor");
  const [memoryQuery, setMemoryQuery] = useState("");
  const [memoryResults, setMemoryResults] = useState<string[]>([]);
  const [error, setError] = useState("");
  const approvals = useMemo(
    () => events.filter((event) => event.includes("approval.required")),
    [events]
  );

  useEffect(() => {
    fetch(`${API_BASE}/api/tasks`)
      .then((response) => response.json())
      .then((data) => {
        const next = data.tasks ?? [];
        setTasks(next);
        setSelected(next[0] ?? null);
      })
      .catch((err) => setError(String(err)));
  }, []);

  useEffect(() => {
    if (!selected) return;
    setEvents([]);
    const stream = new EventSource(`${API_BASE}/api/tasks/${selected.id}/events`);
    const append = (event: MessageEvent) => {
      setEvents((items) => [event.data, ...items].slice(0, 50));
    };
    [
      "task.created",
      "task.updated",
      "agent.message",
      "tool.started",
      "tool.completed",
      "tool.failed",
      "approval.required",
      "report.generated",
      "audit"
    ].forEach((type) => stream.addEventListener(type, append));
    stream.onerror = () => stream.close();
    return () => stream.close();
  }, [selected?.id]);

  return (
    <main className="shell">
      <nav className="sidebar">
        <div className="brand">OctoBot</div>
        <button onClick={() => setActivePanel("monitor")}><Activity size={18} /> Tasks</button>
        <button onClick={() => setActivePanel("stream")}><Radio size={18} /> Streams</button>
        <button onClick={() => setActivePanel("diffs")}><GitPullRequest size={18} /> Diffs</button>
        <button onClick={() => setActivePanel("approvals")}><ListChecks size={18} /> Approvals</button>
        <button onClick={() => setActivePanel("memory")}><Search size={18} /> Memory</button>
      </nav>
      <section className="content">
        <header>
          <h1>Autonomous Execution</h1>
          <span>{tasks.length} tasks</span>
        </header>
        {error && <p className="error">{error}</p>}
        <div className="grid">
          <section className="panel">
            <h2>Task History</h2>
            {tasks.map((task) => (
              <button
                className={task.id === selected?.id ? "task selected" : "task"}
                key={task.id}
                onClick={() => setSelected(task)}
              >
                <span>{task.request.goal}</span>
                <strong>{task.status}</strong>
              </button>
            ))}
          </section>
          <section className="panel">
            <h2>{panelTitle(activePanel)}</h2>
            {selected ? (
              renderPanel(activePanel, selected, events, approvals, memoryQuery, setMemoryQuery, memoryResults, setMemoryResults)
            ) : (
              <p>No task selected.</p>
            )}
          </section>
        </div>
      </section>
    </main>
  );
}

function panelTitle(panel: string) {
  return {
    monitor: "Execution Monitor",
    stream: "Event Stream",
    diffs: "Diff Viewer",
    approvals: "Approvals",
    memory: "Memory Search"
  }[panel] ?? "Execution Monitor";
}

function renderPanel(
  panel: string,
  task: Task,
  events: string[],
  approvals: string[],
  memoryQuery: string,
  setMemoryQuery: (value: string) => void,
  memoryResults: string[],
  setMemoryResults: (value: string[]) => void
) {
  if (panel === "diffs") {
    return (
      <div className="stack">
        <dl>
          <dt>Files</dt>
          <dd>{task.report?.modified_files?.join(", ") || "no files modified"}</dd>
          <dt>Summary</dt>
          <dd>{task.report?.pr_summary || "pending"}</dd>
        </dl>
        {(task.report?.failure_classifications ?? []).map((failure, index) => (
          <pre key={index}>{`${failure.tool}: ${failure.classification}\n${failure.summary}`}</pre>
        ))}
      </div>
    );
  }
  if (panel === "approvals") {
    return (
      <div className="stack">
        {approvals.length ? approvals.map((event, index) => <pre key={index}>{event}</pre>) : <p>No pending approvals.</p>}
      </div>
    );
  }
  if (panel === "memory") {
    return (
      <div className="stack">
        <form
          className="search"
          onSubmit={(event) => {
            event.preventDefault();
            setMemoryResults([
              `Query: ${memoryQuery || task.request.goal}`,
              `Repository: ${task.request.repository.path}`,
              `Task: ${task.id}`
            ]);
          }}
        >
          <input value={memoryQuery} onChange={(event) => setMemoryQuery(event.target.value)} />
          <button>Search</button>
        </form>
        {memoryResults.map((result, index) => <pre key={index}>{result}</pre>)}
      </div>
    );
  }
  if (panel === "stream") {
    return <div className="events">{events.map((event, index) => <pre key={index}>{event}</pre>)}</div>;
  }
  return (
    <>
      <dl>
        <dt>Repository</dt>
        <dd>{task.request.repository.path}</dd>
        <dt>Confidence</dt>
        <dd>{task.report?.confidence ?? "pending"}</dd>
        <dt>Validation</dt>
        <dd>{task.report?.validation_gate?.explanation ?? "pending"}</dd>
        <dt>Tools</dt>
        <dd>{task.report?.tools_run?.map((tool) => `${tool.name}:${tool.status}`).join(", ") ?? "pending"}</dd>
      </dl>
    </>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
