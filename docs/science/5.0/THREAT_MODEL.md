# Lumen Science 5.0 Threat Model

**Status**: DRAFT v1
**Date**: 2026-07-25
**Milestone**: LS5-3

## Scope

All Lumen Science components from V1 through V5: connectors, notebooks,
reviewers, renderers, evidence graphs, workflows, remote compute, digital
twins, and device control. Motif renderer is in scope as an embedded
capability. Upstream open-science skills are in scope as AI instruction
templates.

Out of scope: physical security of hardware, OS-level kernel exploits,
side-channel attacks on CPU/RAM, social engineering of operators.

---

## Attack Surface

### A1: Malicious Scientific Documents

**Threat**: Crafted FASTA/GenBank/CSV/PDF files with embedded exploits
targeting parsers, renderers, or notebook kernels.

**Impact**: Code execution in SessionActor process, artifact corruption,
evidence forgery.

**Mitigation**:
- All parsers fail closed on malformed input
- Renderer sandbox (iframe with strict CSP for Motif)
- Max file size enforcement before parsing
- Content-type validation against magic bytes
- No native code execution from parsed documents

### A2: Prompt Injection via Datasets

**Threat**: Scientific datasets containing injected prompts designed to
manipulate AI model behavior during analysis.

**Impact**: Bypassed permission checks, fabricated evidence, unauthorized
device commands.

**Mitigation**:
- Data and instruction channels are separated
- AI model output is never directly trusted as evidence
- All model-generated claims require artifact citation
- Reviewer independence catches injected narratives

### A3: Notebook Package Poisoning

**Threat**: Python/R/Julia packages with backdoors installed in science
environments, exfiltrating data or injecting results.

**Impact**: Credential theft, data exfiltration, poisoned research outputs.

**Mitigation**:
- Default no-network for notebook kernels
- Package lock hashes verified before kernel start
- Environment manifest recorded for reproducibility
- Stdout/stderr size caps prevent data exfiltration channels
- Kernel process isolation (separate process, no shared memory)

### A4: Workflow Recursion / Resource Exhaustion

**Threat**: Malicious or buggy workflow definitions that create infinite
loops, fork bombs, or memory exhaustion.

**Impact**: Denial of service, system crash, lost work.

**Mitigation**:
- DAG validation before execution (must be acyclic)
- Per-step timeout enforcement
- Total workflow resource budget
- Concurrent step limits
- Recursive workflow detection

### A5: Evidence Graph Deception

**Threat**: Crafted evidence edges that create fake derivation chains,
self-referencing claims, or cross-project data leaks.

**Impact**: Fraudulent research conclusions, data confidentiality breach.

**Mitigation**:
- All edges must reference registered artifact SHA-256
- Cycle detection on graph write
- Cross-project edge prohibition
- Superseded artifacts cannot be silently promoted
- Evidence graph integrity verified on every read

### A6: Collaborator Privilege Escalation

**Threat**: Invited collaborator escalates to owner, modifies artifacts,
approves their own claims.

**Impact**: Research integrity compromise, data theft.

**Mitigation**:
- Capability-based permission model (not role-based)
- Owner transfer requires explicit chain of custody
- All mutations logged with actor identity
- Approval cannot be self-referential
- Removed collaborators lose all access immediately

### A7: Scheduler/SSH Credential Leak

**Threat**: Remote compute credentials appear in logs, artifacts, evidence,
or error messages.

**Impact**: Credential compromise, unauthorized HPC access.

**Mitigation**:
- Credentials are process-local only
- Audit records contain operation digests, never raw credentials
- Redacted logs before persistence
- Host key fingerprint verification before connection
- No credential material in URL construction

### A8: Device Command Replay

**Threat**: Attacker captures and replays valid device commands to cause
physical damage or data corruption.

**Impact**: Equipment damage, safety incident, fraudulent experimental results.

**Mitigation**:
- Every command plan has unique sha256
- Command sequence numbers prevent replay
- Preflight checks verify session state before execution
- Emergency stop is always available
- Device sessions have bounded duration

### A9: Sensor Forgery

**Threat**: Malicious adapter or MITM injects fake sensor readings to
manipulate experimental conclusions.

**Impact**: Invalid scientific conclusions, safety bypass.

**Mitigation**:
- Sensor identity bound to calibration record
- Raw payload hash recorded before parsing
- Timestamp source verified
- Quality flags on every observation
- Sensor trust chain verifiable from device to evidence

### A10: Digital Twin / Real Target Confusion

**Threat**: Digital twin outputs are presented or interpreted as real
experimental results.

**Impact**: Invalid research conclusions, regulatory violations.

**Mitigation**:
- target_mode field mandatory on all experiments
- UI must display target_mode prominently
- Digital twin evidence labeled as "simulated"
- Real device actions require explicit operator confirmation
- target_mode cannot change mid-experiment

### A11: Reviewer Collusion

**Threat**: Multiple LLM-based reviewers collude to approve fraudulent
results, or human reviewers are compromised.

**Impact**: Invalid acceptance of fabricated research.

**Mitigation**:
- Minimum reviewer count per verdict
- Automated review must be labeled as such
- Review independence verified by non-overlapping artifact access
- Reviewer identity tracked per verdict
- Anomaly detection on review patterns

### A12: Malicious Model Output

**Threat**: AI model generates scientifically plausible but incorrect
conclusions, or deliberately deceptive output.

**Impact**: Propagation of false scientific claims.

**Mitigation**:
- Model output is never direct evidence — must cite artifacts
- All claims require supporting evidence edges
- Reviewer independence catches unsupported claims
- Model version and prompt template recorded for audit
- Provider cache truth verified

### A13: Provenance Truncation or Replacement

**Threat**: Attacker deletes or replaces provenance records to hide
misconduct or fabricate evidence chains.

**Impact**: Undetectable research fraud.

**Mitigation**:
- Provenance is append-only in event log
- All provenance entries include input/output SHA-256
- Replay must reproduce provenance exactly
- Tamper-evident audit trail
- Migration preserves all provenance records

### A14: Migration Attack

**Threat**: Attacker exploits migration process to inject data, bypass
validation, or corrupt schemas.

**Impact**: Data corruption, privilege escalation during upgrade.

**Mitigation**:
- Pre-migration backup mandatory
- Migration journal records every transformation
- Schema version verification before and after
- Corrupt source store fails closed
- Rollback preserves original data

### A15: Supply Chain Compromise

**Threat**: Compromised npm package (Motif), Cargo dependency, Python
package, or device driver introduces backdoor.

**Impact**: Full system compromise.

**Mitigation**:
- SBOM with exact versions and hashes for all dependencies
- Reproducible build verification
- Third-party provenance documentation
- License audit for all included assets
- Signature verification on all release assets
