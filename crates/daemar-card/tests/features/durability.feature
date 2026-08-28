# S1-B13…S1-B14 — The record survives the process (PER-82).
Feature: The record survives the process

  Scenario: Acknowledged entries survive restart, byte for byte
    # S1-B13 — identical IDs, provenance, timestamps, and payloads, not
    # just count and order (deep-review finding 6).
    Given an open Card
    And 3 appended decisions
    When the Card history is read twice in separate processes
    Then both reads return 4 identical entries in sequence order 1 through 4

  Scenario: Entries acknowledged by killed writers survive
    # S1-B13, forced termination: each writer is SIGKILLed the moment its
    # acknowledgement is readable, denying it an orderly shutdown,
    # connection close, or final checkpoint. An individual writer may win
    # the race and exit first, but the scenario fails unless at least one
    # was verifiably terminated by the signal — otherwise it would prove
    # nothing about forced termination. Reopening must show every
    # acknowledged entry exactly as acknowledged.
    Given an open Card
    When 5 appends are acknowledged and their writers killed immediately
    Then at least one writer was terminated by SIGKILL
    And history holds exactly those acknowledged entries at sequences 2 through 6

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

  Scenario: A dotenv-selected database keeps its directory untouched
    # Deep review: a dotenv-selected path is operator-selected exactly
    # like Env/Flag — the factory must not create or tighten an external
    # parent directory it does not own.
    When a Card is created via a factory-home dotenv pointing at an external directory
    Then the external directory keeps its permissions
    And the dotenv database holds exactly one Card

  Scenario: A broken dotenv file fails loudly instead of splitting the record
    # Deep review: silently falling back to the default database would
    # split the durable Card record across two stores.
    When a Card creation is attempted with a malformed factory-home dotenv
    Then the command fails with error category "storage"
    And no default database was created in that factory home

  Scenario: Without configuration the record lands in a private factory home
    # S1-B14 default location + deep-review finding 4: isolated HOME, no
    # DAEMAR_DB, cwd elsewhere — the record lands in ~/.daemar/daemar.db,
    # owner-only on Unix regardless of umask.
    When a Card is created in an isolated factory home
    Then the database exists at .daemar/daemar.db inside that home
    And the factory home and database are private to the operator
