# S1-B13…S1-B14 — The record survives the process (PER-82).
# Every CLI invocation is its own OS process, so "a fresh process" is the
# restart seam this slice can honestly test. The default database
# location under the factory home is a review concern, not an e2e one:
# these tests always pin DAEMAR_DB to a scenario-local path.
Feature: The record survives the process

  Scenario: Acknowledged entries survive restart
    # S1-B13
    Given an open Card
    And 3 appended decisions
    When the Card history is read in a fresh process
    Then 4 entries return in sequence order 1 through 4

  Scenario: The database path is discoverable and selectable
    # S1-B14
    Given an open Card
    Then the CLI reports the configured database path
    And pointing DAEMAR_DB at a different path uses that database
