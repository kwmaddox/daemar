#!/usr/bin/env python3
"""PROTOTYPE (PER-9) — throwaway CLI stub. Do not ship.

Simulates Daemar's Change Request input surface so the CLI contract can be
evaluated by feel. Validation behavior implements the PER-11 decision:
four strictly parsed fields, bounds, unknown/duplicate keys rejected.

Run:
  python3 daemar.py run examples/add-run-inspection.json
  python3 daemar.py run examples/invalid-unknown-key.json
  python3 daemar.py run examples/invalid-bounds.json
"""
import json
import re
import sys

MAX_DOC_BYTES = 16 * 1024
SCHEMA_ID = "change_request.v1"
ID_PATTERN = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
ID_MAX = 64
OBJECTIVE_MAX = 4096
CRITERIA_MAX_ITEMS = 20
CRITERION_MAX = 1024
KNOWN_FIELDS = ("schema", "id", "objective", "acceptance_criteria")
IGNORED_FIELDS = ("$schema",)  # editor hint; allowed, value not interpreted


class Problem:
    """One Preflight failure. `code` previews a future PreflightError enum variant."""

    def __init__(self, code, message, pointer):
        self.code = code
        self.message = message
        self.pointer = pointer

    def render(self):
        return f"  [{self.code}] {self.message} (at {self.pointer})"


def load_document(path):
    """Return (problems, document). document is None when reading/parsing fails fatally."""
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError as e:
        return [Problem("io_error", f"cannot read file: {e.strerror}", "/")], None
    if len(raw) > MAX_DOC_BYTES:
        return [Problem("document_too_large",
                        f"document is {len(raw):,} bytes, maximum is {MAX_DOC_BYTES:,}", "/")], None
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        return [Problem("invalid_encoding", "document is not valid UTF-8", "/")], None

    duplicates = []

    def track_pairs(pairs):
        seen = {}
        for key, value in pairs:
            if key in seen:
                duplicates.append(key)
            seen[key] = value
        return seen

    try:
        document = json.loads(text, object_pairs_hook=track_pairs)
    except json.JSONDecodeError as e:
        return [Problem("invalid_json",
                        f"not valid JSON: {e.msg} (line {e.lineno}, column {e.colno})", "/")], None
    if not isinstance(document, dict):
        return [Problem("not_an_object", "top level must be a JSON object", "/")], None

    problems = [Problem("duplicate_field", f"duplicate field `{key}`", f"/{key}") for key in duplicates]
    return problems, document


def check_fields(document):
    problems = []
    for key in document:
        if key not in KNOWN_FIELDS and key not in IGNORED_FIELDS:
            problems.append(Problem(
                "unknown_field",
                f"unknown field `{key}`; {SCHEMA_ID} accepts exactly: {', '.join(KNOWN_FIELDS)}",
                f"/{key}"))
    for key in KNOWN_FIELDS:
        if key not in document:
            problems.append(Problem("missing_field", f"missing required field `{key}`", f"/{key}"))

    if "schema" in document:
        value = document["schema"]
        if not isinstance(value, str):
            problems.append(Problem("wrong_type", "`schema` must be a string", "/schema"))
        elif value != SCHEMA_ID:
            problems.append(Problem("unsupported_version",
                                    f'`schema` is "{value}"; this Daemar accepts exactly "{SCHEMA_ID}"',
                                    "/schema"))

    if "id" in document:
        value = document["id"]
        if not isinstance(value, str):
            problems.append(Problem("wrong_type", "`id` must be a string", "/id"))
        elif not 1 <= len(value) <= ID_MAX:
            problems.append(Problem("field_too_long",
                                    f"`id` is {len(value)} characters, allowed 1-{ID_MAX}", "/id"))
        elif not ID_PATTERN.match(value):
            problems.append(Problem("bad_slug",
                                    "`id` must be lowercase kebab-case (a-z, 0-9, single dashes)", "/id"))

    if "objective" in document:
        value = document["objective"]
        if not isinstance(value, str):
            problems.append(Problem("wrong_type", "`objective` must be a string", "/objective"))
        elif not value.strip():
            problems.append(Problem("blank_field", "`objective` must not be blank", "/objective"))
        elif len(value) > OBJECTIVE_MAX:
            problems.append(Problem("field_too_long",
                                    f"`objective` is {len(value):,} characters, maximum is {OBJECTIVE_MAX:,}",
                                    "/objective"))

    if "acceptance_criteria" in document:
        value = document["acceptance_criteria"]
        if not isinstance(value, list):
            problems.append(Problem("wrong_type",
                                    "`acceptance_criteria` must be an array of strings",
                                    "/acceptance_criteria"))
        else:
            if not 1 <= len(value) <= CRITERIA_MAX_ITEMS:
                problems.append(Problem("bad_item_count",
                                        f"`acceptance_criteria` has {len(value)} items, allowed 1-{CRITERIA_MAX_ITEMS}",
                                        "/acceptance_criteria"))
            for i, item in enumerate(value):
                pointer = f"/acceptance_criteria/{i}"
                if not isinstance(item, str):
                    problems.append(Problem("wrong_type", f"criterion #{i + 1} must be a string", pointer))
                elif not item.strip():
                    problems.append(Problem("blank_field", f"criterion #{i + 1} must not be blank", pointer))
                elif len(item) > CRITERION_MAX:
                    problems.append(Problem("field_too_long",
                                            f"criterion #{i + 1} is {len(item):,} characters, maximum is {CRITERION_MAX:,}",
                                            pointer))
    return problems


def validate(path):
    problems, document = load_document(path)
    if document is not None:
        problems.extend(check_fields(document))
    return problems, document


def main(argv):
    if len(argv) != 3 or argv[1] != "run":
        print("usage: daemar.py run <change-request.json>", file=sys.stderr)
        return 2
    path = argv[2]
    problems, document = validate(path)
    if problems:
        print(f"error: invalid Change Request - {len(problems)} problem(s) in {path}\n", file=sys.stderr)
        for problem in problems:
            print(problem.render(), file=sys.stderr)
        print("\nno Workflow Run created", file=sys.stderr)
        return 1
    print(f"valid Change Request: {document['id']}")
    print(f"  objective: {len(document['objective'])} chars - "
          f"acceptance_criteria: {len(document['acceptance_criteria'])} items")
    print()
    print("prototype: would now allocate a Workflow Run ID, initialize")
    print(".daemar/runs/<run_id>/, preserve a verbatim copy of the request,")
    print("and begin trusted setup. Execution is not implemented.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
