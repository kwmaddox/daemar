# S2-B1…S2-B4 — Agents record stage events on the Card (PER-83).
# The stage event is deliberately structure-light: stages are agent-chosen
# free-form strings, payloads are arbitrary JSON objects recorded verbatim,
# and the gate judges form only — no stage vocabulary, no payload schema.
# The entry-type working name "stage-event" is confirmed at skeleton review.
Feature: Agents record stage events on the Card

  Background:
    Given an open Card

  Scenario: A producer records a stage event
    # S2-B1 — exactly three event-content fields riding the slice-1 envelope.
    When producer "claude" of kind "agent" records a stage event with stage "gherkin" and summary "Behavior spec written and red" and payload:
      """
      {"features": 4, "scenarios": {"count": 21}}
      """
    Then the entry is accepted at sequence 2
    And the recorded entry carries type "stage-event", producer "claude" of kind "agent", a schema version, an entry ID, and a server-assigned timestamp
    And the recorded stage is "gherkin" and the recorded summary is "Behavior spec written and red"
    And the recorded payload is exactly the submitted JSON

  Scenario: The payload is optional
    # S2-B1 — stage and summary are the only required event content.
    When producer "codex" of kind "agent" records a stage event with stage "triage" and summary "No findings to carry" and no payload
    Then the entry is accepted at sequence 2
    And the recorded event carries no payload

  Scenario: Stage names are not judged
    # S2-B2 — free-form discovery: real stage names emerge from dogfood
    # use; the gate accepts any non-blank stage verbatim.
    When producer "claude" of kind "agent" records a stage event with stage "review / adversarial refutation ✈" and summary "Round two clean" and no payload
    Then the entry is accepted at sequence 2
    And the recorded stage is "review / adversarial refutation ✈" and the recorded summary is "Round two clean"

  Scenario Outline: Malformed stage events are rejected whole
    # S2-B2 — form-only gate: structured validation error, no partial
    # append, nothing normalized. Equivalence classes are spelled out:
    # bare CR and bare LF each reject on their own, and every non-object
    # JSON payload variant rejects — not just arrays. Duplicate JSON
    # member names are rejected at any depth because silent
    # last-member-wins parsing would falsify "recorded verbatim". Size
    # limits stay deferred until dogfooding.
    Given the Card has 3 entries
    When a producer submits a stage event that is <defect>
    Then the command fails with error category "validation"
    And the Card still has exactly 3 entries

    Examples:
      | defect                                                    |
      | carrying a blank stage                                    |
      | carrying a whitespace-only stage                          |
      | carrying a blank summary                                  |
      | carrying a whitespace-only summary                        |
      | carrying a summary containing a bare carriage return      |
      | carrying a summary containing a bare line feed            |
      | carrying a payload that is a JSON array, not an object    |
      | carrying a payload that is a JSON string, not an object   |
      | carrying a payload that is a JSON number, not an object   |
      | carrying a payload that is a JSON boolean, not an object  |
      | carrying a payload that is JSON null, not an object       |
      | carrying a payload with duplicate member names            |
      | carrying a payload with duplicate member names nested deep |
      | carrying a payload with duplicate member names followed by a later member |
      | carrying a nested duplicate with a later sibling after the nested object |
      | of an unknown schema version                              |

  Scenario: A large-integer payload value survives verbatim
    # S2-B1 boundary (operator-review finding): "recorded verbatim" is
    # value fidelity, not best-effort parsing — a 64-bit-plus integer
    # must not collapse to a nearby float between ingest and readback.
    When producer "claude" of kind "agent" records a stage event with stage "measure" and summary "Large counter recorded" and payload:
      """
      {"n": 18446744073709551617}
      """
    Then the entry is accepted at sequence 2
    And the Card history text contains "18446744073709551617"

  Scenario: Changing only a large integer under a reused key is a conflict
    # S2-B3 boundary (operator-review finding): 2^64 and 2^64 + 1 are
    # different content; a fingerprint that rounds them together turns a
    # forbidden silent replay into "idempotency".
    Given producer "claude" of kind "agent" recorded a stage event with idempotency key "stage-k3" and a large-integer payload
    When the same stage event is retried with idempotency key "stage-k3" and the large integer incremented
    Then the command fails with error category "conflict"
    And the Card has exactly 2 entries

  Scenario: Retrying a stage event after a lost response
    # S2-B3
    Given producer "claude" of kind "agent" recorded a stage event with idempotency key "stage-k1"
    When the same stage event is retried with idempotency key "stage-k1"
    Then the command succeeds and returns the same entry ID and sequence as before
    And the Card has exactly 2 entries

  Scenario: Reusing a stage-event idempotency key with different content
    # S2-B3 boundary (review finding): the shared idempotency fingerprint
    # must cover the new event content — one changed nested payload value
    # is a conflict, never a silent return of the original.
    Given producer "claude" of kind "agent" recorded a stage event with idempotency key "stage-k2"
    When the same stage event is retried with idempotency key "stage-k2" and a changed nested payload value
    Then the command fails with error category "conflict"
    And the Card has exactly 2 entries

  Scenario: Stage events share the Card's single ordered stream
    # S2-B4 — no side channel: stage events interleave with card-created
    # and decision entries under one sequence, and the history filter
    # selects them by entry type.
    Given producer "claude" of kind "agent" appended a decision with idempotency key "mix-d1"
    And producer "claude" of kind "agent" recorded a stage event with idempotency key "mix-s1"
    And producer "codex" of kind "agent" appended a decision with idempotency key "mix-d2"
    When the Card history is read
    Then 4 entries return in sequence order 1 through 4
    And the entry types read "card-created", "decision", "stage-event", "decision" in that order
    When the Card history is read filtered to entry type "stage-event"
    Then 1 entry returns, all of type "stage-event", in sequence order
