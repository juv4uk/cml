# Shared GPU admission lease v1

## Purpose

CML owns the single physical GPU resource. A client that must run a bounded CUDA workload outside the CML worker execution protocol must first acquire a CML-owned admission lease.

The lease is a resource-ownership mechanism only. It does not execute client commands and does not define SENS meaning.

## Ownership rule

At most one holder may own the physical GPU admission at a time.

Worker-internal CUDA operations and external bounded CUDA clients use the same admission state. No repo-local runner lock is sufficient evidence of global ownership.

## Client identity

Every acquisition carries four bounded provenance fields:

- repository
- run_id
- job
- case_id

Fields are ASCII, non-empty, and bounded to prevent unbounded metadata growth.

## Lifecycle

```text
acquire(provenance, ttl)
    -> granted(lease_id, wait_ns, ttl)

renew(lease_id, ttl)
    -> renewed(lease_id, ttl)

release(lease_id)
    -> released(lease_id)
```

A lease expires automatically when its bounded TTL elapses without renewal. A stale holder cannot release or renew a successor's lease.

The client is responsible for bounded work duration and periodic renewal before expiry.

## Scheduling

Admission is serialized by the CML-owned resource mechanism. A client that cannot acquire immediately waits in the shared admission path rather than starting CUDA concurrently.

The implementation must not introduce a second GPU scheduler in a consumer repository.

## Evidence

Acquisition evidence must expose at least:

- lease id;
- client provenance;
- admission wait time;
- granted TTL;
- resource identity.

Release/expiry evidence must identify the same lease and provenance.

For CUDA execution, queue/admission wait, host/service time, transfer time, and kernel time remain separate measurements.

## Failure behavior

- malformed provenance: reject;
- invalid/expired lease id: reject;
- expired holder: reclaim;
- unavailable physical GPU: explicit blocked/error result;
- GPU-classified work never silently falls back to CPU.

## Security boundary

The lease protocol does not accept shell commands, executable paths, arbitrary process specifications, or arbitrary CUDA source.

The client owns its already-admitted bounded workload; CML owns only the physical GPU admission/resource boundary.

## First consumer

`juv4uk/sens-futhark#99` will use this mechanism to prevent direct Futhark CUDA launches from bypassing the shared physical-GPU owner.

## Falsification

Two independent clients must never simultaneously hold a valid lease for the same physical GPU. A crashed holder must cease to block admission after the bounded TTL.