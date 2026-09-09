# Local commits ahead of origin (Contract 3.0 + claim-authority)

## Apply the already-pushed unified patch (src only)

```bash
curl -sL https://raw.githubusercontent.com/juv4uk/cml/master/evidence/semantic-ir-recovery/contract3-src-patches.diff | patch -p1
```

## Then update compatibility.my claim-authority

Copy from local sandbox or ensure these markers exist under `(language . ...)`:

- `(claim-authority . ((global-contract . (2 0)) ...))`
- `(contract-3.0-gap . ((status . partial-c-backend) ...))`
- `(contract-5.0-decimal-separator . ((status . partial-reader) ...))`
- `(contract-6.0-canon-reservation . ((status . partial-static-rejection) ...))`

## Files still needing full content on origin

| File | Markers |
|------|---------|
| `src/compiler.rs` | `Arity:`, `Unsupported:`, `NumericOverflow:` |
| `src/c_backend.rs` | `DivisionByZero`, `checked_long_add` |
| `compatibility.my` | `claim-authority` |

## Recommended: local git push

```bash
cd /path/to/cml
git pull --rebase origin master   # may need merge of API commits
# OR from sandbox worktree that has HEAD 79eef6d:
git push origin master
```

Sandbox local HEAD (with all changes): `79eef6d`
