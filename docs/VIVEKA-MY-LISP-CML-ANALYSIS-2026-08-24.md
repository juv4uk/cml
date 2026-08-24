# Viveka: live analysis of `my-lisp` and `cml`

Date: 2026-08-24  
Author: Viveka (Codex), architect of epistemic integrity  
Status: **direct repository audit**  
Scope: read-only analysis; no source, contract, fixture, task, or evidence files were changed

Inspected revisions:

- `my-lisp`: `e047425fe39870e391767ae97f5bbc0653cb74b1`
- `cml`: `d3eb4afeba53e9e19a2abcb76cd78e2efc50c9e7`

Repository contracts, current source, and executable fixtures outrank this report. The report distinguishes confirmed observations from architectural recommendations.

## Executive conclusion

The heterogeneous-compiler direction is real, not merely aspirational: CML has a shared IR and two physical consumers, the fpga-lisp emitter and the C emitter. However, the immediate blocker is neither CUDA nor a new Compute IR. The authority chain between the current `my-lisp` contract, its conformance fixtures, and CML is inconsistent.

The highest-value next milestone is:

```text
CML-CONTRACT-REALIGNMENT-M0
```

Its purpose is to restore one executable semantic truth across evaluator, fixtures, and compiler before adding another backend.

## 1. Critical finding: contract 2.1 contradicts a Tier-1 fixture

`my-lisp/language-contract.my` declares version 2.1 and explicitly ratifies lexical shadowing of builtins:

```text
Builtins bootstrap global env as ordinary values.
Any scope may redefine them; inner bindings shadow outer.
```

Relevant contract location: `my-lisp/language-contract.my:72–86`.

But `my-lisp/tests/fixtures/conformance.my:132` still contains:

```lisp
(let ((car (lambda (x) (quote shadowed))))
  (car (quote (1 2))))
```

with expected result `1` and a note claiming the seven primitives are syntax and cannot be shadowed.

The expression was run directly through the canonical evaluator in the declared Guix environment:

```sh
printf '%s\n' \
  '(let ((car (lambda (x) (quote shadowed)))) (car (quote (1 2))))' \
  | guix shell --pure -m manifest.scm -- cargo run -q -p my-lisp-cli
```

Observed result:

```text
shadowed
```

Therefore the live state is:

```text
language-contract 2.1 = shadowed
canonical evaluator   = shadowed
conformance fixture   = 1
CML/C backend         = 1 through that fixture
```

Status: **CONFIRMED semantic-authority conflict**.

This is more serious than ordinary CML lag. CML's “10/10 constitutive fixtures” test passes, but at least one fixture represents pre-2.1 semantics.

## 2. Contract drift between the repositories

Current declarations:

```text
my-lisp language contract: 2.1
cml supported contract:     2.0
```

`cml/compatibility.my` still declares `(contract . (2 0))` and pins an older tested my-lisp SHA. That is honest as a supported-version claim, because CML has not implemented first-class builtin semantics.

A clean Guix test run was executed:

```sh
guix shell --pure -m manifest.scm -- \
  cargo test \
    --test revision_contract_test \
    --test ir_lowering_test \
    --test c_backend_conformance_test
```

Results:

```text
c_backend_conformance_test: PASS (1 test)
ir_lowering_test:           PASS (2 tests)
revision_contract_test:     FAIL (1 pass, 1 failure)
```

The failing test correctly detected that `compatibility.my` says 2.0 while the live upstream contract says 2.1.

However, simply editing CML's claim to 2.1 would be incorrect. It would turn visible incompatibility into a false compatibility claim. A version-aware compiler needs three distinct fields:

```text
supported-contract
observed-upstream-contract
tested-upstream-revision
```

The current test assumes CML must always claim the latest live contract. That model does not permit a compiler to legitimately support an older language version. A durable historical 2.0 contract/fixture boundary is needed if 2.0 remains supported.

Status: **CONFIRMED drift; test policy requires redesign rather than a version-only edit**.

## 3. What CML already is

The imported ChatGPT analysis was correct about the major structural facts.

Confirmed pipeline:

```text
source
→ parser
→ AST
→ macro expansion
→ lower
→ shared Ir
→ FPGA emitter or C emitter
```

Confirmed properties:

- `src/ir.rs` explicitly defines a backend-neutral intermediate representation.
- `src/compiler.rs` and `src/c_backend.rs` consume the same `Ir`.
- AST no longer reaches either code generator directly.
- CML has a real C runtime/emitter and a real fpga-lisp emitter.
- The C tests invoke `gcc` and execute the produced binary.
- `docs/heterogeneous-backends.md` explicitly assigns CML the shared middle-end role.

Therefore CML is already a real multi-backend compiler architecture, not merely an FPGA assembler generator.

Status: **CONFIRMED**.

## 4. Why the current IR is not yet a full semantic IR

The common IR is real, but its value and operation surface still reflects the older FPGA-oriented compiler subset.

Current key representations:

```text
Quoted: Int(i64), Sym, Str, Nil, List, DottedList

PrimOp:
  Add, Cons, Car, Cdr, Eq, Atom, EqualP

Ir:
  Int, Nil, True, Var, Quote, Lambda, App,
  Cond, Let, Def, Prim
```

Important limitations:

- only `i64` integer IR values;
- source strings are lowered to uppercase target symbols;
- builtin names in head position are dispatched syntactically before environment lookup;
- no value-level `Builtin` representation;
- no rationals, inexact-number representation, vectors, effects, or representation metadata;
- no buffer, index, map, reduce, scan, or parallel-region abstraction.

The IR is shared and backend-independent at the module boundary, but it is not yet sufficient to represent the complete current `my-lisp` semantics or a GPU-oriented Compute IR.

Status: **CONFIRMED**.

## 5. Current `my-lisp` surface is substantially wider

The live `my-lisp::Value` model includes:

- NIL and booleans;
- exact and inexact numeric values;
- arbitrary-precision exact `Rational` backed by a custom `BigInt`;
- strings and symbols;
- pairs;
- closures and macros;
- first-class builtins;
- mutable vectors;
- opaque host resource handles.

The first-class builtin acceptance tests cover, among other cases:

```lisp
(def f +)
(f 20 22)

(reduce + 0 (list 1 2 3))

((lambda (+) (+ 2 3))
 (lambda (a b) (* a b)))
```

CML cannot currently represent these semantics faithfully because `+`, `car`, and the other recognized primitives are lowered as closed `PrimOp` syntax rather than ordinary callable environment values.

Status: **CONFIRMED semantic gap**.

## 6. Language stability assessment

The proposed `LANGUAGE-STABLE` gate is architecturally sound, but the repository has not yet satisfied it.

Observed reasons:

- the language contract recently moved from 2.0 to 2.1;
- first-class builtin resolution changed a core callable boundary;
- the language axioms document still identifies itself as a draft;
- ratification remains deferred until release 1.0;
- the exact meaning of the 1.0 release remains open in `PLAN.md`;
- GC has detailed design/consensus documents but remains an active architectural boundary;
- contract 2.1 and the canonical conformance fixture set currently disagree.

Before stability, CML should remain an experimental witness that tests whether the semantics can be compiled. It must not silently dictate the language through an early ABI or IR freeze.

Status: **CONFIRMED that the proposed stability gate is not currently met**.

## 7. C backend assessment

The C backend is a genuine and valuable compiled proof for a selected subset.

Confirmed capabilities include:

- tagged runtime values;
- closures with captured environments;
- fixed and variadic calls;
- cons-based environments;
- self-recursive definitions via placeholder/backpatch;
- structural `equal?`;
- generated C compiled by real `gcc`.

But it is not yet a reference implementation of full compiled my-lisp:

- allocated runtime objects use `malloc` and are never freed;
- runtime tags cover only NIL, TRUE, INT, SYM, CONS, and CLOSURE;
- builtins are not first-class runtime values;
- primitives bypass normal environment resolution;
- conformance testing skips all error fixtures and inexact cases;
- parser failures in the conformance loop can be silently skipped;
- the test only asserts that at least ten cases were executed;
- case differences are erased before comparison;
- nested generated C function arguments may rely on C's unspecified argument-evaluation order, which threatens future equivalence for errors, allocation identity, and effects.

Therefore:

```text
C backend = real compiled proof for a narrow subset
C backend ≠ reference compiled my-lisp execution path
```

Status: **CONFIRMED**.

## 8. Assessment of the imported ChatGPT proposals

### Strong and consistent with live code

- CML should remain the single shared middle-end.
- CUDA or GPU concepts should not enter canonical language semantics.
- C backend completeness should precede GPU implementation.
- A complete compiler frontend does not require every physical backend to support the entire language.
- Unsupported backend features need explicit, versioned statuses.
- A future Compute IR should sit below the semantic IR rather than replacing it.
- Rust plus `wgpu` is a plausible portable GPU path, with CUDA reserved for later measured specialization.

### Overstated or requiring correction

- “A pure function is trivially data-parallel” is false as a general rule. Purity is helpful but insufficient; dependencies, recursion, allocation, divergence, data layout, and complexity still matter.
- CML does not yet perform a full semantic analysis; current lowering is mostly a structural AST-to-IR translation.
- C-backend conformance does not approach the entire stable contract.
- Semantic stability must not require freezing CML's internal ABI or IR permanently. Representation may evolve while observable semantics remain stable.
- Neither CUDA nor `wgpu` is the next blocking milestone.

## 9. Recommended milestone: CML-CONTRACT-REALIGNMENT-M0

### Phase 1 — repair the authority layer

1. Resolve the contract 2.1 versus `car`-shadowing fixture conflict in `my-lisp`.
2. Add the complete first-class-builtin acceptance matrix to canonical conformance fixtures.
3. Regenerate `my-lisp-constitution.my` and exercise its synchronization guards.
4. Preserve the resolved fixtures as versioned semantic evidence.

### Phase 2 — make compatibility truthful

5. Separate CML's supported contract from the current observed upstream contract and tested revision.
6. Keep CML at supported contract 2.0 until 2.1 behavior is implemented and verified.
7. Make contract drift visible without forcing an unearned version claim.

### Phase 3 — implement contract 2.1 in CML

8. Add a first-class builtin value or an equivalent semantically faithful callable representation.
9. Resolve builtins through the ordinary lexical environment.
10. Support lexical shadowing.
11. Preserve the syntax-only special-form boundary.
12. Add explicit not-callable behavior and error comparison.

### Phase 4 — make conformance fail closed

13. Do not silently continue on fixture parse failures.
14. Record one explicit result for every fixture.
15. Compare successful values and contractual errors.
16. Report exact counts and explicit states:

```text
SUPPORTED
UNSUPPORTED-BY-DESIGN
UNIMPLEMENTED
UNVERIFIED
FAILED
```

17. Never use a bare `-` to conflate these states.

### Deferred work

Only after realignment:

- extend numeric and runtime coverage;
- establish the C backend as the reference compiled path;
- add execution-shape/effect analysis;
- design a lower Compute IR;
- benchmark CPU versus portable GPU execution;
- decide whether a specialized CUDA backend is justified.

## Final verdict

`my-lisp` already has a strong and growing semantic system, but it has not yet passed its own stability gate. CML already has the architecture of a real heterogeneous compiler, but it currently compiles an older and narrower model of the language.

The next decisive improvement is not another backend. It is one non-contradictory executable contract shared by:

```text
canonical evaluator
       =
versioned fixtures
       =
CML compiled behavior
```

Once that equality is evidence rather than aspiration, Compute IR and GPU work will rest on a trustworthy foundation.
