# S1-B1…S1-B4 — Open a Card for a real task (PER-82).
# The machine-facing success contract (S1-B12) is asserted implicitly:
# every step parses the CLI's JSON output.
Feature: Open a Card for a real task

  Scenario: Creating a Card assigns durable identity
    # S1-B1
    When a producer creates a Card titled "Build the append boundary" with task key "PER-90" and workspace "github/daemar"
    Then the command succeeds and returns a Card ID
    And the Card ID is not "PER-90"
    And the Card list shows that Card with title "Build the append boundary" and task key "PER-90"

  Scenario: A Card without external references
    # S1-B2
    When a producer creates a Card titled "Untethered work"
    Then the command succeeds and returns a Card ID
    And the listed Card's task key and workspace read as absent

  Scenario: A blank title is rejected
    # Deep review: required workflow content cannot be whitespace.
    When a producer creates a Card titled "   "
    Then the command fails with error category "validation"
    And no Card exists

  Scenario: Retrying a create after a lost response
    # S1-B3
    Given a Card was created with idempotency key "create-k1" and title "Once only"
    When a producer creates a Card with idempotency key "create-k1" and title "Once only"
    Then the command succeeds and returns the same Card ID as before
    And exactly one Card exists

  Scenario: Reusing a create idempotency key with different content
    # S1-B4
    Given a Card was created with idempotency key "create-k1" and title "Original"
    When a producer creates a Card with idempotency key "create-k1" and title "Different"
    Then the command fails with error category "conflict"
    And exactly one Card exists
