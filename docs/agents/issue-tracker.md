# Issue Tracker: Linear

Daemar uses Linear as its issue tracker. Agents access Linear through the
official Linear MCP server.

## Scope

- Team: `Personal` (`PER`)
- Team ID: `6684d7ba-85e9-4df3-b392-8bdadd1fd9da`
- Project: `Daemar`
- Project ID: `c6d980d9-3337-4f1b-94bb-90148027d98a`
- Project URL: <https://linear.app/kwamddox-sedai/project/daemar-734419689f1c>
- Assignee used for claims: `me` (Kendall Maddox)

Create and query Daemar issues within this team and project. Do not use another
team or project unless the user explicitly changes this contract.

## Wayfinding operations

### Map

The map is a Linear issue in the `Personal` team and `Daemar` project with:

- label `wayfinder:map`
- state `In Progress` while the effort is active
- no parent issue

Use the standard Wayfinder map body:

```markdown
## Destination

<What reaching the end of this map means.>

## Notes

<Standing instructions, skills to consult, and known concerns.>

## Decisions so far

## Not yet specified

<In-scope fog that is not yet precise enough to become a ticket.>

## Out of scope

<Work explicitly beyond this destination.>
```

The map is an index. Keep each decision's detailed answer on its ticket and add
only a named link with a one-line gist under `## Decisions so far`.

### Child ticket

Create every Wayfinder ticket as a child of its map using the map's Linear issue
identifier as `parentId`. Put it in the `Personal` team and `Daemar` project,
leave it unassigned, and set its initial state to `Todo`.

Use exactly one ticket-type label:

- `wayfinder:research`
- `wayfinder:prototype`
- `wayfinder:grilling`
- `wayfinder:task`

Use this body:

```markdown
## Question

<The decision or investigation this ticket resolves.>
```

Create tickets first, then add native Linear `blockedBy` relations in a second
pass. Refer to issues by linked title in human-facing text, not by a bare issue
identifier.

### Load

Load the map as the low-resolution view. List its children with the map issue as
`parentId`; do not load every child body up front. Fetch a child's full body and
relations only when evaluating or working that ticket.

### Frontier

The frontier is the map's child issues that are:

- in `Todo`
- unassigned
- not blocked by any issue that remains non-terminal

Inspect native Linear relations when determining blockers. Sort eligible issues
by the numeric portion of their Linear identifier and select the first unless
the user names a specific frontier ticket.

Treat a missing blocker, dependency cycle, or relationship that cannot be read
as a blocking tracker error. Report it instead of guessing.

### Claim

Immediately before claiming, re-fetch the ticket and its relations. Confirm it
is still in `Todo`, unassigned, and unblocked. Then set:

- assignee: `me`
- state: `In Progress`

Assignment is the claim. Re-fetch after the mutation and stop if the expected
claim is not visible. Linear assignment through this adapter is not an atomic
compare-and-set operation, so concurrent sessions must re-check before writing.

### Resolve

Resolve exactly one ticket per session, except for Wayfinder's parallel research
flow:

1. Post the answer as a resolution comment on the ticket. Link any repository
   artifact rather than duplicating it in the map.
2. Re-read the map before changing it. Append a named ticket link and one-line
   gist under `## Decisions so far`.
3. Create newly visible tickets first and add blocker relations in a second
   pass. Move newly precise work out of `## Not yet specified`; add newly visible
   fog that is still too coarse to ticket.
4. Set the resolved ticket to `Done`.

If a ticket is superseded or falls outside the map's destination, post a comment
explaining why, set it to `Canceled`, and link it under `## Out of scope` when
appropriate. Do not add it to `## Decisions so far`.

Map-description updates are shared writes. Re-fetch the current description
immediately before each update and serialize concurrent map updates.

### Completion

Set the map to `Done` when:

- no non-terminal child tickets remain,
- `## Not yet specified` contains no unresolved fog toward the destination, and
- reaching the destination requires no further decisions.

## Labels

The `Personal` team has these Wayfinder labels:

- `wayfinder:map`
- `wayfinder:research`
- `wayfinder:prototype`
- `wayfinder:grilling`
- `wayfinder:task`

Reuse them. Do not create per-map copies.
