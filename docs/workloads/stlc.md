# STLC

The STLC workload tests a simply-typed lambda calculus in de Bruijn form with booleans. Source is `workloads/Rocq/STLC/Src/Impl.v`; spec is `workloads/Rocq/STLC/Src/Spec.v`; reference counterexamples live in `docs/workloads/stlc.json`.

Syntax:

```coq
Inductive Typ := TBool | TFun : Typ -> Typ -> Typ.

Inductive Expr :=
| Var   : nat -> Expr
| Bool  : bool -> Expr
| Abs   : Typ -> Expr -> Expr
| App   : Expr -> Expr -> Expr.
```

Typing (`getTyp`) walks the term in a de Bruijn context `Ctx = list Typ`; `Var n` looks up `nth_error ctx n`. Reduction (`pstep`) is parallel β: it reduces under binders and, for `App (Abs _ e1) e2`, reduces `e1` and `e2` first, then applies `substTop e2' e1'`.

## Properties

Both live in `Spec.v`. Both are preservation: a well-typed closed term should stay well-typed at the same type after one step (`prop_SinglePreserve`) or after up to 40 steps (`prop_MultiPreserve`).

```coq
Definition prop_SinglePreserve (e: Expr) : option bool :=
  isJust (mt e) -=>
    t' <- (mt e) ;;
    Some (mtypeCheck (pstep e) t').

Definition prop_MultiPreserve (e: Expr) : option bool :=
  isJust (mt e) -=>
    t' <- mt e ;;
    Some (mtypeCheck (multistep 40 pstep e) t').
```

The guard `isJust (mt e)` means ill-typed inputs are discarded; every counterexample is a closed term with a computed type `t'` that `pstep` or `multistep` fails to preserve.

Substitution is where everything happens. The original implementation uses three mutually dependent pieces:

- `shift d e` — add `d` to every free variable of `e`. Internally calls `go c e` with a cutoff `c`, initially `0`, incremented on each `Abs`. The intent: "only shift variables with index `≥ c`".
- `subst n s e` — replace `Var n` in `e` with `s`, shifting `s` by one and bumping `n` as we cross each `Abs`.
- `substTop s e = shift (-1) (subst 0 (shift 1 s) e)` — the β-reduction helper used by `pstep`.

Every mutation in this workload is a local edit to one of the three. All ten of them fail both `SinglePreserve` and `MultiPreserve`; the counterexample shape changes with which invariant got broken.

## Shift mutations

Original:

```coq
Definition shift (d: Z) (ex: Expr) : Expr :=
  let fix go (c: Z) (e: Expr) :=
    match e with
    | Var n =>
        if n <? Z.to_nat c then Var n
        else Var (Z.to_nat ((Z.of_nat n) + d))
    | Bool b => Bool b
    | Abs t e => Abs t (go (1 + c)%Z e)
    | App e1 e2 => App (go c e1) (go c e2)
    end in
  go 0%Z ex.
```

### `shift_var_none` — variable case never shifts

```coq
| Var n => Var n
```

Free variables never move. When β-reduction performs `substTop`, the outer `shift 1 s` leaves `s` unchanged, so a free variable inside `s` that was supposed to point past the new binder now still points at the old one.

Counterexample `(Abs TBool (App (Abs TBool (Var 1)) (Bool #t)))` has type `TFun TBool TBool`. The inner `App (Abs TBool (Var 1)) (Bool #t)` β-reduces to `substTop (Bool #t) (Var 1)`; `Var 1` should become the surrounding free variable (originally the outer `Abs`'s binder), via the shift-by-1-then-shift-back-by-1 dance. With the mutant, those shifts are no-ops, and the net effect is a wrong index that no longer type-checks under the outer binder.

### `shift_var_all` — variable case always shifts

```coq
| Var n => Var (Z.to_nat (Z.of_nat n + d))
```

Now bound variables get shifted too — the cutoff `c` is ignored. Counterexample `(App (Abs TBool (Abs TBool (Var 0))) (Bool #t))`: `Var 0` under two `Abs` binders is bound, and β-reducing the outer application runs `substTop` over the body `(Abs TBool (Var 0))`, which calls `shift 1` on the argument `Bool #t` (harmless) but then, under the inner `Abs`, eventually runs `shift (-1)` on a term still containing `Var 0`. With the mutant, that last shift turns `Var 0` into a now-out-of-range `Var (−1 wrapped to 0)` under a context that lost its outer entry — typing fails.

### `shift_var_leq` — cutoff comparison off by one

```coq
| Var n =>
    if (Z.leb (Z.of_nat n) c) then Var n
    else Var (Z.to_nat (Z.of_nat n + d))
```

The test is `n ≤ c` instead of `n < c`. A variable exactly at the cutoff is left alone when it should be shifted. Needs a slightly deeper term (the counterexample nests three binders) to force a variable to land exactly at the cutoff during substitution.

### `shift_abs_no_incr` — cutoff doesn't grow under binders

```coq
| Abs t e => Abs t (go c e)
```

Recursing under `Abs` without bumping the cutoff means bound-from-the-outside variables and newly-bound ones are treated uniformly. Same minimal shape as `shift_var_all`: `(App (Abs TBool (Abs TBool (Var 0))) (Bool #t))`.

## Subst mutations

Original:

```coq
Fixpoint subst (n: nat) (s: Expr) (e: Expr) : Expr :=
  match n, s, e with
  | n, s, (Var m) =>
      if m =? n then s
      else Var m
  | _, _, (Bool b) => Bool b
  | n, s, (Abs t e) =>
      Abs t (subst (n + 1) (shift 1 s) e)
  | n, s, (App e1 e2) => App (subst n s e1) (subst n s e2)
  end.
```

### `subst_var_all` — substitutes at every variable

```coq
| n, s, (Var m) => s
```

Every `Var m` becomes `s`, regardless of whether `m = n`. This obviously breaks typing: variables that should have type X get replaced with a term of potentially different type. The counterexample uses `(TFun TBool TBool)` on the outer `Abs` so the substitution puts a `(Bool #t)` in a position that expected a function.

### `subst_var_none` — substitutes nowhere

```coq
| n, s, (Var m) => Var m
```

`subst` is now a no-op on variables. The trivial β-redex `(App (Abs TBool (Var 0)) (Bool #t))` should step to `Bool #t`, but the mutant runs `substTop (Bool #t) (Var 0) = shift (-1) (subst 0 (shift 1 (Bool #t)) (Var 0)) = shift (-1) (Var 0)`; under the mutant that's `Var (0 + (-1))` which wraps via `Z.to_nat` back to `Var 0` — a free variable in an empty context, which no longer type-checks.

### `subst_abs_no_shift` — does not shift the argument when crossing a binder

```coq
| n, s, (Abs t e) => Abs t (subst (n + 1) s e)
```

The argument `s` should have its indices bumped once per binder crossed to avoid capture. Dropping the `shift 1 s` means the occurrences of free variables in `s` that end up under a new binder now reference the new binder instead of their original target. Counterexample `(App (Abs TBool (App (Abs TBool (Abs (TFun TBool TBool) (Var 1))) (Var 0))) (Bool #t))` forces a double crossing where the `Var 1` in the innermost `Abs` should refer to a specific outer binder; without the shift, it captures incorrectly.

### `subst_abs_no_incr` — does not bump `n` across binders

```coq
| n, s, (Abs t e) => Abs t (subst n (shift 1 s) e)
```

Now `subst 0` under an outer binder still looks for `Var 0` under the inner binder, matching the inner bound variable instead of the de Bruijn'd outer one. This is the classic "captures the wrong variable" bug: counterexample `(App (Abs TBool (Abs (TFun TBool TBool) (Var 1))) (Bool #t))` demonstrates the mismatch.

## substTop mutations

Original:

```coq
Definition substTop (s: Expr) (e: Expr) : Expr :=
  shift (-1) (subst 0 (shift 1 s) e).
```

### `substTop_no_shift` — drops both shifts

```coq
Definition substTop (s: Expr) (e: Expr) : Expr := subst 0 s e.
```

Equivalent, in effect, to `shift_var_none` + `substTop`'s loss of the outer decrement. Free variables in `s` don't get bumped up before substitution, and the body doesn't get decremented afterwards, so every outer free variable ends up one level too deep. Counterexample `(Abs TBool (App (Abs TBool (Var 1)) (Bool #t)))` — same shape as the shift-based failure.

### `substTop_no_shift_back` — drops only the final shift

```coq
Definition substTop (s: Expr) (e: Expr) : Expr := subst 0 (shift 1 s) e.
```

`s` is shifted up correctly, but the post-shift-down is gone, so the body keeps its old indices. Free variables past the β-contracted binder are all now off by one. Counterexample `(Abs TBool (App (Abs TBool (Var 0)) (Var 0)))`: β-reducing the inner redex should substitute the second `Var 0` (outer-bound) into the body without shifting the surrounding context; the mutant leaves the body shifted up by one, so what was a well-typed `TBool` becomes a reference to an out-of-range index.

## Reading the counterexamples

Counterexamples in `docs/workloads/stlc.json` are S-expressions matching the `ShowExpr` encoder:

- `Var n` / `Bool #t` / `Bool #f`
- `Abs T e` with `T` = `TBool` or `(TFun T1 T2)`
- `App e1 e2`

Every recorded counterexample is a closed well-typed term of the type `mt e` would compute; one step (or a chain of steps) under the mutated helper produces a term that no longer type-checks at that type. A task is solved when the strategy finds any closed well-typed input on which preservation breaks under the mutant.
