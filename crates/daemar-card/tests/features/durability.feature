# S1-B13…S1-B14 — The record survives the process (PER-82).
# Every CLI invocation is its own OS process: the process that
# acknowledged each write has already exited by the time a later process
# reads. That exit-then-read seam is this architecture's honest
# process-termination proof; a resident-daemon slice will owe a stronger
# kill-based one.
Feature: The record survives the process

  Scenario: Acknowledged entries survive restart, byte for byte
    # S1-B13 — identical IDs, provenance, timestamps, and payloads, not
    # just count and order (deep-review finding 6).
    Given an open Card
    And 3 appended decisions
    When the Card history is read twice in separate processes
    Then both reads return 4 identical entries in sequence order 1 through 4

  Scenario: The database path is discoverable and selectable
    # S1-B14
    Given an open Card
    Then the CLI reports the configured database path
    And pointing DAEMAR_DB at a different path uses that database

  Scenario: The --db flag outranks DAEMAR_DB
    # S1-B14 precedence (deep-review finding 6)
    When a Card titled "Flag target" is created with a --db override
    Then the override database holds exactly one Card titled "Flag target"
    And the environment database holds no Cards
    And db-path with the override reports source "flag"

  Scenario: Without configuration the record lands in a private factory home
    # S1-B14 default location + deep-review finding 4: isolated HOME, no
    # DAEMAR_DB, cwd elsewhere — the record lands in ~/.daemar/daemar.db,
    # owner-only on Unix regardless of umask.
    When a Card is created in an isolated factory home
    Then the database exists at .daemar/daemar.db inside that home
    And the factory home and database are private to the operator
