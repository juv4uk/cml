# Local GPU execution lane

This repository owns the single persistent CUDA execution service and CUDA admission/cost mechanism for the SENS ecosystem. Primary repo runners may advertise GPU capability, but they delegate eligible execution to the CML-owned service.

## Topology

```text
GitHub Actions (primary repo runners)
        |
        +--> repo-local CPU / orchestration work
        |
        `--> CML admission / shared CUDA mechanism
                    |
                    +--> direct CML CUDA witnesses
                    |
                    `--> /run/cml-gpu-worker/worker.sock
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

Runner registration is not semantic authority. The current owner policy allows all primary repo runners to advertise `gpu,gtx-1050-ti,cuda-12.6`; CML remains the owner of the single persistent CUDA execution service and admission/cost mechanism.

The CML repo itself keeps one CML-specific runner registration with labels:

```text
self-hosted
Linux
X64
gpu
gtx-1050-ti
cuda-12.6
```

Other primary repos may carry equivalent capability labels, but must not create competing CUDA semantics or persistent workers. Physical-GPU workflows remain controlled/manual while the lane is being requalified. Do not enable untrusted pull-request execution on the owner machine.

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

## Worker service source of truth

The repository-owned service contract is:

```text
systemd/cml-gpu-worker.service
```

It describes the already-existing **system** service. It is not a second worker.

The unit pins the live lane facts that must survive host recovery:

- `User=agents`;
- runtime socket `/run/cml-gpu-worker/worker.sock`;
- CUDA 12.6 toolkit paths and WSL driver-library precedence;
- `CML_CUDA_HOST_PROBE=/home/agents/ecosystem/scripts/cuda-host-profile.sh`;
- restartable long-lived `cml-gpu-worker serve`.

When the authorized host is available, re-install the unit atomically rather
than recreating it from memory:

```bash
sudo install -m 0644 systemd/cml-gpu-worker.service /etc/systemd/system/cml-gpu-worker.service
sudo systemd-analyze verify /etc/systemd/system/cml-gpu-worker.service
sudo systemctl daemon-reload
sudo systemctl restart cml-gpu-worker.service
sudo systemctl status --no-pager cml-gpu-worker.service
```

Restart **only** `cml-gpu-worker.service`. Do not restart WSL or create a
second worker. If a unit update needs rollback, restore the previously known
unit file, run `daemon-reload`, and restart the same service.

The unit carries no GitHub registration token, PAT, repository secret, or SENS
semantic authority. Host capability comes from the ecosystem probe; language
admission remains in CML/SENS layers above it.

## Worker validation

`.github/workflows/gpu-worker-live.yml` has two independent phases:

1. probe the long-lived system worker;
2. build the exact workflow head and run it on a private temporary socket.

This separation proves current source without giving a GitHub job root authority to replace a system service.

## Placement

Do not use a universal element-count threshold. Placement must consume measured mechanism costs such as transfer bytes, arithmetic intensity, residency, chain depth, requested host materialization, and CPU worker count.

See #468 and #469.
