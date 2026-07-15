# Responses API model-loop contract

Research for [Specify the Responses API model-loop contract](https://linear.app/kwamddox-sedai/issue/PER-2/specify-the-responses-api-model-loop-contract), current as of 2026-07-15.

## Question and fixed scope

What exact OpenAI Responses API request, continuation, function-tool, usage, and response-item contracts should Daemar preserve for its first bounded model loop?

The settled scope is OpenAI's Responses API, model `gpt-5.6-sol`, reasoning effort `medium`, direct API or provider-SDK use, no third-party agent harness, no shell, and a closed set of strongly typed repository-navigation and structured-editing tools. Daemar must bound iterations, tokens, elapsed time, and cost, and must append every model request, model response, tool request, and tool result to its JSONL Run Record.

## Answer

Daemar should own a **stateless, manually replayed Responses loop**. Each turn sends `store: false`, the fixed instructions, the complete ordered Item history, and the same closed strict function-tool definitions. Daemar appends every returned output Item unchanged, executes at most one requested function, appends a `function_call_output` with the same `call_id`, and repeats until the model returns a terminal assistant message or a local/provider bound stops the loop.

This keeps the Run Record—not hidden provider state—as the reconstructable conversation record. It also preserves the opaque reasoning Items that GPT-5.6 can use across turns. OpenAI documents manual Item replay as a supported continuation strategy, requires reasoning Items returned alongside tool calls to be passed back, and returns encrypted reasoning content by default when `store` is false. [`previous_response_id` is an alternative, but it relies on stored provider state and does not carry top-level instructions forward](https://developers.openai.com/api/docs/guides/migrate-to-responses#update-multi-turn-conversations). [The stateless reasoning contract requires preserving every output Item and replaying the complete history](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-across-calls).

## Request contract

Every `POST /v1/responses` request in this first loop should explicitly carry:

| Field | First-loop contract |
| --- | --- |
| `model` | Exact value `gpt-5.6-sol`, not the moving `gpt-5.6` alias. The alias currently routes to Sol, but an exact identifier makes the recorded choice explicit. [Model guide](https://developers.openai.com/api/docs/guides/latest-model#update-api-and-model-parameters) |
| `reasoning` | `{ "effort": "medium", "context": "all_turns" }`. GPT-5.6 supports `medium`; `all_turns` makes compatible prior reasoning available when the complete history is replayed. The response's effective `reasoning.context` must also be recorded. [Model guide](https://developers.openai.com/api/docs/guides/latest-model#update-api-and-model-parameters), [reasoning continuity](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-across-calls) |
| `store` | `false`, making Daemar's local transcript authoritative and causing reasoning Items to include replayable encrypted content by default. [Reasoning continuity](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-across-calls) |
| `instructions` | The exact fixed Generation Stage instructions on **every** request. Instructions are a top-level field and are not inherited when using `previous_response_id`; resending them also makes each recorded request self-describing. [Responses migration guide](https://developers.openai.com/api/docs/guides/migrate-to-responses#update-multi-turn-conversations) |
| `input` | The complete ordered Item history. Initially this contains the deliberately admitted context and the Change Request. Later requests add every prior `response.output` Item, followed by Daemar-authored `function_call_output` Items. Never keep only messages: Responses also returns reasoning and function-call Items. [Responses migration guide](https://developers.openai.com/api/docs/guides/migrate-to-responses#map-messages-to-items), [function example](https://developers.openai.com/api/docs/guides/function-calling#function-tool-example) |
| `tools` | Only Daemar's closed repository-navigation and structured-editing functions. Each has `type: "function"`, a stable name and description, a JSON Schema `parameters` object, and `strict: true`. Strict schemas require every property to be required and every object to specify `additionalProperties: false`; model optionality is represented with a nullable type. [Strict mode](https://developers.openai.com/api/docs/guides/function-calling#strict-mode) |
| `tool_choice` | `"auto"`, permitting either a function request or a terminal message. No OpenAI built-in, MCP, hosted shell, apply-patch, web-search, code-interpreter, or custom free-form tool is included.
| `parallel_tool_calls` | `false`. OpenAI says this constrains a response to zero or one tool call, which makes tool authorization, transcript order, and iteration accounting unambiguous in the first loop. [Parallel function calling](https://developers.openai.com/api/docs/guides/function-calling#parallel-function-calling) |
| `max_output_tokens` | A required per-request configured limit. This bounds visible output **plus reasoning tokens**, not the whole multi-turn run. Reaching it produces `status: "incomplete"` with `incomplete_details.reason: "max_output_tokens"`, potentially before visible output appears. [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create), [reasoning token allocation](https://developers.openai.com/api/docs/guides/reasoning#allocating-space-for-reasoning) |
| `truncation` | `"disabled"`. The documented default rejects an over-context request rather than silently dropping Items from the beginning; that matches Daemar's deliberate-context principle. [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create) |
| `background`, `stream` | `false` for the first implementation. A single synchronous response envelope is the smallest auditable contract. Wall-clock deadlines still belong to Daemar's HTTP client and Workflow policy; they are not server-side Responses request fields.

Daemar should attach a unique `X-Client-Request-Id` to every HTTP attempt and preserve the returned `x-request-id`, `openai-processing-ms`, `openai-version`, organization, and rate-limit headers when present. OpenAI explicitly recommends logging request IDs; a caller-supplied ID also identifies a request that times out before the server request ID is received. Never record the `Authorization` header or API key. [OpenAI request-debugging reference](https://developers.openai.com/api/reference/overview#debugging-requests)

## Function Item contract

Responses uses typed Items rather than a messages-only transcript. Daemar must preserve the ordered `response.output` array verbatim before projecting known Items into its own typed events.

For this closed tool surface, the relevant provider output Item types are:

- `reasoning`: opaque model reasoning state. In stateless mode it includes `encrypted_content`; preserve and replay it unchanged. It is continuity state, not raw chain-of-thought text. [Reasoning continuity](https://developers.openai.com/api/docs/guides/reasoning#preserve-reasoning-across-calls)
- `function_call`: contains an Item `id`, `type`, `call_id`, function `name`, and JSON-encoded `arguments`. Treat `arguments` first as untrusted raw text, then JSON-decode and validate it against Daemar's canonical typed tool input before execution. A response may generally contain several calls, hence the explicit `parallel_tool_calls: false`. [Handling function calls](https://developers.openai.com/api/docs/guides/function-calling#handling-function-calls)
- `message`: an assistant Item whose content can include `output_text` or a `refusal`. Do not use an SDK's `output_text` convenience property as the evidence record; retain the complete message/content Items. Refusals are explicitly represented and must become a terminal non-success outcome. [Responses Item mapping](https://developers.openai.com/api/docs/guides/migrate-to-responses#map-messages-to-items), [refusal shape](https://developers.openai.com/api/docs/guides/structured-outputs#how-to-use-structured-outputs-with-textformat)

Daemar authors the continuation Item:

```json
{
  "type": "function_call_output",
  "call_id": "call_...",
  "output": "{\"ok\":true,\"value\":{...}}"
}
```

The `call_id` must exactly match the provider's function call. The API accepts a string whose internal format is application-defined, so Daemar should standardize one versioned JSON result envelope for both success and tool-level failure. An operation with no value still returns an explicit success or failure string. [Function-result contract](https://developers.openai.com/api/docs/guides/function-calling#handling-function-calls)

Unknown provider Item types or fields must not be discarded. Store the raw envelope and represent an unknown typed projection so a newer API response cannot silently disappear from the Run Record.

## Bounded loop

The provider's function-calling flow deliberately permits as many turns as the application allows, so all workflow-wide limits are Daemar responsibilities. [`max_output_tokens` is only per response](https://developers.openai.com/api/docs/guides/function-calling#the-tool-calling-flow).

The loop should be a small explicit state machine:

1. Before a model request, reject the next turn if its iteration, cumulative token, deadline, or conservative cost admission would exceed Workflow policy.
2. Append a request-attempt event, including exact JSON body, body digest, sanitized headers, iteration/attempt numbers, and client request ID.
3. Send the request with a deadline. Append the complete response body, body digest, HTTP status, selected response headers, timestamps, and elapsed time—even when the provider returns an error envelope.
4. Accept only a completed Response. Treat HTTP/API errors, `failed`, `incomplete`, `cancelled`, a refusal, malformed JSON, or an unsupported Item combination as typed terminal failures. Preserve `error` and `incomplete_details` unchanged.
5. Append every output Item to history in order. If there is one `function_call`, validate its name and arguments, execute the typed tool in the sandbox, record request/result evidence, append the matching `function_call_output`, and continue if all run-wide bounds still permit another request.
6. If there is no function call, require a completed assistant message as the final model response and end model execution successfully. Deterministic validation happens later in its own Workflow stage; it is not a model tool or repair loop.

Iteration count should mean **Responses requests**, while tool-call count is tracked separately. With parallel calls disabled, one request produces at most one tool call, but keeping both counters preserves the semantic distinction.

## Token and cost accounting

Preserve the full `response.usage` object rather than only `total_tokens`. The current envelope reports `input_tokens`, input-token details such as cached tokens, `output_tokens`, output-token details such as reasoning tokens, and `total_tokens`; GPT-5.6 guidance also calls out cache-write tokens. [Responses response example](https://developers.openai.com/api/reference/resources/responses/methods/create), [GPT-5.6 caching guidance](https://developers.openai.com/api/docs/guides/latest-model#update-api-and-model-parameters)

For pre-request admission, OpenAI exposes `POST /v1/responses/input_tokens`, which accepts a Responses-shaped input and returns an exact `input_tokens` count. Daemar can combine that count, the configured `max_output_tokens`, accumulated actual usage, and a conservative versioned price schedule to decide whether another request can fit. [Input-token counting reference](https://developers.openai.com/api/docs/guides/token-counting#api-reference)

The Responses envelope reports tokens, not dollars. Therefore each Run Record must identify the pricing schedule used for admission and accounting (source URL, captured/effective timestamp, model, service tier, long-context rule, and rates). Current `gpt-5.6-sol` standard pricing is listed per million tokens and has separate input, cached-input, output, cache-write, and long-context rules; these values can change and must not be compiled as timeless facts. [GPT-5.6 Sol model page](https://developers.openai.com/api/docs/models/gpt-5.6-sol)

A hard monetary ceiling cannot be derived solely from usage after the response. Before each request, Daemar must reserve a worst-case charge for the exact counted input plus `max_output_tokens`, using no cache discount unless the request contract can prove it. Release the reservation and replace it with actual charge after a complete usage envelope arrives. A transport timeout without a usage envelope is an **ambiguous charge**, not zero cost.

## JSONL Run Record evidence

The JSONL file should keep immutable provider evidence and Daemar's typed interpretation separate. At minimum, append these event kinds:

- `model_request_started`: run/iteration/attempt IDs, endpoint, exact body and digest, sanitized headers, `X-Client-Request-Id`, configured bounds, timestamp, and price-schedule identity.
- `model_response_received`: HTTP status, selected headers including `x-request-id`, exact body and digest, receive timestamp, elapsed time, and raw usage.
- `model_response_item`: response ID, output index, raw Item, known/unknown projection, and whether it was admitted to continuation history.
- `model_tool_requested`: response/item/call IDs, tool name, raw arguments string, decoded JSON if possible, schema/tool version, and validation outcome.
- `model_tool_result`: call ID, tool version, structured success/failure result, exact serialized `function_call_output`, elapsed time, and sandbox policy result.
- `model_loop_stopped`: typed reason (`completed`, local bound, provider incomplete/error, refusal, transport ambiguity, unsupported contract, or tool failure) plus cumulative iterations, calls, usage, reserved/actual cost, and elapsed time.

The exact provider payload is evidence; the normalized event is an index over it. Redaction rules must be field-aware and explicit. They may remove credentials but must never silently summarize or omit admitted model context, model output, tool arguments, tool results, reasoning ciphertext, or usage.

## Decisions still required

This research settles the API-facing shape but exposes three implementation decisions that should not be guessed here:

1. **Choose numeric loop budgets and pricing-refresh policy:** per-request output cap, total iterations/tool calls/tokens/time/cost, long-context admission, and what happens when a price schedule is stale.
2. **Choose the final assistant-response contract:** plain text is sufficient for transcript evidence, but a strict `text.format` JSON Schema may be preferable if downstream Workflow stages consume a typed summary. This is separate from the code changes, which occur only through editing tools.
3. **Choose transport retry and ambiguous-outcome policy:** no Responses-specific idempotency guarantee was found in the official contract reviewed here. A timeout can leave receipt, usage, and charge unknown; retries must be explicitly modeled as new attempts and cannot be assumed free or duplicate-safe.

These are precise new decisions. They should be routed into Wayfinder rather than hidden inside the provider adapter or Run Record implementation.

## Primary sources

- [OpenAI: Using GPT-5.6](https://developers.openai.com/api/docs/guides/latest-model)
- [OpenAI: Function calling](https://developers.openai.com/api/docs/guides/function-calling)
- [OpenAI: Reasoning models](https://developers.openai.com/api/docs/guides/reasoning)
- [OpenAI: Migrate to the Responses API](https://developers.openai.com/api/docs/guides/migrate-to-responses)
- [OpenAI: Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [OpenAI: Count input tokens](https://developers.openai.com/api/docs/guides/token-counting#api-reference)
- [OpenAI: API request debugging](https://developers.openai.com/api/reference/overview#debugging-requests)
- [OpenAI: GPT-5.6 Sol model and pricing](https://developers.openai.com/api/docs/models/gpt-5.6-sol)
