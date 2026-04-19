# IFC

The IFC workload tests an information-flow control abstract machine. Unlike BST / RBT / STLC, mutations are *generated* rather than hand-edited — there is a procedure in `workloads/Rocq/IFC/Src/Mutate.v` that produces all permitted weakenings of the rule table, and each generated table is a distinct mutant. The source of truth for the machine lives in `workloads/Rocq/IFC/Src/` (notably `Machine.v`, `Rules.v`, `Instructions.v`); the property is SSNI, defined in `SSNI.v`.

## Machine model in brief

Values carry two-point security labels (`L` low, `H` high). An instruction's effect is constrained by a **rule**:

```coq
Record AllowModify (n: nat) := almod {
   allow    : rule_scond n;           (* when the rule fires *)
   labRes   : option (rule_expr n);    (* label on the returned value, if any *)
   labResPC : rule_expr n              (* label of the new PC *)
}.
```

`rule_expr` builds labels from `L_Bot`, `L_Var` (one of the input labels `lab1..lab4`, or `labpc`), and `L_Join`; `rule_scond` builds side conditions from `A_True`, `A_LE` (a `≤` on label expressions), `A_And`, `A_Or`. Together they express things like "allow if lab2 ≤ labpc, tag the result with lab1 ⊔ lab2, tag the new PC with labpc".

A **table** assigns an `AllowModify` rule to each opcode. The "correct" table is crafted so that runs of the machine satisfy noninterference.

## The property: SSNI

From `SSNI.v`:

```coq
Definition propSSNI_smart (t : table) (v : Variation) : option bool :=
    let '(Var lab st1 st2) := v in
    if indist lab st1 st2 && well_formed st1 && well_formed st2 then
      match fstep t st1 with
      | Some st1' =>
        if is_low_SState st1 lab then
          match fstep t st2 with
          | Some st2' => Some (indist lab st1' st2' && …)
          | _ => None
          end
        else …
      | _ => None
      end
    else None.
```

A **variation** is a pair of machine states that agree on the low-labeled data (`indist lab st1 st2`). SSNI ("single-step non-interference") says: for every such variation, stepping both states with the rule table `t` yields a pair that is still indistinguishable at the attacker's level. Mutations that leak information produce variations whose successors differ on low-labeled data — and a property-based tester finds them by sampling variations.

## How mutations are generated (`Mutate.v`)

Mutations in this workload are not hand-written clauses with `(*! … *)` markers. Instead, `Mutate.v` has a procedure that weakens each rule field and emits the resulting table as a new mutant.

Three weakenings, one per field of `AllowModify`:

### 1. Drop a disjunct from `allow`

```coq
Fixpoint break_scond n (c : rule_scond n) : list (rule_scond n) := …
Definition mutate_scond n (c : rule_scond n) : list (rule_scond n) :=
  let cs := break_scond c in
  match cs with
  | nil => []
  | _ => List.map (@and_sconds n) (drop_each cs)
  end.
```

`break_scond` flattens an `A_And` tree into a list of atomic `A_LE` conditions, `drop_each` produces the list of one-shorter sublists, and `and_sconds` re-ANDs them. Each resulting condition is strictly weaker than the original: the rule now fires in situations where it previously would not, which is where leaks come from.

### 2. Drop a disjunct from `labRes`

```coq
Definition mutate_expr n (e : rule_expr n) : list (rule_expr n) :=
  let es := break_expr e in
  match es with
  | nil => []
  | _ => List.map (@join_exprs n) (drop_each es)
  end.
```

`break_expr` flattens `L_Join`s into the set of `L_Var`s that go into a label; `drop_each` picks one to remove. A result labeled `lab1 ⊔ lab2` becomes either `lab1` or `lab2`, which means an operation that should taint its output with the union of two input labels now only tracks one of them — a classic explicit flow leak.

### 3. Drop a disjunct from `labResPC`, or move one into the result label

```coq
Definition mutate_pc n (ores : option (rule_expr n)) (epc : rule_expr n) : … :=
  …
  match ores with
  | Some eres =>
      let f xxs := (Some (L_Join eres (fst xxs)), join_exprs (snd xxs)) in
      List.map f (drop_each_but_not_lpc es)
  | None =>
      let f xs := (None, join_exprs xs) in
      List.map f (drop_each es)
  end.
```

The PC label is where *implicit* flows live. `drop_each_but_not_lpc` never drops the intrinsic `labpc` from the new-PC label (that would make the machine forget the current context entirely — the canonical broken rule, uninteresting as a leak target); instead it peels off one of the joined input labels. When there is a result label, the peeled disjunct is folded into the result so the total taint is preserved *locally* but the PC is under-tainted going forward, which is the shape of an implicit-flow bug.

### Putting it together

```coq
Definition mutate_rule n (r : AllowModify n) : list (AllowModify n) :=
  let a   := allow r in
  let res := labRes r in
  let pc  := labResPC r in
  (List.map (fun a' => almod a' res pc) (mutate_scond a))
    ++ (match res with
        | Some lres => List.map (fun lres' => almod a (Some lres') pc) (mutate_expr lres)
        | None => []
        end)
    ++ (List.map (fun respc => almod a (fst respc) (snd respc)) (mutate_pc res pc)).

Definition mutate_table t := mutate_table' t t.
```

`mutate_table` loops over every opcode, applies `mutate_rule` to the default rule for that opcode, and emits one new table per generated variant. Each resulting table is one weaker rule away from the original — concretely, either one allowed situation too many, one missing taint on a value, or one missing taint on the PC.

## Why the mutants break SSNI

Every mutant is a strictly weaker propagation rule for exactly one opcode. Given a variation `(st1, st2)` that runs the mutated opcode with inputs that differ at label `H`:

- **`allow` weakened** — the instruction fires on one side and faults on the other (because the original rule's extra side condition was needed to reject that input), so the post-step states diverge on whether the step happened at all.
- **`labRes` weakened** — the result in `st1'` is labeled `L` instead of `H` even though its value depended on an `H` input; indistinguishability was required to hold on `L`, so the post-step states are no longer indistinguishable.
- **`labResPC` weakened** — the new PC loses an `H` disjunct it needed to carry. The next branching instruction thinks it is in a public context, and writes made in the branch will leak the high-labeled guard into low memory on the next step.

A sampled variation is a witness when it exercises the exact opcode whose rule got weakened and stages the right data pattern. Counterexamples are therefore short machine traces — the generator's job is to build an initial `Variation` that reaches the mutated instruction with an H/L disagreement on exactly the label the mutant stops tracking.

## State of `docs/workloads/ifc.json`

`docs/workloads/ifc.json` is currently an empty array — mutation-level counterexamples are not checked in here the way they are for the tree / STLC workloads. The per-mutant naming scheme is algorithmic: `mutate_table` enumerates the cross product of opcodes × rule fields × droppable disjuncts, so the mutant identifier is typically the opcode plus a position index rather than a hand-named tag. To populate the JSON, record each interesting `(opcode, field, drop_index)` tuple alongside a witness `Variation`.

## Reading counterexamples (once populated)

A counterexample for IFC is not a single term; it is a variation, i.e. a pair of machine states that agree at the attacker's label plus the attacker's label itself. Well-formedness and initial indistinguishability are enforced by `propSSNI_smart`'s guard; every legitimate witness must pass those filters. The witness proves a leak by running one step under the mutant table and observing that `indist lab st1' st2'` now returns false.
