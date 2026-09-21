# Security and ethics boundary

JOCKY is restricted to authorised computer and network forensic investigation
inside explicitly designated isolated Linux and Windows virtual-machine labs.
Linux development comes first; Windows operational support is deferred to Version 16.
It is not an endpoint
evasion, intrusion, persistence, exploitation, or destructive-action toolkit.

These rules implement Principles I and VI of the persistent
[JOCKY constitution](../.specify/memory/constitution.md). A new product version
does not reset them.

## Prohibited implementation

The repository must not implement or bundle:

- EDR disabling or security-control bypasses;
- kernel tampering or kernel-structure manipulation;
- process hollowing or reflective injection into other processes;
- API unhooking, direct-syscall evasion, or thread execution hijacking;
- privilege-escalation exploits or automatic elevation;
- covert persistence or persistence modification;
- domain-fronting bypasses;
- weaponised BYOVD, vulnerable drivers, driver loading, or driver exploitation;
- malware, shellcode, credential theft, destructive actions, or anti-analysis
  logic.

The official requirement names remain in `docs/traceability.md` so scope and
safety substitutions are reviewable. Documentation is not authorization to
implement them.

## Safe treatment of sensitive categories

Any later interface for a sensitive category must be disabled by default and
return a typed `Unsupported`, `NotImplemented`, or `LabNotConfigured` result
until its documented safe implementation exists. Permitted research is limited
to harmless same-process simulations, loopback-only networking tests,
detection-only driver inventory from synthetic or approved catalogues, and
externally produced lab evidence under an explicit gate.

An auditable lab gate must identify the authorising operator, case, endpoint,
approved test profile, and bounded validity period. It must never turn a
prohibited implementation into an allowed one.

## Operational safeguards

- Only endpoints explicitly designated for the current authorised lab case may
  be addressed.
- A Linux development workstation is not automatically an authorised endpoint.
  Planned collector tests require named lab VMs and explicit bounded policies.
- Distribution support is evidence-based. Missing facilities or permissions
  must yield typed failures, not trigger elevation, security-control changes
  or fallback to unapproved evidence sources.
- The Version 3 reference interpreter stays fixture-only; a future live
  collector uses a separate explicit entry point, never a run-fixture fallback.
- The default developer configuration enables no lab-only adapter.
- APIs validate input and return typed errors.
- Logs for task and endpoint operations must contain the associated task IDs
  and endpoint IDs, but not forensic payloads, evidence contents, credentials,
  encryption keys, or secrets. Health/version operations without task or
  endpoint context must not invent those identifiers.
- Evidence is integrity-addressed with standard SHA-256 metadata; no novel
  cipher is introduced in Version 0.
- Offline tests use synthetic fixtures. Integration and lab tests must be
  separately marked and must name their controlled environment.
- Secrets and production credentials are never committed. Versioned example
  files contain local placeholders only.

Versions 0 and 1 contain no endpoint networking, forensic collection, task
dispatch, native execution, or sensitive adapter code. The Version 1 parser
consumes caller-supplied source without host I/O; its CLI adapter reads only
the explicitly supplied file and writes command output.
