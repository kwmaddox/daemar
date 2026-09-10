# Copilot PR37 dispositions

Dedicated read-only planner /root/copilot_triage; baseline369974d. Root records, does not semantically review.

CP-01 SURVIVES, thread PRRT_kwDOTZwv0c6hPwUg (comment3983495675): storage.rs:142 inner join drops Cards with no entries; migration permits state, contract.md:193-201 promises every Card and :417-422 complete-read errors. Fix left join and explicit missing timestamp -> existing Error::Corrupt { context: Queue }, preserving other decode errors/order/highest sequence/payload independence.

CP-02 SURVIVES, thread PRRT_kwDOTZwv0c6hPwVI (comment3983495731): error.rs:9,150 says four while enum152-162 has five; main.rs:7 omits unavailable. Update these docs only, keep API/mappings.

CP-03 SURVIVES (suppressed, no thread): console.rs web::tests :657,695,719,731,739,747 etc repeat chosen7331; C14 repetition applies to fixtures. Private named fixture port with rationale, derive all valid/malformed/hostile authorities without weakening assertions. No other fixture cleanup.

CP-04 REJECT as current required defect (suppressed, no thread): console.rs:489 .ok() exists, but entry_view's sole fallible operation (:304-305) is history_fields; domain.rs:362-389 serializes current string/optional-string structs or existing JSON objects with derived non-custom serializers (:633-667). No current error-producing serializer. Invalid stored payloads fail entry_from_row storage.rs:742-747 before handler and already map500 at console.rs:493-495. Hypothetical new serializer is not required present fix; no artificial seam/reproducer. Reconsider if payload serializer gains real failure mode.

CP-05 summary-only pinned Playwright mention: not promoted, no concrete location/failure/reproduction supplied. browser/package.json:9 pins1.62.1; lock pins corresponding Playwright; justfile:34 npm ci and :30 runs in browser. Planner verified local CLI and installed package version1.62.1. This confirms installed invocation, NOT all missing-install environments. Record uncertainty, no invented change.

Planner returned minimal one-Luna packet; no plan review. Final replies cite actual pushed commit/checks. Both threads resolve only after verified fixes; suppressed/summary items receive review-level response with this evidence. All five remain preserved on Card.

