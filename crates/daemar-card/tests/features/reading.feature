# S1-B11…S1-B12 — Read the ordered Card record (PER-82).
# The success half of S1-B12 is asserted implicitly by every step that
# parses stdout JSON; the failure taxonomy is asserted here and by the
# category checks in the other features.
Feature: Read the ordered Card record

  Background:
    Given an open Card
    And 4 appended decisions

  Scenario: Reading history returns ordered complete envelopes
    # S1-B11
    When the Card history is read
    Then 5 entries return in sequence order 1 through 5
    And every entry carries a complete envelope

  Scenario: Filtering history by entry type
    # S1-B11
    When the Card history is read filtered to entry type "decision"
    Then 4 entries return, all of type "decision", in sequence order

  Scenario: Failures are structured for machines
    # S1-B12
    When history is read for a nonexistent Card
    Then the command fails with error category "missing"
