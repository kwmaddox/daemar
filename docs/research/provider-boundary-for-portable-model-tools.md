# Provider boundary for portable Model Tools

- **Date:** 2026-07-16
- **Wayfinder ticket:** [Research the provider boundary for portable Model Tools](https://linear.app/kwamddox-sedai/issue/PER-15/research-the-provider-boundary-for-portable-model-tools)
- **Scope:** Decision input for an OpenAI Responses-only V1. Anthropic and Ollama are comparison targets, not V1 execution targets.

## Answer

Daemar should make the **meaning of a Model Tool** provider-independent, but it should not invent a provider-independent tool-calling wire protocol.

The portable semantic contract should say what operation the model can request: its stable identity, model-facing purpose and behavioral guidance, typed input contract, and the meaning of successful and failed outcomes. Daemar should validate that contract at its own execution boundary. A provider adapter should own how those semantics are represented on a particular API: declaration shape, schema dialect and strictness controls, call correlation, message or item types, result encoding, streaming assembly, tool-selection controls, parallel-call controls, and provider-hosted tools.

Provider capabilities should not be collapsed into a lowest-common-denominator transport. Each adapter should preserve supported provider-native features explicitly, reject required features it cannot represent, and keep provider-native events available for the Run Record. Portability means that the same Daemar operation keeps the same meaning across providers; it does **not** mean that all providers expose the same controls or response shape.

For V1, this boundary is implemented only for OpenAI Responses. This research does not justify an Anthropic or Ollama execution path, a provider registry, or a generic multi-provider framework.

## The boundary

### Provider-independent Model Tool semantics

The semantic contract belongs to Daemar and should remain stable when the transport changes:

1. **Identity:** a stable Daemar tool identity and a model-visible name that an adapter can validate against its provider's naming rules.
2. **Purpose and behavior:** what the tool does, when it should and should not be used, important preconditions, side effects, and limitations. Providers consistently treat the name, description, and parameter descriptions as information presented to the model, even though they encode that information differently. OpenAI describes function definitions as model-visible names, usage descriptions, and JSON Schema parameters; Anthropic says its API builds a special system prompt from the tool definitions and configuration; Ollama accepts the same semantic trio inside its native nested `function` object. ([OpenAI function definitions](https://developers.openai.com/api/docs/guides/function-calling#defining-functions), [Anthropic tool definitions](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools), [Ollama tool calling](https://docs.ollama.com/capabilities/tool-calling))
3. **Typed input:** a Daemar-owned input type and constraints, with a JSON Schema representation drawn from an intentionally supported profile. Provider-side constrained generation is useful but is not Daemar's execution-time validator: OpenAI and Anthropic each document provider-specific strict modes and supported-schema limitations, while Ollama's native Chat API documents a `parameters` schema without a corresponding strictness field. Daemar must parse and validate the call before dispatch regardless of provider claims. ([OpenAI strict mode](https://developers.openai.com/api/docs/guides/function-calling#strict-mode), [Anthropic strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use), [Ollama Chat API](https://docs.ollama.com/api/chat))
4. **Typed outcome meaning:** the operation's success payload, expected negative result, and execution failure remain distinct inside Daemar. The semantic result presented back to the model should keep the same meaning even when an adapter must encode it as a free-form output string, a content block with an error flag, or a tool-role message.
5. **Execution facts that affect correct use:** for example, whether an operation mutates the worktree and what evidence it returns. These facts are part of the model-facing description and typed handler contract, not properties inferred from a provider's transport.

This semantic contract does not contain provider response IDs, content-block roles, streaming offsets, or SDK types.

### Workflow and execution policy

The Workflow Definition, rather than each Model Tool, owns which tools are exposed for a request and the allowed loop behavior. Selection policy, whether parallel calls are permitted, iteration and cost limits, and the decision to stop or continue are request/orchestration concerns. Providers offer different knobs for some of these policies, so the concrete adapter lowers the Workflow's choice when the provider can enforce it and Daemar enforces the policy itself in all cases.

Tool authorization, sandbox capabilities, handler dispatch, input validation, and typed Rust error conversion are also Daemar execution concerns. They must not be delegated to a model provider's `strict` setting or confused with a successful HTTP response.

### Provider-native transport

An explicit provider adapter owns:

- lowering a Daemar tool declaration into the provider's request shape;
- checking whether the provider can faithfully represent every required schema or result capability;
- parsing zero, one, or several provider-native calls;
- retaining the provider's opaque correlation value for the lifetime of each call;
- translating a typed Daemar outcome into the provider's result envelope without erasing success versus failure;
- preserving required conversational state, including provider-specific reasoning or thinking items;
- assembling streaming deltas into complete calls before execution;
- applying provider-native tool choice, parallelism, strictness, caching, or deferred-loading controls when selected by Workflow policy; and
- recording raw provider request/response items alongside normalized semantic Model Tool events.

Provider-hosted tools are a separate category. OpenAI distinguishes application-executed function tools from built-in and custom tools, and Anthropic distinguishes client-executed tools from server-executed tools with their own server-side loop. Those tools have different trust, mediation, and result-handling boundaries and should not be disguised as portable Daemar Model Tools. ([OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling), [Anthropic tool-use contract](https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works), [Anthropic server tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/server-tools))

## What the provider specifications actually share

The providers share the abstract loop: declare callable operations, receive a structured call, execute client code, return a result, and let the model continue. They do not share a transport contract.

| Concern | OpenAI Responses | Anthropic Messages | Ollama native Chat |
| --- | --- | --- | --- |
| Declaration | Flat function tool with `type`, `name`, `description`, `parameters`, and `strict` | Tool with `name`, `description`, `input_schema`, plus optional provider features | Nested `type: function` / `function: { name, description, parameters }` |
| Model call | `function_call` output item; arguments are a JSON-encoded string | `tool_use` content block; input is a JSON object | Assistant `tool_calls`; arguments are an object and examples include an index |
| Correlation | `call_id` is returned in `function_call_output` | `tool_use.id` is returned as `tool_result.tool_use_id` | Native examples return results as ordered `tool` messages identified by `tool_name`; calls may carry an index |
| Result | `function_call_output` with provider `call_id`; output is normally an application-defined string and may also contain supported image/file input objects | User `tool_result` block with `tool_use_id`, content blocks, and optional `is_error` | `role: tool` message with `tool_name` and string content in the documented examples |
| Selection and parallelism | `tool_choice` supports auto, required, forced, none, and allowed subsets; `parallel_tool_calls` can prevent multiple calls | `tool_choice` supports auto, any, a named tool, or none; `disable_parallel_tool_use` changes the maximum according to mode | The native guide documents single, parallel, multi-turn, and streamed calls; the native Chat reference does not document `tool_choice` or a strict-schema switch |

Sources: [OpenAI handling function calls](https://developers.openai.com/api/docs/guides/function-calling#handling-function-calls), [OpenAI tool choice](https://developers.openai.com/api/docs/guides/function-calling#tool-choice), [OpenAI parallel function calling](https://developers.openai.com/api/docs/guides/function-calling#parallel-function-calling), [OpenAI Responses API reference](https://developers.openai.com/api/docs/api-reference/responses/create), [Anthropic handle tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [Anthropic parallel tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/parallel-tool-use), [Anthropic Messages API](https://platform.claude.com/docs/en/api/messages/create), [Ollama tool calling](https://docs.ollama.com/capabilities/tool-calling), [Ollama Chat API](https://docs.ollama.com/api/chat).

These differences are not cosmetic:

- OpenAI returns function arguments as encoded JSON text, while Anthropic and Ollama expose argument objects in their documented client shapes. Parsing belongs in the adapter; typed validation belongs in Daemar. ([OpenAI handling function calls](https://developers.openai.com/api/docs/guides/function-calling#handling-function-calls), [Anthropic handle tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [Ollama tool calling](https://docs.ollama.com/capabilities/tool-calling))
- Anthropic has an explicit `is_error` signal and rich result content blocks. OpenAI deliberately leaves the usual function-result string format to the application, including JSON, error codes, or plain text. Ollama's native examples return string content under a tool role. Therefore success/error meaning must be typed before transport, and each adapter must render that distinction deliberately. ([Anthropic handle tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [OpenAI formatting results](https://developers.openai.com/api/docs/guides/function-calling#formatting-results), [Ollama tool calling](https://docs.ollama.com/capabilities/tool-calling))
- Conversation continuation is provider-specific. OpenAI requires reasoning items returned with tool calls to be passed back for reasoning models; Anthropic requires each client `tool_result` to immediately follow its `tool_use` in a user content block and imposes ordering rules; Ollama's streaming guide requires accumulated thinking, content, and tool calls to be returned together with tool results. These are adapter state machines, not Model Tool semantics. ([OpenAI function tool example](https://developers.openai.com/api/docs/guides/function-calling#function-tool-example), [Anthropic handle tool calls](https://platform.claude.com/docs/en/agents-and-tools/tool-use/handle-tool-calls), [Ollama streaming](https://docs.ollama.com/capabilities/streaming))
- Provider controls are not interchangeable. For example, OpenAI's allowed-tool subset, Anthropic's `input_examples`, `allowed_callers`, `defer_loading`, and server-tool loop, and Ollama's `think` and local runtime options do not have one natural portable field. ([OpenAI tool choice](https://developers.openai.com/api/docs/guides/function-calling#tool-choice), [Anthropic tool reference](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-reference), [Ollama Chat API](https://docs.ollama.com/api/chat))

## Preserve capabilities; do not design to the LCD

A lowest-common-denominator request/response struct would make the easy fields look uniform while hiding the behavior Daemar must reason about. It would either discard explicit Anthropic errors and content types, pretend Ollama's native correlation shape is the same as OpenAI's, or prevent useful OpenAI controls from being represented. It would also make the Run Record less trustworthy by translating away provider evidence.

Instead, a future adapter should use three rules:

1. **Portable requirements are explicit.** A Model Tool declares only semantic requirements that must survive transport. An adapter either proves it can lower them or returns a typed unsupported-capability error before making a provider request. It must not silently weaken a schema or outcome.
2. **Provider extensions are namespaced and typed.** Native controls remain on an OpenAI-, Anthropic-, or Ollama-specific request configuration rather than being added as misleading optional fields on every Model Tool.
3. **Both views are recorded.** The Run Record keeps the normalized semantic invocation/outcome used by Daemar and the provider-native request/response evidence, including opaque call identifiers and untouched provider state needed for continuation.

Ollama's compatibility APIs reinforce this rule rather than eliminating it. Ollama documents partial OpenAI compatibility, including `/v1/responses` tool calling, but says that Responses support is non-stateful and lists a bounded set of supported fields. It also exposes a separate native `/api/chat` transport and an Anthropic-compatible endpoint. A compatible surface therefore identifies an encoding option, not full behavioral parity; Daemar should still identify the actual provider and check its supported capability set. ([Ollama OpenAI compatibility](https://docs.ollama.com/api/openai-compatibility), [Ollama Anthropic compatibility](https://docs.ollama.com/api/anthropic-compatibility), [Ollama API introduction](https://docs.ollama.com/api/introduction))

## V1 consequence

V1 should have one concrete OpenAI Responses integration that:

- lowers the Daemar Model Tool semantic definitions into Responses function tools;
- uses provider-native Responses items and opaque `call_id` values in the loop;
- independently parses and validates every generated argument value before dispatch;
- converts typed tool success and failure into an explicit, documented model-facing result representation;
- preserves all response items needed for continuation and records both raw and semantic events; and
- applies the Workflow's tool-choice and parallel-call policy using Responses controls plus Daemar-side enforcement.

The portable seam may be represented by Daemar's typed Model Tool definitions and outcomes, but V1 does not need a generic provider trait, provider registry, capability-negotiation framework, or implementations for Anthropic and Ollama. Those providers establish design pressure for keeping Responses DTOs out of the semantic Model Tool contract; they do not expand the executable V1 scope.

## Decisions still left to the contract prototype

This research establishes the ownership boundary but intentionally does not settle:

- the exact JSON Schema profile Daemar will accept for V1 Model Tool inputs;
- the exact model-facing success/error result envelope for the OpenAI Responses adapter;
- whether the first Generation Stage permits parallel Model Tool calls; or
- the concrete Rust types and function signatures for tool registration, dispatch, and Run Record events.

Those choices can be made against the actual repository-navigation and structured-editing Model Tools without changing the boundary above.
