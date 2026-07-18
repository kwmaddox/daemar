#!/usr/bin/env python3
"""PROTOTYPE (PER-19) — throwaway CLI stub. Do not ship.

Simulates Daemar's run-inspection surface against fabricated Run Records
(fixture-runs/) so the CLI can be evaluated by feel:

  python3 daemar.py runs                      # newest first
  python3 daemar.py runs add-run-inspection   # filter by Change Request slug
  python3 daemar.py show 0197e1a0             # unambiguous id prefix
  python3 daemar.py show 0197e1               # ambiguous -> error

State is derived by folding each run-record.jsonl per PER-10: a record with
no run_terminated IS an interrupted run. Unknown kinds are retained and
ignored, per the open-kind reader contract.
"""
import json
import sys
from pathlib import Path

RUNS_DIR = Path(__file__).parent / "fixture-runs"


def fold(run_id, record_path, events):
    state = {
        "run_id": run_id, "record": record_path, "events": len(events),
        "cr": None, "started": None, "base": None, "commit": None, "pr": None,
        "outcome": "interrupted", "cause": None, "terminated_seq": None,
        "model_requests": 0, "tool_calls": 0, "seq_gaps": False,
    }
    last_seq = 0
    for event in events:
        seq = event.get("seq", 0)
        if seq != last_seq + 1:
            state["seq_gaps"] = True
        last_seq = seq
        kind = event.get("kind", "")
        payload = event.get("payload", {})
        if kind == "run_initialized.v1":
            state["cr"] = payload.get("change_request_id")
            state["started"] = event.get("ts")
            state["base"] = f"{payload.get('base_branch')} @ {payload.get('base_commit', '')[:7]}"
        elif kind == "model_request.v1":
            state["model_requests"] += 1
        elif kind == "model_tool_request.v1":
            state["tool_calls"] += 1
        elif kind == "commit_created.v1":
            state["commit"] = payload.get("commit_sha", "")[:7]
        elif kind == "draft_pr_created.v1":
            state["pr"] = payload.get("url")
        elif kind == "run_terminated.v1":
            state["outcome"] = payload.get("outcome")
            state["cause"] = payload.get("cause")
            state["terminated_seq"] = seq
    return state


def load_runs():
    runs = []
    for run_dir in RUNS_DIR.iterdir():
        record = run_dir / "run-record.jsonl"
        if not record.is_file():
            continue
        events = [json.loads(line) for line in record.read_text().splitlines() if line.strip()]
        runs.append(fold(run_dir.name, record, events))
    # UUIDv7 sorts chronologically; newest first.
    runs.sort(key=lambda r: r["run_id"], reverse=True)
    return runs


def cmd_runs(slug):
    runs = load_runs()
    if slug:
        runs = [r for r in runs if r["cr"] == slug]
        if not runs:
            print(f"no runs for Change Request `{slug}`", file=sys.stderr)
            return 1
    if not runs:
        print("no Workflow Runs yet")
        return 0
    print(f"{'ID':10}{'STARTED (UTC)':20}{'CHANGE REQUEST':22}{'OUTCOME':22}EVENTS")
    for r in runs:
        started = (r["started"] or "????-??-??T??:??:??Z").replace("T", " ")[:16]
        print(f"{r['run_id'][:8]:10}{started:20}{r['cr'] or '?':22}{r['outcome']:22}{r['events']}")
    return 0


def cmd_show(prefix):
    matches = [r for r in load_runs() if r["run_id"].startswith(prefix)]
    if not matches:
        print(f"no run matching `{prefix}`", file=sys.stderr)
        return 1
    if len(matches) > 1:
        print(f"`{prefix}` is ambiguous:", file=sys.stderr)
        for r in matches:
            print(f"  {r['run_id'][:8]}  {r['cr']}  {r['outcome']}", file=sys.stderr)
        return 1
    r = matches[0]
    run_dir = r["record"].parent
    print(f"run {r['run_id']}")
    print(f"  change request: {r['cr']}  (preserved: {run_dir}/change-request.json)")
    print(f"  started:        {r['started']}")
    if r["outcome"] == "interrupted":
        print(f"  outcome:        interrupted  (no run_terminated; detected on sweep)")
    else:
        print(f"  outcome:        {r['outcome']}  (run_terminated.v1 at seq {r['terminated_seq']})")
    if r["cause"]:
        cause = r["cause"]
        if isinstance(cause, dict):
            cause = f"{cause.get('kind')}: {cause.get('detail')}"
        print(f"  cause:          {cause}")
    print(f"  base:           {r['base']}")
    if r["commit"]:
        print(f"  commit:         {r['commit']}")
    if r["pr"]:
        print(f"  draft PR:       {r['pr']}")
    requests = f"{r['model_requests']} model request" + ("s" if r["model_requests"] != 1 else "")
    calls = f"{r['tool_calls']} tool call" + ("s" if r["tool_calls"] != 1 else "")
    print(f"  activity:       {r['events']} events - {requests} - {calls}")
    if r["seq_gaps"]:
        print(f"  warning:        seq gaps detected — record may be truncated")
    print(f"  run record:     {r['record']}")
    return 0


def main(argv):
    if len(argv) >= 2 and argv[1] == "runs" and len(argv) <= 3:
        return cmd_runs(argv[2] if len(argv) == 3 else None)
    if len(argv) == 3 and argv[1] == "show":
        return cmd_show(argv[2])
    print("usage: daemar.py {runs [change-request-slug] | show <run-id-prefix>}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
