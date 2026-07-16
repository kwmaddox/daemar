# Agent-effective repository Model Tool contracts

- **Ticket:** PER-14
- **Date:** 2026-07-16
- **Status:** Recommended semantic contract for the V1 prototype

## Question

Which provider-independent semantic Model Tool registry and concrete request,
result, and recoverable-error shapes best support a Generation Stage performing
bounded repository navigation and structured editing?

## Answer

Use four versioned semantic Model Tools:

1. `repository.list.v1`
2. `repository.search.v1`
3. `repository.read.v1`
4. `repository.edit.v1`

All paths are normalized repository-relative paths. Every observation that can
feed a later write carries an opaque content revision. Every potentially large
read is bounded and explicitly resumable. `repository.edit.v1` applies a batch
of exact, structured file operations transactionally: either every operation
commits or the repository is unchanged. Existing files require the revision
returned by a prior read or search, so concurrent changes become a recoverable
`stale_revision` error instead of an accidental overwrite.

Keep the registry small. Do not expose shell execution, validation commands,
provider request objects, streaming events, HTTP status codes, or provider tool
call identifiers through these contracts. Those are respectively outside the
V1 Model Tool set, owned by Validation Stages, or owned by PER-15's API
transport boundary.

## Why this shape

The strongest evaluation evidence is that interface design materially changes
coding-agent performance. SWE-agent's Agent-Computer Interface used a small set
of simple viewing, search, and editing actions with concise feedback and
outperformed a shell-only agent by 10.7 percentage points on SWE-bench Lite.
Its ablations also found that a bounded 100-line viewer beat both a 30-line
viewer and full-file display, summarized search beat iterative one-result
navigation, and guarded edits beat unguarded edits. The authors explicitly
attribute the design to simple actions, compact operations, and informative
errors ([SWE-agent paper, pp. 1-5](https://papers.nips.cc/paper_files/paper/2024/file/5a7c947568c1b1328ccc5230172e1e7c-Paper-Conference.pdf)).

Agentless is complementary evidence against a broad or clever V1 interface: a
simple, inspectable localization-repair-validation decomposition reached 32%
on SWE-bench Lite at low reported cost, despite avoiding a complex autonomous
tool loop. This does not determine individual schemas, but it supports keeping
the semantic surface narrow and composable
([Agentless paper](https://lingming.cs.illinois.edu/publications/fse2025.pdf)).

Provider guidance points in the same direction. Anthropic recommends clear
functional namespaces, strict input and output models, sensible defaults,
pagination or range selection, filters, explicit truncation, and actionable
errors. It also shows that concise response variants can use about one third of
the tokens of detailed ones in a representative retrieval tool
([Anthropic, "Writing effective tools for AI agents"](https://www.anthropic.com/engineering/writing-tools-for-agents)).

Leading coding-agent repositories supply concrete precedents:

- Gemini CLI separates directory listing, regex search, ranged file reading,
  and exact replacement. Its public schemas bound total matches and matches per
  file, its reader reports the displayed and total line ranges with a concrete
  continuation instruction, and its edit normally requires exactly one
  occurrence of the old text. Its typed errors distinguish no match, ambiguous
  match, no change, missing file, and create-on-existing-file
  ([tool registry and argument keys](https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/tools.md),
  [read continuation](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/tools/read-file.ts),
  [search bounds](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/tools/grep.ts),
  [edit matching and errors](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/tools/edit.ts)).
- OpenAI Codex's `apply_patch` grammar groups relative-path add, delete,
  update, and move operations in one structured patch. Updates carry old
  context, and application fails when that context cannot be found. This is
  evidence for a single structured edit surface and explicit mismatch feedback,
  not evidence that its current filesystem application is transactional
  ([Codex patch instructions](https://github.com/openai/codex/blob/main/codex-rs/core/prompt_with_apply_patch_instructions.md),
  [Codex apply implementation](https://github.com/openai/codex/blob/main/codex-rs/apply-patch/src/lib.rs)).
- Aider's SEARCH/REPLACE format asks for short old-text anchors with only
  enough surrounding context to be unique, and its failure path offers nearby
  actual lines when an exact match is absent. That is a useful recovery pattern
  for deterministic editing
  ([Aider edit prompts](https://github.com/Aider-AI/aider/blob/main/aider/coders/editblock_prompts.py),
  [Aider edit implementation](https://github.com/Aider-AI/aider/blob/main/aider/coders/editblock_coder.py)).

Finally, the Language Server Protocol supplies a mature semantic precedent for
version-addressed multi-resource edits. A `WorkspaceEdit` can combine versioned
text changes with ordered create, rename, and delete operations, and its
transactional failure mode means either all operations succeed or none are
applied ([LSP 3.17 `WorkspaceEdit`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspaceEdit)).

The contracts below are a synthesis of those sources. No source specifies this
exact registry.

## Shared semantic types and invariants

```text
RepoPath = string
Revision = opaque string
Cursor = opaque string

TextRange = {
  start_line: u32,   // one-based, inclusive
  start_column: u32, // one-based Unicode scalar position, inclusive
  end_line: u32,     // one-based, inclusive
  end_column: u32    // one-based Unicode scalar position, exclusive
}
```

`RepoPath` uses `/` separators, is relative to the repository root, and cannot
be empty except where a tool explicitly treats it as the root. Absolute paths,
`.` or `..` segments, NULs, and paths that resolve through a symlink outside the
repository are rejected. Results always return canonical `RepoPath` values.

`Revision` identifies the exact bytes of one file observation. Its encoding is
an implementation detail; callers compare or return it but never interpret it.
A revision is mandatory for modifying, moving, or deleting an existing file.

`Cursor` is an opaque continuation capability bound to the original normalized
request, ordering, bounds, and repository observation. A caller either supplies
the cursor alone or starts a fresh request; it cannot alter filters while
continuing. If the observation can no longer be continued consistently, the
tool returns `stale_cursor` with a fresh-request recovery shape.

All item collections have deterministic ordering. Every potentially large
result accepts both an item or line bound and `max_bytes`; the implementation
may clamp a requested bound to a documented system maximum and reports the
applied bounds. Results never silently drop data: they return `truncated` and a
`next_cursor` when more data can be read.

## Registry

| Tool | Effect | Purpose |
| --- | --- | --- |
| `repository.list.v1` | Read-only | Discover bounded repository-relative paths and kinds. |
| `repository.search.v1` | Read-only | Locate literal or regex matches with small previews. |
| `repository.read.v1` | Read-only | Read a bounded text-file window and obtain its revision. |
| `repository.edit.v1` | Mutating | Atomically create, replace, move, or delete text files. |

The descriptions presented to the model should say when to use each tool, the
default and maximum bounds, the path rules, whether ignored files are included,
and the exact recovery action for every error kind. The version suffix is part
of the semantic registry identity, not a provider API version.

## `repository.list.v1`

### Request

```json
{
  "path": "src",
  "depth": 2,
  "include": ["file", "directory", "symlink"],
  "limit": 200,
  "max_bytes": 16384
}
```

For continuation:

```json
{ "cursor": "opaque" }
```

`path` defaults to the repository root, `depth` defaults to `1`, and results
are ordered lexicographically by canonical path. Listing does not read file
contents or follow directory symlinks.

### Result

```json
{
  "path": "src",
  "entries": [
    { "path": "src/lib.rs", "kind": "file", "size_bytes": 4812 },
    { "path": "src/model", "kind": "directory" }
  ],
  "returned": 2,
  "applied_limit": 200,
  "applied_max_bytes": 16384,
  "truncated": true,
  "next_cursor": "opaque"
}
```

File size is useful for deciding whether to read narrowly; timestamps,
permissions, owners, and provider-specific metadata are omitted from V1.

## `repository.search.v1`

### Request

```json
{
  "path": "src",
  "query": { "kind": "literal", "text": "ModelTool" },
  "include_globs": ["**/*.rs"],
  "exclude_globs": ["target/**"],
  "case_sensitive": true,
  "context_lines": 1,
  "max_matches": 100,
  "max_matches_per_file": 20,
  "max_bytes": 32768
}
```

`query.kind` is `literal` or `regex`. Literal is the default recommendation;
regex syntax must be named and fixed by the implementation. Search respects
repository ignore rules by default. `context_lines` defaults to zero so the
normal result is a matching line, not an unsolicited code dump.

### Result

```json
{
  "matches": [
    {
      "path": "src/model_tool.rs",
      "revision": "opaque-file-revision",
      "range": {
        "start_line": 41,
        "start_column": 12,
        "end_line": 41,
        "end_column": 21
      },
      "preview": "pub enum ModelTool {"
    }
  ],
  "returned": 1,
  "applied_max_matches": 100,
  "applied_max_matches_per_file": 20,
  "applied_max_bytes": 32768,
  "truncated": false
}
```

Continuation uses `{ "cursor": "opaque" }`. Search does not promise an exact
total count because counting can defeat the bound. Each match carries the file
revision so it can safely seed a later read or edit.

## `repository.read.v1`

### Request

```json
{
  "path": "src/model_tool.rs",
  "start_line": 35,
  "line_count": 120,
  "max_bytes": 32768,
  "if_revision": "optional-observed-revision"
}
```

For continuation:

```json
{ "cursor": "opaque" }
```

`start_line` defaults to `1`. `line_count` has a bounded default. When
`if_revision` is present, reading a different revision returns
`stale_revision`; this prevents a search-to-read continuation from silently
switching underneath the model.

### Result

```json
{
  "path": "src/model_tool.rs",
  "revision": "opaque-file-revision",
  "start_line": 35,
  "end_line": 154,
  "total_lines": 311,
  "content": "...",
  "applied_line_count": 120,
  "applied_max_bytes": 32768,
  "truncated": true,
  "next_cursor": "opaque"
}
```

The result says exactly what was shown, how large the file is in lines, and how
to continue. Binary or non-UTF-8 content is a typed error in V1 rather than an
implicit lossy conversion.

## `repository.edit.v1`

### Request

```json
{
  "changes": [
    {
      "kind": "replace",
      "path": "src/model_tool.rs",
      "expected_revision": "opaque-file-revision",
      "edits": [
        {
          "old_text": "enum ToolKind {\n    Read,\n    Write,\n}",
          "new_text": "enum ToolKind {\n    List,\n    Search,\n    Read,\n    Edit,\n}",
          "expected_occurrences": 1
        }
      ]
    },
    {
      "kind": "create",
      "path": "src/tool_error.rs",
      "content": "...",
      "must_not_exist": true
    }
  ]
}
```

The closed V1 change union is:

```text
Create {
  kind: "create",
  path: RepoPath,
  content: string,
  must_not_exist: true
}

Replace {
  kind: "replace",
  path: RepoPath,
  expected_revision: Revision,
  edits: [ExactReplacement]
}

Move {
  kind: "move",
  from: RepoPath,
  to: RepoPath,
  expected_revision: Revision,
  destination_must_not_exist: true
}

Delete {
  kind: "delete",
  path: RepoPath,
  expected_revision: Revision
}

ExactReplacement {
  old_text: string,
  new_text: string,
  expected_occurrences: 1
}
```

V1 intentionally requires one exact occurrence. It does not offer fuzzy
matching or `replace_all`; the model can submit several explicit replacements
after inspecting the matches. Empty `old_text`, identical old and new text,
overlapping replacements, repeated operations on the same path, and operations
whose ordering is internally inconsistent are rejected before any write.

All preconditions and post-state contents are computed first. The commit step
is transactional across the whole request: all changes succeed or no repository
bytes or paths change. This is stricter than best-effort patch application and
makes the Run Record's result unambiguous.

The edit tool enforces text and repository invariants only. It does not run a
formatter, parser, linter, compiler, or test, because those are Validation
Operations outside the Generation Stage's Model Tool registry.

### Result

```json
{
  "applied": true,
  "files": [
    {
      "kind": "replace",
      "path": "src/model_tool.rs",
      "old_revision": "opaque-file-revision",
      "new_revision": "opaque-new-revision",
      "replacements": 1,
      "added_lines": 4,
      "removed_lines": 2
    },
    {
      "kind": "create",
      "path": "src/tool_error.rs",
      "new_revision": "opaque-created-revision",
      "added_lines": 88
    }
  ]
}
```

Return summaries and new revisions, not the full changed files. The model
already supplied the new text; echoing it would spend context without adding
evidence. A later read is available when observation is actually needed.

## Recoverable error contract

Every tool failure returns one discriminated semantic error. It is a tool
result, not a provider or transport failure.

```json
{
  "error": {
    "kind": "stale_revision",
    "message": "src/model_tool.rs changed after the referenced observation.",
    "path": "src/model_tool.rs",
    "expected_revision": "opaque-old-revision",
    "current_revision": "opaque-current-revision",
    "retry": {
      "tool": "repository.read.v1",
      "request": {
        "path": "src/model_tool.rs",
        "start_line": 35,
        "line_count": 120,
        "max_bytes": 32768
      }
    }
  }
}
```

The closed V1 error kinds are:

| Kind | Applies to | Recovery data |
| --- | --- | --- |
| `invalid_request` | all | Invalid field paths and a corrected example where useful. |
| `path_outside_repository` | all | Rejected path and repository-relative path rule. |
| `path_not_found` | all | Canonical parent path and optional close path candidates. |
| `path_exists` | create/move | Conflicting canonical path. |
| `path_kind_mismatch` | list/read/edit | Actual kind and required kind. |
| `not_text` | search/read/edit | Path and detected reason; no lossy content. |
| `invalid_pattern` | search | Pattern location and syntax message. |
| `invalid_cursor` | list/search/read | Fresh normalized request to restart. |
| `stale_cursor` | list/search/read | Fresh normalized request to restart. |
| `result_limit` | list/search/read | Used only when no complete item can fit; applied bounds and a narrower suggested request. |
| `stale_revision` | read/edit | Expected/current revision and a targeted fresh read. |
| `anchor_not_found` | edit | Path, concise old-text preview, and up to a small bounded set of nearby candidate ranges. |
| `ambiguous_anchor` | edit | Occurrence count and a bounded list of candidate ranges. |
| `overlapping_edits` | edit | Conflicting edit indices and ranges. |
| `atomic_commit_failed` | edit | Explicit `repository_unchanged: true`; no partial file list. |

Messages are concise and actionable, but callers branch on `kind`, not prose.
Recovery data is bounded. In particular, an anchor failure must not return an
entire large file; it should point the model toward a targeted re-read.

## Stale-write recovery sequence

1. `repository.search.v1` or `repository.read.v1` returns `path + revision`.
2. The model constructs exact replacements against those observed bytes.
3. `repository.edit.v1` checks every existing file revision before computing
   any write.
4. If any revision changed, it returns `stale_revision` and changes nothing.
5. The model performs the supplied targeted read, updates its exact anchor, and
   retries the complete atomic edit request with fresh revisions.

Do not silently rebase, fuzzily relocate, or partially apply a stale request.
Those behaviors can make a tool call appear successful while changing code the
model never observed. Exact failure plus a cheap read-and-retry is consistent
with Daemar's preference for on-demand operations over retaining more context.

## Token-efficiency rules

- Prefer structured data and short snippets over narrative wrappers.
- Do not echo normalized requests or newly supplied edit content on success.
- Default search context to zero lines; let the model request more.
- Include `size_bytes`, `total_lines`, revisions, and precise ranges because
  they directly support the next tool decision.
- Bound both item count and bytes. Item-only limits still permit one enormous
  item to flood context.
- Use explicit `truncated` plus an opaque continuation instead of cutting a
  string mid-item.
- Keep deterministic ordering across pages and bind continuations to the
  original observation.
- Return a bounded retry request with recoverable errors instead of a stack
  trace or generic failure code.
- Treat repeated small reads as normal. Do not add full files to the Context
  Surface merely to avoid a subsequent Model Tool call.

## V1 boundaries and follow-up

This ticket settles the provider-independent semantics needed for the Model
Tool and context-entry prototype. It deliberately does not settle:

- how these schemas are encoded in any provider's tool-definition format;
- provider tool-call identifiers, message roles, streaming, cancellation, or
  retry transport;
- HTTP, MCP, JSON-RPC, or SDK mappings;
- filesystem transaction implementation strategy;
- ignore-file policy configuration or numerical production limits;
- syntax-aware or language-server-assisted editing;
- shell, formatter, build, test, or validation tools.

PER-15 owns provider API transport. The Model Tool and context-entry prototype
can use these four names and shapes as its semantic input while choosing small
initial numeric bounds empirically.

## Sources

- John Yang et al., ["SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering"](https://papers.nips.cc/paper_files/paper/2024/file/5a7c947568c1b1328ccc5230172e1e7c-Paper-Conference.pdf), NeurIPS 2024.
- Chunqiu Steven Xia et al., ["Agentless: Demystifying LLM-based Software Engineering Agents"](https://lingming.cs.illinois.edu/publications/fse2025.pdf), FSE 2025.
- Anthropic, ["Writing effective tools for AI agents—using AI agents"](https://www.anthropic.com/engineering/writing-tools-for-agents), 2025.
- Google, [Gemini CLI tool reference](https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/tools.md) and the public [`read_file`](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/tools/read-file.ts), [`grep_search`](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/tools/grep.ts), and [`replace`](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/tools/edit.ts) implementations.
- OpenAI, [Codex `apply_patch` instructions](https://github.com/openai/codex/blob/main/codex-rs/core/prompt_with_apply_patch_instructions.md) and [implementation](https://github.com/openai/codex/blob/main/codex-rs/apply-patch/src/lib.rs).
- Aider, [SEARCH/REPLACE prompt contract](https://github.com/Aider-AI/aider/blob/main/aider/coders/editblock_prompts.py) and [implementation](https://github.com/Aider-AI/aider/blob/main/aider/coders/editblock_coder.py).
- Microsoft, [Language Server Protocol 3.17: `WorkspaceEdit`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspaceEdit).
