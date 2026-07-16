# Daemar

Daemar is a software factory and workflow engine for executing bounded software-generation work.

## Language

**Workflow**:
An ordered, strongly typed sequence of stages that owns a complete unit of work across its trust boundaries.
_Avoid_: Loop, flow, pipeline

**Workflow Definition**:
The compiled Rust program that defines a Workflow's exact stage sequence, configuration, typed handoffs, and failure behavior. The first Workflow uses direct linear control flow rather than a separately loaded document or reusable graph abstraction.
_Avoid_: Configuration file, workflow document, command graph, workflow DSL

**Workflow Run**:
One execution of a Workflow, including its trusted setup, sandboxed execution, and trusted publication stages.
_Avoid_: Job, workflow execution

**Change Request**:
The human-approved input describing the objective and acceptance criteria for a Workflow Run. Operational policy belongs to the repository and Workflow Definition rather than to the requester.
_Avoid_: Ticket, prompt, task

**Sandboxed Execution**:
The portion of a Workflow Run confined by an enforceable, deny-by-default capability policy. It can access only explicitly granted filesystem, network, credential, process, time, and resource capabilities.
_Avoid_: Isolated workspace, sandboxed workflow

**Generation Stage**:
A sandboxed stage in which Daemar calls a model provider directly and mediates every model-initiated operation through registered Model Tools. The first Workflow bounds iteration, tokens, time, and cost.
_Avoid_: Agent harness, coding agent

**Model Tool**:
A strongly typed, deterministic operation exposed to a model by Daemar. In the first Workflow, Model Tools are limited to repository navigation and structured editing; no shell or validation operation is exposed.
_Avoid_: Shell command, plugin

**Validation Stage**:
A deterministic Workflow stage that evaluates generated changes with configured static-analysis and linting tools after the Generation Stage. In the first Workflow, its results are not fed back into the model.
_Avoid_: Model review, agent review

**Run Record**:
The append-only JSONL event record of a Workflow Run, including every model request and response and every Model Tool request and result.
_Avoid_: Log, transcript

**Context Entry**:
An item deliberately admitted into a model request with its source, size, and reason recorded in the Run Record. Source code and prior history are retrieved on demand rather than preloaded.
_Avoid_: Context blob, repository dump
