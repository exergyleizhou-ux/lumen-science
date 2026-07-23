# Lumen Science wet-lab safety boundary

Seam contract: **S5**. Status: design and safety boundary only. No instrument
connector, protocol executor, device credential, or physical-control path is
implemented or authorized by this document.

## Non-negotiable authority model

Lumen may prepare an experiment proposal, collect evidence, and create a
human-reviewable run record. It must not autonomously actuate a wet-lab device.
Every future physical action requires all of the following independently
recorded gates:

1. a named human operator with the applicable laboratory authorization;
2. a second, distinct reviewer approving the exact immutable protocol revision;
3. verified device identity, maintenance/calibration state, and a live emergency
   stop capability owned by the laboratory, not by the model;
4. a bounded device command plan with safe ranges, expected sample identity,
   waste/disposal handling, and an explicit abort condition;
5. a fresh confirmation immediately before action. A model, consultant, Goal,
   Expert, or stale background callback can never satisfy this gate.

Any missing, expired, conflicting, or unreadable gate is a fail-closed denial.
The system must not queue, retry, resume, or simulate a physical action after
a restart.

## Required future data contract

A real wet-lab integration must introduce a separate project-owned record; it
must not overload the SSH/SCP connector or a generic tool call. At minimum it
needs immutable IDs for the protocol revision, sample/batch, device, operator,
reviewer, approval timestamps, calibration evidence, command-plan hash, and
emergency-stop verification. Durable events must record the gate decisions and
redacted device identity, never credentials or raw patient/subject data.

The only terminal states are `denied`, `cancelled`, `interrupted`, `failed`, and
`completed_with_operator_attestation`. A transport acknowledgement alone is not
completion: HostVerification requires the operator's post-run attestation and
the device-generated audit artifact to match the approved command-plan hash.

## Explicit exclusions

- No direct control of liquid handlers, incubators, centrifuges, thermal
  cyclers, microscopes, sequencers, robots, pumps, or hazardous-material
  equipment.
- No automatic selection or execution of laboratory protocols.
- No bypass of local SOPs, biosafety level requirements, institution policy,
  or device-vendor safeguards.
- No live tests using a device, sample, reagent, or patient/subject data without
  a separately approved study and authorized humans.

## Entry criteria for a future P7 implementation

Before code is written, the user/laboratory must explicitly supply the device
class, operating environment, risk classification, applicable SOP, named
operator/reviewer roles, emergency-stop ownership, test fixture/simulator, and
authorization for a narrowly scoped integration. The first implementation must
be a disconnected simulator with deny/cancel/restart tests; a live device is a
separate L5 authorization and is never an implied follow-up.
