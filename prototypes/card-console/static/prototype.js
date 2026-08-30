(() => {
  "use strict";

  const variants = [
    { key: "A", name: "Queue first" },
    { key: "B", name: "Chronology first" },
    { key: "C", name: "Evidence first" },
  ];

  const scenarios = {
    running: "Live · retry recovered",
    accepted: "Accepted · evidence ready",
    missing: "Trace unavailable",
  };

  const stages = {
    frame: { title: "Frame activity", trace: "trace seq 32 · complete", cardTitle: "Frame record", cardSeq: "card seq 7", input: "card @ 1", changes: "—", evidence: "accepted", outcome: "accepted", defaultEvent: "model-frame" },
    plan: { title: "Plan activity", trace: "trace seq 94 · complete", cardTitle: "Plan record", cardSeq: "card seq 14", input: "card @ 7", changes: "—", evidence: "6 checks", outcome: "accepted", defaultEvent: "model-plan" },
    build: { title: "Build activity", trace: "trace seq 188 · live", cardTitle: "Build record", cardSeq: "card seq 27", input: "card @ 22", changes: "3 paths", evidence: "gate pending", outcome: "in progress", defaultEvent: "tool-failed" },
  };

  const traceEvents = {
    "tool-failed": {
      kind: "TOOL CALL",
      title: "cargo test --workspace",
      status: "FAILED",
      statusClass: "bad",
      sequence: "trace seq 184",
      relation: "Build / model step 14 / call tool_7Jq2 / retry 1",
      invocation: `<dl class="detail-rows"><div><dt>Command</dt><dd><code>cargo test --workspace</code></dd></div><div><dt>Working directory</dt><dd><code>/work/daemar</code></dd></div><div><dt>Environment delta</dt><dd><code>RUST_BACKTRACE=1</code></dd></div><div><dt>Producer</dt><dd>codex tool adapter</dd></div></dl>`,
      output: `<div class="stream-label"><span>STDOUT · 318 B</span><button type="button">Open artifact</button></div><pre>running 42 tests\ntest card_projection::tests::accepted_stage_is_terminal ... FAILED</pre><div class="stream-label"><span>STDERR · 196 B</span><b>exit 101</b></div><pre class="stderr">assertion failed\n  left: running\n right: accepted</pre><p class="artifact-ref">artifact sha256:7f83…a921 · retained 7 days</p>`,
      metrics: `<div class="metric-grid"><div><span>Wall time</span><b>12.4s</b></div><div><span>CPU time</span><b>8.9s</b></div><div><span>Output</span><b>514 B</b></div><div><span>Exit</span><b class="bad-text">101</b></div></div><div class="waterfall"><span style="width:18%">spawn 0.2s</span><span style="width:68%">process 11.8s</span><span style="width:14%">capture 0.4s</span></div>`,
      raw: `<pre>{\n  "type": "tool.completed",\n  "trace_seq": 184,\n  "call_id": "tool_7Jq2",\n  "exit_code": 101,\n  "duration_ms": 12403\n}</pre>`,
    },
    "model-build": {
      kind: "MODEL STEP",
      title: "Investigate failing projection test",
      status: "COMPLETED",
      statusClass: "good",
      sequence: "trace seq 181–183",
      relation: "Build / model step 14 / parent agent build-1",
      invocation: `<dl class="detail-rows"><div><dt>Provider / model</dt><dd>OpenAI · gpt-5.4</dd></div><div><dt>Input</dt><dd>4 messages · 12,842 tokens reported</dd></div><div><dt>Tools exposed</dt><dd>read, search, edit, exec</dd></div><div><dt>Stop reason</dt><dd>tool call</dd></div></dl><div class="reasoning-summary"><span>PROVIDER-SUPPLIED REASONING SUMMARY</span><p>The failure likely reflects a fixture state mismatch rather than projection logic. Run the full test to preserve exact evidence.</p></div>`,
      output: `<div class="message-stack"><span>VISIBLE ASSISTANT MESSAGE</span><p>The state projection is correct. I’m narrowing the failing assertion to the terminal transition.</p></div><div class="tool-intent"><span>NEXT ACTION</span><code>cargo test --workspace</code></div>`,
      metrics: `<div class="metric-grid"><div><span>Input</span><b>12,842</b></div><div><span>Cached input</span><b>8,190</b></div><div><span>Output</span><b>1,204</b></div><div><span>Total latency</span><b>8.4s</b></div></div><div class="waterfall"><span style="width:10%">queue .8s</span><span style="width:72%">model 6.1s</span><span style="width:18%">stream 1.5s</span></div>`,
      raw: `<pre>{\n  "type": "model.completed",\n  "trace_seq": 183,\n  "reported_input_tokens": 12842,\n  "reported_output_tokens": 1204,\n  "stop_reason": "tool_call"\n}</pre>`,
    },
    "retry-build": {
      kind: "RETRY",
      title: "Attempt 2 of 3",
      status: "IN PROGRESS",
      statusClass: "warn",
      sequence: "trace seq 185",
      relation: "Build / model step 15 / caused by trace seq 184",
      invocation: `<dl class="detail-rows"><div><dt>Policy</dt><dd>agent-directed correction</dd></div><div><dt>Cause</dt><dd>tool exit 101</dd></div><div><dt>Previous event</dt><dd>trace seq 184</dd></div><div><dt>Next event</dt><dd>trace seq 186</dd></div></dl>`,
      output: `<div class="retry-chain"><div class="failed-node"><span>1</span><b>Full workspace test</b><em>failed · 12.4s</em></div><i></i><div class="active-node"><span>2</span><b>Fixture correction</b><em>running</em></div><i></i><div><span>3</span><b>Available retry</b><em>unused</em></div></div>`,
      metrics: `<div class="metric-grid"><div><span>Retry</span><b>2 / 3</b></div><div><span>Elapsed</span><b>31.2s</b></div><div><span>Prior failures</span><b>1</b></div><div><span>Backoff</span><b>0s</b></div></div>`,
      raw: `<pre>{\n  "type": "retry.started",\n  "trace_seq": 185,\n  "attempt": 2,\n  "cause_seq": 184\n}</pre>`,
    },
    "tool-passed": {
      kind: "TOOL CALL",
      title: "cargo test card_projection",
      status: "PASSED",
      statusClass: "good",
      sequence: "trace seq 188",
      relation: "Build / model step 15 / call tool_P9x1 / retry 2",
      invocation: `<dl class="detail-rows"><div><dt>Command</dt><dd><code>cargo test card_projection</code></dd></div><div><dt>Working directory</dt><dd><code>/work/daemar</code></dd></div><div><dt>Producer</dt><dd>codex tool adapter</dd></div></dl>`,
      output: `<div class="stream-label"><span>STDOUT · 244 B</span><button type="button">Open artifact</button></div><pre>running 8 tests\n........\ntest result: ok. 8 passed; 0 failed</pre><p class="artifact-ref">artifact sha256:2d19…0ac4 · retained 7 days</p>`,
      metrics: `<div class="metric-grid"><div><span>Wall time</span><b>4.1s</b></div><div><span>CPU time</span><b>3.4s</b></div><div><span>Output</span><b>244 B</b></div><div><span>Exit</span><b class="good-text">0</b></div></div>`,
      raw: `<pre>{\n  "type": "tool.completed",\n  "trace_seq": 188,\n  "exit_code": 0,\n  "duration_ms": 4102\n}</pre>`,
    },
    "model-frame": { kind: "MODEL STEP", title: "Frame task boundary", status: "COMPLETED", statusClass: "good", sequence: "trace seq 32", relation: "Frame / model step 3", invocation: `<dl class="detail-rows"><div><dt>Input</dt><dd>Task frontmatter + framing contract</dd></div><div><dt>Provider / model</dt><dd>OpenAI · gpt-5.4</dd></div></dl>`, output: `<div class="message-stack"><span>STRUCTURED CONCLUSION</span><p>Proceed with the Card console as a read-mostly local operator surface. Exclude workflow editing and Nar'baha telemetry.</p></div>`, metrics: `<div class="metric-grid"><div><span>Input</span><b>4,211</b></div><div><span>Output</span><b>622</b></div><div><span>Latency</span><b>5.8s</b></div></div>`, raw: `<pre>{ "type": "model.completed", "trace_seq": 32 }</pre>` },
    "model-plan": { kind: "MODEL STEP", title: "Plan implementation slices", status: "COMPLETED", statusClass: "good", sequence: "trace seq 91", relation: "Plan / model step 8", invocation: `<dl class="detail-rows"><div><dt>Input</dt><dd>Card @ 7 + plan/v1 contract</dd></div><div><dt>Provider / model</dt><dd>OpenAI · gpt-5.4</dd></div></dl>`, output: `<div class="message-stack"><span>VISIBLE ASSISTANT MESSAGE</span><p>Implement append paths first, then projections, then the operator console against fixture-complete data.</p></div>`, metrics: `<div class="metric-grid"><div><span>Input</span><b>7,902</b></div><div><span>Output</span><b>1,044</b></div><div><span>Latency</span><b>7.1s</b></div></div>`, raw: `<pre>{ "type": "model.completed", "trace_seq": 91 }</pre>` },
    "tool-read": { kind: "TOOL CALL", title: "Read conventions.md", status: "COMPLETED", statusClass: "good", sequence: "trace seq 94", relation: "Plan / model step 9 / call tool_3Hk8", invocation: `<dl class="detail-rows"><div><dt>Path</dt><dd><code>conventions.md</code></dd></div><div><dt>Operation</dt><dd>read text</dd></div></dl>`, output: `<div class="stream-label"><span>OUTPUT · 4.8 KB</span><button type="button">Open artifact</button></div><pre># Rust conventions\n\nThe adjudication referent for Rust code…</pre>`, metrics: `<div class="metric-grid"><div><span>Duration</span><b>18ms</b></div><div><span>Bytes</span><b>4.8 KB</b></div></div>`, raw: `<pre>{ "type": "tool.completed", "trace_seq": 94 }</pre>` },
  };

  function selectedVariant() {
    const candidate = new URLSearchParams(window.location.search)
      .get("variant")
      ?.toUpperCase();
    return variants.some(({ key }) => key === candidate) ? candidate : "A";
  }

  function showVariant(key) {
    document.querySelectorAll("[data-variant]").forEach((element) => {
      element.hidden = element.dataset.variant !== key;
    });

    const current = variants.find(({ key: candidate }) => candidate === key);
    document.querySelector("#variant-label").textContent =
      `${current.key} — ${current.name}`;
    document.body.dataset.currentVariant = key;
  }

  function navigate(offset) {
    const currentIndex = variants.findIndex(
      ({ key }) => key === document.body.dataset.currentVariant,
    );
    const nextIndex = (currentIndex + offset + variants.length) % variants.length;
    const next = variants[nextIndex].key;
    const url = new URL(window.location.href);
    url.searchParams.set("variant", next);
    window.history.replaceState({}, "", url);
    showVariant(next);
  }

  function showInspectorTab(name) {
    document.querySelectorAll("[data-inspector-tab]").forEach((button) => {
      button.setAttribute("aria-selected", String(button.dataset.inspectorTab === name));
    });
    document.querySelectorAll("[data-inspector-panel]").forEach((panel) => {
      panel.hidden = panel.dataset.inspectorPanel !== name;
    });
  }

  function renderInspector(key) {
    const event = traceEvents[key];
    const target = document.querySelector("#event-inspector-content");
    if (!event || !target) return;

    target.innerHTML = `<header class="event-detail-header">
      <div><span>${event.kind}</span><h4>${event.title}</h4></div>
      <b class="event-status ${event.statusClass}">${event.status}</b>
    </header>
    <div class="event-provenance"><span>${event.sequence}</span><span>${event.relation}</span></div>
    <nav class="inspector-tabs" aria-label="Event detail">
      <button type="button" data-inspector-tab="invocation" aria-selected="true">Invocation</button>
      <button type="button" data-inspector-tab="output" aria-selected="false">Output</button>
      <button type="button" data-inspector-tab="metrics" aria-selected="false">Timing & usage</button>
      <button type="button" data-inspector-tab="raw" aria-selected="false">Raw</button>
    </nav>
    <div class="inspector-panel" data-inspector-panel="invocation">${event.invocation}</div>
    <div class="inspector-panel" data-inspector-panel="output" hidden>${event.output}</div>
    <div class="inspector-panel" data-inspector-panel="metrics" hidden>${event.metrics}</div>
    <div class="inspector-panel" data-inspector-panel="raw" hidden>${event.raw}</div>`;

    document.querySelectorAll("[data-inspector-tab]").forEach((button) => {
      button.addEventListener("click", () => showInspectorTab(button.dataset.inspectorTab));
    });
    document.querySelectorAll("[data-inspect-event]").forEach((row) => {
      row.classList.toggle("selected-event", row.dataset.inspectEvent === key);
    });
  }

  function selectStage(name) {
    const stage = stages[name];
    if (!stage) return;
    document.body.dataset.selectedStage = name;
    document.querySelectorAll("[data-stage-pane]").forEach((pane) => {
      pane.hidden = pane.dataset.stagePane !== name;
    });
    document.querySelectorAll("[data-stage-button]").forEach((button) => {
      button.classList.toggle("selected-stage", button.dataset.stageButton === name);
    });
    document.querySelector("#stage-activity-title").textContent = stage.title;
    document.querySelector("#stage-trace-sequence").textContent = stage.trace;
    document.querySelector("#card-stage-title").textContent = stage.cardTitle;
    document.querySelector("#card-stage-sequence").textContent = stage.cardSeq;
    document.querySelector("#card-stage-input").textContent = stage.input;
    document.querySelector("#card-stage-changes").textContent = stage.changes;
    document.querySelector("#card-stage-evidence").textContent =
      name === "build" && document.body.dataset.scenario === "accepted"
        ? "gate passed"
        : stage.evidence;
    const outcome = name === "build" && document.body.dataset.scenario === "accepted"
      ? "accepted"
      : stage.outcome;
    const outcomeElement = document.querySelector("#card-stage-outcome");
    outcomeElement.textContent = outcome;
    outcomeElement.classList.toggle("in-progress", outcome === "in progress");
    outcomeElement.classList.toggle("accepted-value", outcome === "accepted");
    renderInspector(stage.defaultEvent);
  }

  function setScenario(name) {
    document.body.dataset.scenario = name;
    document.querySelectorAll("[data-scenario-button]").forEach((button) => {
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.scenarioButton === name),
      );
    });
    document.querySelectorAll("[data-scenario-copy]").forEach((element) => {
      element.textContent = scenarios[name];
    });
    document.querySelectorAll("[data-missing-trace]").forEach((element) => {
      element.hidden = name !== "missing";
    });
    document.querySelectorAll("[data-live-trace]").forEach((element) => {
      element.hidden = name === "missing";
    });
    document.querySelectorAll("[data-accepted-only]").forEach((element) => {
      element.hidden = name !== "accepted";
    });
    document.querySelectorAll("[data-running-only]").forEach((element) => {
      element.hidden = name === "accepted";
    });
    document.querySelectorAll("[data-stage-state]").forEach((element) => {
      element.classList.remove(
        element.dataset.runningClass,
        element.dataset.acceptedClass,
      );
      element.classList.add(
        name === "accepted"
          ? element.dataset.acceptedClass
          : element.dataset.runningClass,
      );
    });
    selectStage(document.body.dataset.selectedStage ?? "build");
  }

  document.addEventListener("DOMContentLoaded", () => {
    showVariant(selectedVariant());
    setScenario("running");

    document.querySelector("#variant-previous").addEventListener("click", () => {
      navigate(-1);
    });
    document.querySelector("#variant-next").addEventListener("click", () => {
      navigate(1);
    });
    document.querySelectorAll("[data-scenario-button]").forEach((button) => {
      button.addEventListener("click", () => setScenario(button.dataset.scenarioButton));
    });
    document.querySelectorAll("[data-stage-button]").forEach((button) => {
      button.addEventListener("click", () => selectStage(button.dataset.stageButton));
    });
    document.querySelectorAll("[data-inspect-event]").forEach((row) => {
      const inspect = () => renderInspector(row.dataset.inspectEvent);
      row.addEventListener("click", inspect);
      row.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") inspect();
      });
    });
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target.isContentEditable
    ) return;
    navigate(event.key === "ArrowLeft" ? -1 : 1);
  });
})();
