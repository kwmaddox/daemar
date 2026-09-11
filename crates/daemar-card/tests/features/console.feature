# S3-B1…S3-B9 — The operator reads Cards on a local Queue First console
# (PER-84). `card serve` is a loopback-only HTTP server in the `card`
# binary serving one server-rendered, script-free HTML screen: the Card
# queue on the left, the selected Card's raw stream on the right, an entry
# inspector one click away. Nothing is derived, nothing is written.
#
# Oracle rules (PER-83 record) bind every step here: a rendered field is
# compared to the specific durable entry it claims to render, keyed by
# entry ID and sequence, never counted; hostile content is planted per
# field and traced to every sink; the loopback bind is proved by the
# refused non-loopback connection and the 421, never by the reported URL
# alone. The structural halves of S3-B2 and S3-B8 (loopback-only address
# type, read-only capability) are established at skeleton and conformance
# review and are not asserted by source-text tests.
#
# Every scenario spawns `card serve --port 0` against the scenario-local
# database and drives it over HTTP with a parsed-DOM oracle.
Feature: The operator reads Cards on a local Queue First console

  # ---------------------------------------------------------------- S3-B1

  Scenario: The console is discoverable from the CLI
    # S3-B1 — discoverability is read from `card --help`, not assumed.
    When the CLI is invoked with arguments "--help"
    Then the command succeeds
    And the help text lists the subcommand "serve"

  Scenario: Starting the console reports where to look
    # S3-B1 — one JSON startup line: URL, resolved database path, source
    # in the `card db-path` vocabulary.
    Given an open Card
    When the console is started with "--port 0"
    Then the startup line is one JSON object carrying "url", "db_path", and "source"
    And the startup line's "db_path" is the scenario database path
    And the startup line's "source" is "env"
    And the startup line's "url" names a loopback host and a nonzero port

  Scenario: A port that cannot be bound is a structured failure
    # S3-B1 — never a silent fallback to another port.
    Given an open Card
    And a socket already bound on a loopback port
    When the console is started on that port
    Then the console exits with error category "unavailable"
    And the error names the port
    And no startup line was printed

  # ---------------------------------------------------------------- S3-B2

  Scenario: The console listens on loopback only
    # S3-B2 — black-box half of the loopback seam. The reported host is a
    # claim; the refused non-loopback connection is the evidence.
    Given an open Card
    And the console is running
    Then the startup line's host is a loopback address
    And a connection to the console's port through a non-loopback interface address is refused
    And a connection to the console's port through loopback succeeds

  Scenario Outline: The console answers its own loopback authorities
    # S3-B2 — the bound loopback authority in each spelling, with the
    # bound port, is served.
    Given an open Card
    And the console is running
    When the queue page is requested with Host "<host>" and the bound port
    Then the response status is 200

    Examples:
      | host      |
      | 127.0.0.1 |
      | localhost |
      | [::1]     |

  Scenario Outline: A foreign Host authority is rejected before any handler runs
    # S3-B2 — DNS rebinding defense (RFC 9110 §7.4): the rejection carries
    # no queue, no Card, and no error page body that reads storage. The
    # <empty> row sends a present-but-empty Host field; a missing Host is
    # a different case (RFC 9112 §3.2) and is not asserted here.
    Given an open Card
    And the console is running
    When the queue page is requested with Host "<host>"
    Then the response status is 421
    And the response body renders no queue and no Card content

    Examples:
      | host                         |
      | evil.example                 |
      | evil.example:<bound port>    |
      | 127.0.0.1:1                  |
      | 127.0.0.1.evil.example       |
      | <empty>                      |

  # ---------------------------------------------------------------- S3-B3

  Scenario: The console resolves the database like every other subcommand
    # S3-B3 — S1-B14 precedence: --db outranks DAEMAR_DB.
    Given an open Card in the flag database
    And an open Card in the environment database
    When the console is started with "--db" naming the flag database
    Then the startup line's "db_path" is the flag database path
    And the startup line's "source" is "flag"
    And the queue shows exactly the Cards of the flag database

  Scenario: DAEMAR_DB outranks the factory-home dotenv for the console
    # S3-B3 — S1-B14 precedence, review finding G3: the shared
    # suite never put env and dotenv in direct conflict. Two
    # distinguishable databases; path, source, and content all resolve
    # to the environment one.
    Given an open Card titled "Env target" in the environment database
    And an open Card titled "Dotenv target" in a factory-home dotenv database
    When the console is started with DAEMAR_DB set and the factory-home dotenv present
    Then the startup line's "db_path" is the environment database path
    And the startup line's "source" is "env"
    And the queue lists the Card titled "Env target" and not the Card titled "Dotenv target"

  Scenario: The console serves a dotenv-selected database
    # S3-B1/S3-B3, review finding G6: every ConfigSource variant is proved
    # through serve itself, not only through the shared resolver. Dotenv
    # only: no --db, no DAEMAR_DB.
    Given an open Card titled "Dotenv target" in a factory-home dotenv database
    When the console is started with no --db and no DAEMAR_DB and the factory-home dotenv present
    Then the startup line's "db_path" is the dotenv database path
    And the startup line's "source" is "dotenv"
    And the queue lists the Card titled "Dotenv target" and no other

  Scenario: The console serves the unconfigured default database
    # S3-B1/S3-B3, review finding G6: isolated HOME, no --db, no
    # DAEMAR_DB, no dotenv; the pre-existing migrated default at
    # ~/.daemar/daemar.db is served, not created.
    Given an open Card titled "Default target" in an isolated factory home's default database
    When the console is started in that isolated factory home with no configuration
    Then the startup line's "db_path" is that default database path
    And the startup line's "source" is "default"
    And the queue lists the Card titled "Default target" and no other

  Scenario: A missing database is a startup failure, not a creation
    # S3-B3 — serve requires an existing, migrated database; it never
    # creates one. The assertion is on the main file, not on the absence
    # of any filesystem entry.
    Given no database file exists at the scenario database path
    When the console is started with "--port 0"
    Then the console exits with error category "missing"
    And the error names the scenario database path
    And no database file exists at the scenario database path

  Scenario: An uninitialized database is a startup failure, not a migration
    # S3-B3 — an empty SQLite file (no schema) is refused; serve applies
    # no migration.
    Given an empty SQLite database file at the scenario database path
    When the console is started with "--port 0"
    Then the console exits with error category "unavailable"
    And the error names the scenario database path
    And the database file at the scenario database path carries no Card schema

  Scenario: A behind-schema database is a startup failure, not a migration
    # S3-B3 — a database with only part of the migration set applied is
    # refused; its migration state is verified, never advanced.
    Given a database at the scenario database path with only the first migration applied
    When the console is started with "--port 0"
    Then the console exits with error category "unavailable"
    And the error names the scenario database path
    And the database's applied migrations are unchanged

  Scenario: A running console leaves the main database file untouched
    # S3-B3 — content, permissions, schema: none change across a full read
    # walk. WAL/SHM coordination files beside it are permitted and are
    # not asserted on either way.
    Given an open Card with 3 appended decisions
    And the main database file's bytes, permissions, and applied migrations are recorded
    And the console is running
    When the queue page, the Card page, and every entry inspector are requested
    Then the main database file's bytes, permissions, and applied migrations are unchanged

  # ---------------------------------------------------------------- S3-B4

  Scenario: The queue lists every Card in creation order
    # S3-B4 — the order `card list` returns; each queue row is matched to
    # its Card by Card ID, never by position count alone.
    Given a Card titled "First" with task key "PER-101" and 2 appended decisions
    And a Card titled "Second" with no task key and no further entries
    And a Card titled "Third" with task key "PER-103" and 1 appended stage event
    And the console is running
    When the queue page is requested
    Then the queue lists the Cards in the order `card list` returns them
    And the queue row for the Card titled "First" shows task key "PER-101" and title "First"
    And the queue row for the Card titled "Second" shows the task key as absent and title "Second"
    And the queue row for each Card shows last activity equal to the recorded_at of that Card's highest-sequence entry
    And the queue row for the Card titled "Second" shows last activity equal to its card-created entry's recorded_at

  Scenario: No Cards renders an explicit empty queue
    # S3-B4 — explicit empty state, not a blank pane.
    Given a migrated database with no Cards
    And the console is running
    When the queue page is requested
    Then the response status is 200
    And the queue pane states that there are no Cards
    And the queue lists no Card rows

  Scenario: Last activity is computed per request
    # S3-B4 / S3-B8 — no cache: a new append moves last activity on the
    # very next request, with the console never restarted.
    Given an open Card
    And the console is running
    And the queue page was requested
    When a producer appends a decision to the Card through the CLI
    And the queue page is requested again
    Then the queue row for that Card shows last activity equal to the new entry's recorded_at

  # ---------------------------------------------------------------- S3-B5

  Scenario: Selecting a Card shows its identity and its complete stream
    # S3-B5 — one row per entry, none omitted or grouped; each row is
    # matched to the durable entry by sequence and checked field by field
    # against `card history`.
    Given a Card titled "Stream under test" with task key "PER-110" and workspace "github/daemar"
    And producer "claude" of kind "agent" appended a decision with summary "Chose Axum" and reason "Fits B1-B3"
    And producer "codex" of kind "agent" recorded a stage event with stage "review" and summary "Round one clean"
    And producer "test-operator" of kind "operator" appended a decision with summary "Accepted" and reason "Demo passed"
    And the console is running
    When the Card page is requested
    Then the response status is 200
    And the Card identity shows the Card ID, task key "PER-110", workspace "github/daemar", title "Stream under test", and the created timestamp from `card list`
    And the stream shows exactly one row per entry of `card history`, in sequence order 1 through 4
    And every stream row shows the sequence, entry type, producer identity, producer kind, and recorded_at of the entry it renders
    And the row at sequence 1 summarizes as the title "Stream under test"
    And the row at sequence 2 summarizes as "Chose Axum"
    And the row at sequence 3 summarizes as stage "review" and summary "Round one clean"
    And the row at sequence 4 summarizes as "Accepted"
    And every stream row is labeled reported
    And nothing on the page denotes verified

  Scenario: The queue is a list of links to Card pages
    # S3-B5 — selection is a link with the Card ID in the path; no form,
    # no script.
    Given a Card titled "Linked"
    And the console is running
    When the queue page is requested
    Then the queue row for the Card titled "Linked" links to "/cards/{card_id}" for that Card
    And following that link renders the Card page for that Card

  Scenario: The selected Card is marked in the queue
    # S3-B5 / S3-B4 — the queue stays on the Card page and shows which
    # Card is selected.
    Given a Card titled "Alpha"
    And a Card titled "Beta"
    And the console is running
    When the Card page for the Card titled "Beta" is requested
    Then the queue lists both Cards
    And the queue marks the Card titled "Beta" as selected and no other

  Scenario: A Card with many entries is rendered whole
    # S3-B5 boundary — no pagination, no grouping: the 50-entry
    # decision-heavy shape of the PER-83 Card renders every row.
    Given an open Card with 49 appended decisions
    And the console is running
    When the Card page is requested
    Then the stream shows exactly one row per entry of `card history`, in sequence order 1 through 50

  # ---------------------------------------------------------------- S3-B6

  Scenario: Selecting a row opens that entry's inspector
    # S3-B6 — the inspector renders the full envelope and payload of the
    # entry named by (Card ID, entry ID), compared field by field to
    # `card history`.
    Given an open Card
    And producer "claude" of kind "agent" recorded a stage event with stage "measure" and summary "Counter" and payload:
      """
      {"n": 18446744073709551617, "nested": {"list": [1, "two", null, true], "text": "plain"}}
      """
    And the console is running
    When the Card page is requested
    And the stream row at sequence 2 is followed to its inspector link
    Then the response status is 200
    And the inspector shows the entry ID, Card ID, sequence 2, schema version, entry type "stage-event", producer "claude" of kind "agent", and recorded_at of that entry
    And the inspector payload is JSON-equal to the payload `card history` returns for that entry
    And the inspector payload text contains "18446744073709551617"
    And the queue and the Card identity remain on the page
    And the stream still shows every row

  Scenario: The inspector link carries the entry in the URL
    # S3-B6 — selection lives in the URL: Card in the path, entry as a
    # query member. No client state.
    Given an open Card
    And producer "claude" of kind "agent" appended a decision with summary "Located" and reason "URL state"
    And the console is running
    When the Card page is requested
    Then the stream row at sequence 2 links to "/cards/{card_id}" for that Card with a query member naming that entry's ID

  Scenario: An entry from another Card is not shown under this Card
    # S3-B6 — lookup is by (Card ID, entry ID); an entry that exists but
    # belongs elsewhere is a 404, never an inspector under the wrong
    # Card's identity.
    Given a Card titled "Owner"
    And producer "claude" of kind "agent" appended a decision to the Card titled "Owner" with summary "Mine"
    And a Card titled "Impostor"
    And the console is running
    When the Card page for the Card titled "Impostor" is requested with the entry ID of the "Mine" decision
    Then the response status is 404
    And no inspector is shown
    And the page shows no field of the "Mine" decision
    And the queue still lists both Cards

  Scenario: A card-created entry has an inspector too
    # S3-B6 — every entry type is inspectable; the card-created payload
    # is the Card's opening facts.
    Given a Card titled "Opened" with task key "PER-120"
    And the console is running
    When the Card page is requested
    And the stream row at sequence 1 is followed to its inspector link
    Then the inspector shows the entry ID, Card ID, sequence 1, schema version, entry type "card-created", producer "test-operator" of kind "operator", and recorded_at of that entry
    And the inspector payload is JSON-equal to the payload `card history` returns for that entry

  Scenario: A decision inspector renders the durable decision, not a look-alike
    # S3-B6, review finding G4: the decision is the entry type the
    # dogfood Cards are made of; its inspector is compared field by field
    # to the keyed history entry so a summary/reason swap cannot pass.
    Given an open Card
    And producer "codex" of kind "agent" appended a decision with summary "Summary text" and reason "Reason text"
    And the console is running
    When the Card page is requested
    And the stream row at sequence 2 is followed to its inspector link
    Then the inspector shows the entry ID, Card ID, sequence 2, schema version, entry type "decision", producer "codex" of kind "agent", and recorded_at of that entry
    And the inspector payload is JSON-equal to the payload `card history` returns for that entry
    And the inspector shows "Summary text" as the summary and "Reason text" as the reason, not exchanged

  # ---------------------------------------------------------------- S3-B7

  Scenario Outline: Producer-controlled text is inert at every sink
    # S3-B7 — the marker is planted in one field at a time and traced to
    # every sink it reaches: the queue row, the Card identity, the stream
    # row, and the inspector. At each sink the marker is text only; no
    # element, attribute, script, or style is created. Each sink is
    # asserted, not the first one found.
    Given a hostile marker that would create an element, an attribute, a script, and a style if interpreted
    And a Card whose <field> is the hostile marker
    And the console is running
    When every page that renders that <field> is requested
    Then at every sink the hostile marker appears as text and nothing else
    And no page contains an element, attribute, script, or style the marker would have created

    Examples:
      | field                                    |
      | title                                    |
      | task key                                 |
      | workspace                                |
      | producer identity                        |
      | decision summary                         |
      | decision reason                          |
      | stage                                    |
      | stage-event summary                      |
      | payload string at the top level          |
      | payload string nested three levels deep  |
      | payload member name                      |

  Scenario: Hostile text inside a link target stays inert
    # S3-B7 boundary — a task key or Card content never lands in an href
    # or any attribute; links are built from Card ID and entry ID only.
    Given a hostile marker that would break out of an attribute if interpreted
    And a Card whose title is the hostile marker
    And the console is running
    When the queue page and the Card page are requested
    Then every link on those pages targets a "/cards/" path built from a Card ID and, at most, an entry ID query member
    And no attribute value on those pages contains the hostile marker

  # ---------------------------------------------------------------- S3-B8

  Scenario Outline: Unsafe methods are rejected on every route
    # S3-B8 — read-only at the HTTP boundary: every route answers unsafe
    # methods with 405 and the record is unchanged.
    Given an open Card with 1 appended decision
    And the console is running
    When "<method>" is sent to "<route>"
    Then the response status is 405
    And the Card history is byte-for-byte unchanged

    Examples:
      | method | route                            |
      | POST   | /                                |
      | PUT    | /                                |
      | DELETE | /                                |
      | PATCH  | /                                |
      | POST   | /cards/{card_id}                 |
      | PUT    | /cards/{card_id}                 |
      | DELETE | /cards/{card_id}                 |
      | PATCH  | /cards/{card_id}                 |
      | POST   | /cards/{card_id}?entry={entry_id} |
      | POST   | /static/console.css              |

  Scenario: A concurrent CLI append is visible on reload exactly once
    # S3-B8 — derived-state-free: no cache, no fold, no background task.
    # The new entry appears once, at its sequence, with its own recorded_at.
    Given an open Card with 2 appended decisions
    And the console is running
    And the Card page was requested and showed rows at sequences 1 through 3
    When producer "codex" of kind "agent" records a stage event through the CLI with stage "late" and summary "Appended while serving"
    And the Card page is requested again
    Then the stream shows exactly one row per entry of `card history`, in sequence order 1 through 4
    And exactly one stream row carries the new entry's entry ID
    And the row at sequence 4 summarizes as stage "late" and summary "Appended while serving"
    And the queue row for that Card shows last activity equal to the new entry's recorded_at

  Scenario: The console is script-free and self-contained
    # S3-B8 — every page scanned: no script element, no inline event
    # handler attribute, no embedded document or inline CSS (so the markup
    # alone is the whole page), no reference to any origin but the
    # server's; one local stylesheet is the only asset and it is served.
    # What a browser actually fetches and shows is proved in a browser:
    # the Playwright layer asserts the load-time fetch set is exactly the
    # page and the stylesheet. This layer does not model a browser.
    Given an open Card with 1 appended decision
    And the console is running
    When the queue page, the Card page, the entry inspector, a 404 page, and a 421 response are requested
    Then no response body contains a script element
    And no response body contains an inline event handler attribute
    And no response body contains an embedded document, a base element, or inline CSS
    And every URL referenced from any response body is same-origin
    And the only referenced asset is one stylesheet at a local path
    And requesting that stylesheet returns 200 with a CSS content type

  Scenario: The console never writes a Card record
    # S3-B8 — a full read walk plus every rejected unsafe method leaves
    # `card history` and `card list` byte-for-byte as they were.
    Given a Card titled "Alpha" with 2 appended decisions
    And a Card titled "Beta" with 1 appended stage event
    And the histories and list are recorded
    And the console is running
    When the queue page, every Card page, and every entry inspector are requested
    And every unsafe method is sent to every route, including the stylesheet and the entry-query form
    Then every one of those responses has status 405
    And the histories and list are byte-for-byte unchanged

  # ---------------------------------------------------------------- S3-B9

  Scenario: An unknown Card is a 404 page with the queue
    # S3-B9
    Given a Card titled "Known"
    And the console is running
    When the Card page for an unknown Card ID is requested
    Then the response status is 404
    And the page says the Card was not found
    And the queue still lists the Card titled "Known"
    And no Card identity or stream is shown

  Scenario: An unknown entry is a 404 page with the queue and no partial Card
    # S3-B9 — no partial Card presented as complete: the Card page is not
    # rendered around an inspector that could not be opened.
    Given an open Card with 1 appended decision
    And the console is running
    When the Card page is requested with an unknown entry ID
    Then the response status is 404
    And the page says the entry was not found
    And the queue still lists that Card
    And no inspector is shown

  Scenario: A malformed Card ID in the path is a 404, not a 500
    # S3-B9 — a path segment that is not a Card ID is "not found", not a
    # storage failure and not a fabricated Card.
    Given an open Card
    And the console is running
    When the Card page for the path segment "not-a-card-id" is requested
    Then the response status is 404
    And the page says the Card was not found
    And the queue still lists that Card
    And no Card identity or stream is shown

  Scenario: A corrupt row is a 500 that fabricates nothing
    # S3-B9 — a storage failure is reported as such and no default stands
    # in for the corrupt value. Review finding G5: the failure is local to
    # the Card stream, the queue is still readable, so B4 keeps it on the
    # page; only a queue the server could not read is withheld.
    Given an open Card with 2 appended decisions
    And the console is running
    And the payload of the entry at sequence 2 is corrupted in storage to invalid JSON
    When the Card page is requested
    Then the response status is 500
    And the page says storage failed
    And the page shows no stream rows and no inspector
    And the page shows no fabricated default in place of the corrupt payload
    And the queue still lists that Card

  Scenario: A queue the console cannot read is not shown
    # S3-B9, review finding G1: swapping the file under an open connection
    # is OS/VFS dependent. Instead a queue-required value is invalidated
    # in the same database through a separate writable connection, so the
    # queue query itself fails to decode and the 500 page shows no queue
    # rather than a queue it could not read.
    Given a Card titled "Alpha"
    And a Card titled "Beta"
    And the console is running
    And the recorded_at of the card-created entry of the Card titled "Beta" is corrupted in storage to a non-timestamp
    When the queue page is requested
    Then the response status is 500
    And the page says storage failed
    And the page renders no queue and no Card content
    And the page shows no row for the Card titled "Alpha"
