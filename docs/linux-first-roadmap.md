# Linux-first roadmap

Approved on **2026-09-21** under constitution **1.1.0** and
[ADR-0013](decisions.md#adr-0013-linux-first-delivery-and-deferred-windows-completion).
This is a delivery-policy change, not a new implemented product version.

## What this means

You do not need to work on a Windows computer to progress through the Linux
milestones. Versions 0–3 remain complete: the language, checker, verified IR and
fixture interpreter work. Real collection is still unimplemented. The next
milestone builds real, bounded, read-only Linux collection; Windows comes last.

The existing Windows and Ubuntu compiler/fixture tests stay as compatibility
checks. They do not imply live Windows collection, and this amendment does not
require manual Windows testing. Historical plans and release reports keep their
original dates and evidence; their old next-version references are superseded here.

## Delivery order

| Stage          | Scope                                                                                    | Demonstrable outcome                                             |
| -------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| Versions 0–3   | Completed foundations/compiler/fixture execution                                         | Existing synthetic triage demonstration                          |
| Version 4      | Deferred, retained as requirements for Version 16                                        | No Windows implementation required now                           |
| Version 5      | Portable Linux collectors and shared collector/policy foundations                        | Real JSON evidence from an explicitly authorised Linux VM        |
| Version 6      | Linux-first native function packages                                                     | A checked JOCKY program builds into a working Linux artifact     |
| Versions 7–10  | Linux agents, controller, dashboard and scheduling                                       | Dispatch a task to Linux VMs and view real results               |
| Versions 11–14 | Protected results, safe compiler research, trusted transport and harmless lab interfaces | Tested Linux integration; existing safety prohibitions unchanged |
| Version 15     | Linux integration, evaluation and demonstration                                          | Repeatable source-to-two-Linux-VMs-to-dashboard presentation     |
| Version 16     | Deferred Windows collectors plus packages, agents and integration                        | Verified mixed Windows/Linux workflow                            |

Each version still requires a separate explicit request and its own acceptance
evidence. Do not start Windows automatically after a Linux milestone.

## First useful result: Version 5 checkpoint

A standalone collector demonstration, separate from `jockyc run-fixture`, will:

1. Require explicit lab authorisation, allowed collection categories and limits.
2. Read system details, process inventory and current connections from that VM.
3. Inspect and hash only explicitly approved sample files.
4. Produce a schema-valid JSON report with honest permission and partial failures.
5. Compare a benign test process, loopback connection and known file hash with
   independently known values.

This gives a useful demonstration before code generation, endpoint enrollment
or the dashboard task interface exists. It does **not** execute a JOCKY source
program against a real endpoint yet; that integration belongs to later milestones.
Creating the benign objects is an explicit lab-test harness action, not a collector
side effect. Existing parse/check/build-ir/verify-ir/run-fixture modes stay non-collecting.

No actual collection command or Linux provider exists today. Version 5 must
document the new command, policy format and output contract when implemented.
If no authorised VM is available, continue offline implementation and report live
acceptance as unverified. Never substitute the development workstation implicitly.

## Linux support policy

Plan for **x86_64/glibc** VM configurations first. Pin releases/image identities
before testing; a family name alone is not a support guarantee.

| Family         | Planned initial configurations                                           | Current live-collector evidence |
| -------------- | ------------------------------------------------------------------------ | ------------------------------- |
| Debian-based   | Debian and Ubuntu, pinned releases                                       | None; future Version 5          |
| Red Hat family | Fedora and one named RHEL-compatible distribution, initially Rocky Linux | None; future Version 5          |
| Arch-based     | Arch Linux, recorded image date and package snapshot                     | None; future Version 5          |

The future support matrix must record each collector as tested, partial,
unsupported or unverified, with kernel, architecture, facility, permissions and
evidence. Passing Fedora tests does not certify RHEL or every derivative. A
container userspace test is not native VM service/kernel/permission verification.

Common collection must not depend on a package manager or systemd. Service and
log backends are capability-specific; non-systemd/missing backends return typed
`Unsupported`, distinct from permission denial or a successful empty result.
Other architectures, musl-based systems and untested derivatives remain outside
the initial guarantee. Optional unsupported backends are permitted only where
documented and tested; they cannot replace required working core collectors.

The early checkpoint can demonstrate one approved VM. Full Version 5 completion
requires the pinned matrix and its documented per-facility acceptance evidence;
missing runs must not be described as passing.

## Compatibility migration required in Version 5

Today the language and schemas recognise `windows` and `ubuntu`, not `linux`.
This governance change intentionally changes **no source, schema or fixture**.

Version 5 must inventory and version the generic Linux addition across grammar,
AST/report contracts, semantic rules, registry, shared types, IR/fixtures and
affected endpoint/build schemas. Keep old schema definitions, source spans,
diagnostics, serialized artifacts and golden tests valid. Do not relabel archived
Ubuntu evidence as generic Linux or globally replace enum strings.

`target ubuntu` keeps its Ubuntu-specific scope. A generic `target linux` must
not accept an Ubuntu-only function solely because Ubuntu uses Linux. Record OS
family separately from distribution/version, architecture and collector facilities;
static portability and actual runtime availability are different checks. Planned
registry declarations never grant collection permission or prove implementation.

The same migration must explicitly preserve Windows-only restrictions and
define mixed Windows/Linux source rules without implementing Windows collectors.
The pure fixture interpreter must never gain a live provider or host fallback.

## Deferred Windows obligations

Version 16 owns all of the following, not just a renamed collector milestone:

- Original Version 4 read-only collectors, policies, partial failures and ground truth.
- Version 6 Windows executable packaging, target handling and native artifact CI.
- Version 7 Windows agent lifecycle, supervision, limits and cancellation.
- Versions 8–10 actual availability, dispatch, mixed-platform tasks and results.
- Versions 11–14 Windows validation of envelopes, benign transformations,
  transport and allowed safe/default-deny interfaces.
- Version 15-equivalent Windows/Linux demonstration, guides and regression evidence.

All security boundaries remain in [security-boundary.md](security-boundary.md).
No EDR disabling, kernel tampering, injection, escalation exploit, covert
persistence, vulnerable-driver exploitation or other prohibited operation becomes
allowed by changing platform or version order.

## Exact next request

Use `$speckit-specify` with **Version 5: Portable Linux Read-Only Forensic Collector
Library** from the updated prompt pack. Then request its plan, tasks and scoped
implementation. Do not copy the original Ubuntu-only Version 5 or deferred
Version 4 prompt. This amendment itself does not begin those workflows.
