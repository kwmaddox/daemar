# PROTOTYPE (PER-19) — run-inspection CLI surface

Throwaway artifacts answering: *what CLI surface makes Workflow Runs
inspectable by humans — listing by recency, filtering by Change Request, and
reading a run's Run Record-derived state?*

## Run it

```console
$ python3 daemar.py runs                        # all runs, newest first
$ python3 daemar.py runs add-run-inspection     # filter by Change Request slug
$ python3 daemar.py show 0197e1a0               # one run, by id prefix
$ python3 daemar.py show 0197e1                 # ambiguous prefix -> error
```

`fixture-runs/` fabricates four Run Records: `published`,
`validation_rejected`, `failed` (typed cause), and `interrupted` (a record
with no `run_terminated` — detected, never written). Two share the slug
`add-run-inspection` to exercise the filter.

## Questions to react to

1. Command shape: flat `daemar runs [slug]` + `daemar show <id>` — or grouped
   `daemar runs list` / `daemar runs show <id>`?
2. Short IDs: list prints the first 8 chars; `show` accepts any unambiguous
   prefix. Right tradeoff for an ID that is only ever copied?
3. The list table and show layout — anything missing that you reach for when
   a run goes wrong, or anything you'd cut?
