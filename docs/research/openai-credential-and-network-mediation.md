# OpenAI credential and network mediation

## Question

Which established pattern should Daemar use to mediate OpenAI Responses API
network access and credentials without granting unnecessary capabilities to
sandboxed, model-directed repository operations?

## Decision

Use a **trusted, per-Workflow-Run OpenAI Provider Broker** outside Sandboxed
Execution. The broker is a narrow provider client, not a general HTTP proxy:

- Only the broker can read the OpenAI credential and initiate external network
  connections.
- Sandboxed Execution receives neither a credential nor general network access.
- The broker accepts a versioned, strongly typed Responses request over a
  capability channel, validates it against Workflow policy, calls only
  `POST https://api.openai.com/v1/responses`, and returns typed provider events.
- Model Tool requests are forwarded to the sandboxed tool executor. The broker
  never reads or writes the worktree and never executes a Model Tool.
- The trusted coordinator records the exact validated request, provider stream
  and response, request identifiers, timing, usage, and structured failures in
  the Run Record. It never records the bearer credential.

This is an established privilege-separation pattern. Chromium describes a
privileged broker that owns policy, spawns sandboxed targets, and performs only
policy-allowed operations on their behalf over IPC. Apple describes XPC as a
way to centralize access to a shared resource and isolate privileges across
process boundaries. [Chromium sandbox design](https://chromium.googlesource.com/chromium/src/+/main/docs/design/sandbox.md#sandbox-windows-architecture),
[Apple XPC](https://developer.apple.com/documentation/xpc)

There is also a directly relevant OpenAI precedent: OpenAI's automation
guidance says not to put an API key in a job-level environment when
repository-controlled code can run, and its GitHub Action starts a Responses
API proxy to reduce key exposure. Daemar should adopt the separation, but use a
typed broker rather than expose a transparent HTTP surface.
[OpenAI non-interactive automation guidance](https://learn.chatgpt.com/docs/non-interactive-mode#use-api-key-auth),
[openai/codex-action](https://github.com/openai/codex-action)

## Why this pattern fits Daemar

The model loop and the repository tool executor need different capabilities.
The model loop needs one provider operation; repository navigation and editing
need one worktree. Giving both capability sets to one sandboxed process makes
the credential and general egress available to model-directed code. A broker
keeps those authorities separate while preserving one Workflow and one
Generation Stage across the trusted and sandboxed trust zones.

The boundary is also observable. The OpenAI API returns `x-request-id`, timing,
rate-limit, and project-token headers, and it accepts a caller-generated
`X-Client-Request-Id` for Responses requests. OpenAI recommends logging request
IDs in production. The broker is the one place that can reliably correlate
these headers with the exact outbound request and inbound stream, including
cases where a timeout prevents receipt of `x-request-id`.
[OpenAI API overview: debugging requests](https://developers.openai.com/api/reference/overview#debugging-requests)

## Alternatives compared

| Pattern | Credential exposure and security boundary | Network policy | Typed mediation and Run Record | Complexity and bootstrap cost | Decision |
| --- | --- | --- | --- | --- | --- |
| Direct sandbox egress with a project-scoped credential | The long-lived bearer credential is present in the process that handles model-directed inputs. A dedicated OpenAI project and service-account key reduce organizational blast radius, but do not prevent theft or misuse of that key. | macOS App Sandbox's client entitlement permits initiating outgoing connections; it does not express an allowlist for only the OpenAI host or path. | The client can record requests, but compromised sandboxed code can bypass or falsify the recorder. | Lowest initial implementation cost. | Reject for the first Workflow. It grants two capabilities the sandbox does not need: possession of the bearer secret and arbitrary client networking. |
| Generic local forward proxy | Keeps the key outside the sandbox only if the proxy injects it, but a CONNECT-capable or arbitrary-destination proxy becomes a broad exfiltration capability. | Requires a route from the sandbox to the proxy. With ordinary App Sandbox TCP, enabling client networking is broader than loopback-only intent. | HTTP is not the domain boundary; URL, method, headers, redirects, and body all require separate policy. Traffic capture can help, but semantic validation is bolted on. | Moderate; also creates proxy hardening, routing, and TLS concerns. | Reject. A general proxy is unnecessarily powerful. |
| Responses-only reverse proxy | Stronger than a forward proxy: the host can fix the upstream and inject auth. OpenAI uses this shape in `codex-action`. | Still depends on safely exposing only the local endpoint. OpenAI's action configures a loopback HTTP base URL; its own documentation notes that process-memory access can still expose the key, so process privilege separation remains necessary. | Can capture raw HTTP well, but a transparent request body is a weaker contract than Daemar's typed provider boundary. | Moderate and proven. | Acceptable only as an implementation detail if it validates the exact Responses surface and the sandbox substrate can constrain reachability. Do not make raw HTTP the Workflow contract. |
| Trusted typed provider broker | The credential exists only in a separate trusted process with no worktree capability. The sandbox receives only a capability to request the one provider operation. | Only the broker has external egress. The sandbox has no network capability; its IPC transport is selected with the sandbox substrate. | Best fit: validate typed requests before auth injection, emit typed stream/error events, and record unambiguous correlation data. | Moderate. Requires a small protocol and lifecycle, but avoids a general proxy and future policy retrofit. | **Select.** |
| macOS XPC provider service | XPC is Apple's native privilege-separation mechanism; each service can have its own sandbox and entitlements, and a well-defined protocol can mediate a shared resource. | The XPC service alone can hold the network entitlement. | Strong process and protocol boundary; interruption and invalidation are explicit. | Higher bootstrap cost for a Rust CLI because packaging, entitlements, code signing, and Rust-to-XPC integration must be settled. | Keep as a possible broker transport/hardening step. Do not require it before the sandbox-substrate decision. |
| Network Extension proxy or content filter | Can mediate system or per-app traffic, but operates at a much broader platform layer than one Workflow Run. | Powerful filtering and proxying, but Apple requires Network Extension capabilities and, for Developer ID distribution, provisioning and signing setup. | Network-oriented rather than provider-semantic; the Run Record still needs an application broker. | Highest bootstrap and deployment cost. | Reject for this slice. |

Apple documents that `com.apple.security.network.client` controls whether an app
may initiate outgoing connections, not which remote endpoint it may contact;
for TCP, it restricts initiation rather than the subsequent flow of data.
[Apple outgoing-network entitlement](https://developer.apple.com/documentation/BundleResources/Entitlements/com.apple.security.network.client)
Network Extension can implement app proxies and content filters, but its
entitlement and Developer ID provisioning requirements make it disproportionate
for a single local Workflow.
[Apple Network Extensions entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.networking.networkextension)

## First-Workflow contract

### Trust and capability placement

1. The trusted coordinator creates the branch, worktree, sandbox, Run Record,
   and per-run broker.
2. The broker is a separate trusted process. Its capability set is limited to:
   reading one OpenAI credential, connecting to the fixed OpenAI Responses
   endpoint, and communicating over the per-run broker channel. It has no
   worktree filesystem capability.
3. Sandboxed Execution has the worktree and registered Model Tools. It has no
   OpenAI credential, Keychain access, general network access, provider URL, or
   ability to select arbitrary HTTP headers.
4. The trusted coordinator owns the model/tool-loop state machine. It sends
   validated provider requests to the broker and routes model-issued tool calls
   to the sandboxed tool executor. Repository reads and changes therefore remain
   inside Sandboxed Execution.
5. Failure to establish either boundary fails the Workflow Run. There is no
   fallback that exposes the key or enables direct sandbox egress.

The broker should be treated as a security boundary, not merely a convenience
wrapper. Chromium's guidance explicitly treats sandboxed code as malicious for
threat modeling and makes the broker the policy enforcement point.
[Chromium sandbox principles](https://chromium.googlesource.com/chromium/src/+/main/docs/design/sandbox.md#principles)

### Credential handling

- Provision a dedicated OpenAI project and service account for Daemar's first
  Workflow rather than use a personal or organization-wide key. OpenAI supports
  project service accounts and returns a project-owned API key when one is
  created. [OpenAI CLI: project service account](https://developers.openai.com/api/docs/libraries/openai-cli#create-a-project-service-account-and-api-key)
- Apply project rate and spend limits as a second layer. OpenAI documents
  separate projects as an isolation boundary with custom rate and spend limits.
  [OpenAI production best practices](https://developers.openai.com/api/docs/guides/production-best-practices#staging-projects)
- Store the key in macOS Keychain and allow only the trusted broker packaging
  identity to retrieve it. Keychain Services stores small secrets in an
  encrypted database and supports controlling which macOS apps can access an
  item. [Apple Keychain Services](https://developer.apple.com/documentation/security/keychain-services)
- Never pass the key through the sandbox environment, command-line arguments,
  worktree files, IPC messages, or Run Record. Retrieve it at broker startup,
  hold it only in broker memory, redact `Authorization`, and discard it when the
  broker exits.
- Keychain storage is defense in depth, not the sandbox boundary. The selected
  sandbox substrate must independently prevent the sandboxed process from
  reading broker memory or invoking the broker's Keychain identity.

OpenAI's API reference treats API keys as secrets and says to load them from an
environment variable or key-management service on a server. Its safety guidance
says to revoke a key promptly if exposure is suspected.
[OpenAI API authentication](https://developers.openai.com/api/reference/overview#authentication),
[OpenAI key revocation guidance](https://developers.openai.com/api/docs/guides/safety-best-practices#revoke-compromised-api-keys)

### Broker request policy

The sandbox must not be able to supply a URL, HTTP method, bearer token, project
header, arbitrary header set, redirect policy, or proxy configuration. The
versioned broker request should contain only provider-semantic fields that the
first Workflow permits. The broker validates at least:

- Workflow Run ID and monotonically increasing provider-call sequence;
- the locked provider, Responses API operation, model, and reasoning effort;
- permitted input and Model Tool schemas;
- iteration, token, time, and cost budgets;
- request size and serialization limits; and
- the Workflow's storage and data-handling policy once that policy is settled.

After validation, the broker constructs the HTTP request, injects the bearer
credential, rejects redirects away from the fixed origin, and uses the platform
TLS trust policy. Additive OpenAI response fields and streaming event types must
be preserved as provider data even when the typed consumer does not yet
understand them; OpenAI explicitly treats those additions as backwards-compatible.
[OpenAI backwards compatibility](https://developers.openai.com/api/reference/overview#backwards-compatibility)

### Run Record fidelity

For every provider call, the trusted coordinator should append:

- the canonical validated request before secret injection;
- a unique `X-Client-Request-Id` derived from the Workflow Run ID and call
  sequence;
- send, first-byte, completion, timeout, and cancellation timestamps;
- HTTP status and non-secret response headers, including `x-request-id`,
  `openai-processing-ms`, and relevant rate-limit headers;
- every Responses streaming event or the complete non-streaming response, in
  arrival order;
- token usage and the broker's cost-accounting input;
- structured transport, authentication, rate-limit, provider, parse, policy,
  and IPC failures; and
- explicit redaction markers for omitted credentials rather than silent field
  removal.

The broker must not summarize or normalize away provider evidence needed to
reconstruct the exchange. Typed projections can be additional Run Record
events; they do not replace the provider payload.

### IPC and macOS implementation boundary

Select the concrete broker channel together with the macOS sandbox substrate.
Preferred channels are an inherited capability, a narrowly allowlisted Unix
domain socket, or XPC. Avoid granting general TCP client access solely to reach
a loopback proxy. The transport must be per-run, authenticated by possession or
OS identity, bounded in message size, closed when the run ends, and inaccessible
to unrelated local processes.

XPC is the macOS-native option if Daemar's packaging and sandbox choice make it
practical. Apple describes XPC services as independently sandboxed helpers for
privilege separation and provides explicit interruption and invalidation
semantics. [Apple: Creating XPC Services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html)

## Failure handling

- Missing or denied Keychain access: fail trusted setup before model work.
- Broker policy rejection: record the rejected field and policy rule; fail the
  Generation Stage.
- OpenAI authentication or permission failure: record the provider error and
  request IDs; do not substitute another credential.
- Rate limit, server error, timeout, dropped stream, broker crash, or IPC
  invalidation: record the last unambiguous event and fail according to the
  first Workflow's no-resume policy. Do not silently replay a request because a
  timed-out request may have reached OpenAI.
- Sandbox escape or suspected key exposure: terminate the run, revoke the key,
  and retain non-secret diagnostics.

Any retry policy, request idempotency strategy, or continuation after a partial
stream belongs to the Responses model-loop decision. This research only fixes
where authority and evidence live.

## Consequences and newly visible decisions

This decision removes provider networking and credentials from the sandbox
substrate requirements. The sandbox still needs a safe per-run capability
channel to the trusted coordinator or broker.

The following questions are now sharp enough to settle in related tickets:

1. Which IPC transport can the selected macOS sandbox substrate expose without
   also granting general local or external networking?
2. What is the exact versioned broker message protocol, including streaming,
   cancellation, size limits, forward-compatible provider events, and error
   taxonomy?
3. What deterministic retry policy is safe for ambiguous Responses API
   timeouts or partial streams?
4. What OpenAI response-storage and data-retention setting should the first
   Workflow enforce and record?
5. What packaging or signing identity is needed for stable broker-only Keychain
   access during the bootstrap phase?
