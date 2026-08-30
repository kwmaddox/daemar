# Daemar Factory

Daemar coordinates agentic software development around durable task records
that make work inspectable and reusable across factory stages.

## Language

**Task**:
A defined piece of software work accepted by the factory.

**Card**:
The durable, append-only record that accompanies exactly one Task and contains
the structured workflow history relevant to moving that Task through the
factory.
_Avoid_: Trace, transcript, run log

**Card entry**:
An immutable, structured workflow fact, decision, or Stage output appended to a
Card by a Stage or the factory control plane.
_Avoid_: Mutable status row

**Reported result**:
A structured claim from a Stage producer about work performed, evidence seen,
or an outcome reached. Its provenance is authoritative; its claim is not
independently verified by the factory.
_Avoid_: Verified result, trusted evidence

**Verified evidence**:
A workflow-relevant observation independently established by the factory or a
deterministic verifier rather than asserted by a Stage producer.
_Avoid_: Agent report, self-attestation

**Attempt**:
One bounded effort to advance a Card through one or more Stages. A Card can
retain multiple Attempts without replacing their history.
_Avoid_: Card, Task

**Stage execution**:
One performance of a Stage within an Attempt, including its lifecycle and
structured conclusion.
_Avoid_: Stage, Attempt, agent session

**Execution Trace**:
The high-detail diagnostic record of how a particular Stage execution unfolded,
including model interaction, tool activity, output, timing, and retries. It is
not workflow input or workflow truth.
_Avoid_: Card, task history

**Trace event**:
An immutable observation appended to an Execution Trace.
_Avoid_: Card entry

**Frontmatter**:
The narrow set of key Card information supplied to every Stage.
_Avoid_: Full context, task dump

**Stage**:
A named portion of factory work that receives Card information under a declared
contract and appends structured results to the Card.
_Avoid_: Agent, prompt

**Stage contract**:
The declaration of Card information a Stage requires to perform its work.
_Avoid_: Prompt

**Card query**:
An explicit request by a Stage for Card information beyond its Frontmatter and
Stage contract.
_Avoid_: Automatic full-history injection
