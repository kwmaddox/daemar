# S1-B5…S1-B10 — Append workflow decisions with provenance (PER-82).
# Opening a Card appends the card-created entry at sequence 1, so the
# first decision on a fresh Card lands at sequence 2.
Feature: Append workflow decisions with provenance

  Background:
    Given an open Card

  Scenario: A producer appends a workflow decision
    # S1-B5
    When producer "claude" of kind "agent" appends a decision with summary "Slice 1 is CLI-only" and reason "the console needs true data first"
    Then the entry is accepted at sequence 2
    And the recorded entry carries type "decision", producer "claude" of kind "agent", a schema version, an entry ID, and a server-assigned timestamp
    And the recorded payload has summary "Slice 1 is CLI-only" and reason "the console needs true data first"

  Scenario: Cards sequence independently
    # S1-B6
    Given a second open Card
    When decisions are appended alternately to both Cards three times
    Then the first Card's entries read sequences 1 through 4
    And the second Card's entries read sequences 1 through 4

  Scenario: Retrying an append after a lost response
    # S1-B7
    Given producer "claude" of kind "agent" appended a decision with idempotency key "append-k9"
    When the same append is retried with idempotency key "append-k9"
    Then the command succeeds and returns the same entry ID and sequence as before
    And the Card has exactly 2 entries

  Scenario Outline: Invalid appends leave no trace
    # S1-B8
    Given the Card has 3 entries
    When a producer submits an append that is <defect>
    Then the command fails with error category "<category>"
    And the Card still has exactly 3 entries

    Examples:
      | defect                                    | category   |
      | missing the decision reason               | validation |
      | missing producer identity                 | validation |
      | of an unknown entry type                  | validation |
      | of an unknown schema version              | validation |
      | addressed to a nonexistent Card           | missing    |
      | of the reserved card-created type         | validation |
      | carrying a blank producer identity        | validation |

  Scenario: Blank required decision fields are rejected
    # Deep review: required workflow content cannot be whitespace.
    When producer "claude" of kind "agent" appends a decision with summary "ok" and reason "   "
    Then the command fails with error category "validation"
    And the Card still has exactly 1 entries

  Scenario: Concurrent producers get one durable order
    # S1-B9
    When two producers append decisions concurrently
    Then both appends succeed with distinct consecutive sequences
    And the recorded history lists both entries in sequence order

  Scenario: History cannot be edited
    # S1-B10
    Then the CLI offers no command that updates or deletes an entry
