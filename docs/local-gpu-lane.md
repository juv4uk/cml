# Local GPU execution lane

This repository owns the single self-hosted GPU execution lane for the SENS ecosystem.

## Topology

```text
GitHub Actions (juv4uk/cml only)
        |
        v
wsm-i5-6400
self-hosted Linux/X64 runner
        |
        +--> direct CML CUDA tests
        |
        +--> /run/cml-gpu-worker/worker.sock
              |
              v
      cml-gpu-worker.service
              |
              v
        GTX 1050 Ti
```

The runner and persistent CUDA worker are WSL **system services**, not per-login user services.

- runner unit: `actions.runner.juv4uk-cml.wsm-i5-6400.service`
- worker unit: `cml-gpu-worker.service`
- worker socket: `/run/cml-gpu-worker/worker.sock`
- hardware witness: GTX 1050 Ti, compute capability 6.1, 4 GiB

Windows keeps the Ubuntu WSL instance alive with a per-user Startup keepalive process. The keepalive has no semantic role; it only prevents WSL from shutting down while the GitHub listener is idle.

## Runner policy

The old per-repository runner fleet is retired. Only the CML repository should have the GPU runner registration.

Required labels:

```text
self-hosted
Linux
X64
gpu
gtx-1050-ti
cuda-12.6
```

Physical-GPU workflows remain `workflow_dispatch` while the lane is being requalified. Do not enable untrusted pull-request execution on this runner.

## One-card serialization

All GPU workflows share:

```yaml
concurrency:
  group: gpu-gtx-1050-ti
  cancel-in-progress: false
```

GitHub concurrency retains at most one running and one pending run for a key. A newer pending run can replace an older pending run, so GPU workflows must be dispatched serially when exhaustive evidence is required.

## Authority boundary

CUDA is an execution mechanism only.

```text
SENS oracle / semantics
        |
        v
CML admission + cost model
        |
        +--> CPU
        |
        `--> CUDA
```

GPU availability, speed, device residency, fusion, transfer cost, and the local hardware profile cannot define SENS meaning.

## Worker validation

`.github/workflows/gpu-worker-live.yml` has two independent phases:

1. probe the long-lived system worker;
2. build the exact workflow head and run it on a private temporary socket.

This separation proves current source without giving a GitHub job root authority to replace a system service.

## Placement

Do not use a universal element-count threshold. Placement must consume measured mechanism costs such as transfer bytes, arithmetic intensity, residency, chain depth, requested host materialization, and CPU worker count.

See #468 and #469.
