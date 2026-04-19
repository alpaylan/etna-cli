# RBT

The RBT workload extends BST with red–black balancing. Trees carry a color on each node and must satisfy the usual RBT invariants in addition to the BST order. Mutations live in `workloads/Rocq/RBT/Src/Impl.v`; the property-based spec lives in `workloads/Rocq/RBT/Src/Spec.v`; reference counterexamples are in `docs/workloads/rbt.json`.

Tree syntax:

```coq
Inductive Color := R | B.
Inductive Tree :=
| E : Tree
| T : Color -> Tree -> Z -> Z -> Tree -> Tree.
```

i.e. `T color left key val right`. Values here are `Z` (signed integers), unlike BST which uses `nat`.

## Properties

- `prop_InsertValid`, `prop_DeleteValid` — the result satisfies the RBT invariant: BST order plus "root is black", "no red child under a red parent", and "equal black-heights on all root-to-leaf paths".
- `prop_InsertPost`, `prop_DeletePost` — `find k` after the operation matches the map post-condition.
- `prop_InsertModel`, `prop_DeleteModel` — `toList` matches the list-level operation on the model.
- `prop_InsertInsert`, `prop_InsertDelete`, `prop_DeleteInsert`, `prop_DeleteDelete` — metamorphic pairs lifted from BST.

Union is not in the RBT surface API, so the union-related properties from the BST workload are absent here.

## BST-style mutations: `insert_1`, `insert_2`, `insert_3`, `delete_4`, `delete_5`

The insert-with-non-empty-tree mutations mirror the BST ones almost verbatim, but on the `ins` inner loop of the RBT insert.

Original:

```coq
| x, vx, (T rb a y vy b) =>
  if x <?? y then balance rb (ins x vx a) y vy b
  else if y <?? x then balance rb a y vy (ins x vx b)
  else T rb a y vx b
```

- `insert_1` collapses the non-empty case to `T R E x vx E`, discarding the whole subtree — every map-level insert property fails on a singleton tree.
- `insert_2` drops the `y <?? x` branch, so keys `> y` overwrite the current node's value instead of recursing right. This is visible even on singletons (`((T B E -1 1 E) 0 0 0)` for `InsertPost`).
- `insert_3` keeps `vy` (old value) on the equal-key case instead of using the new `vx`, so a "rewrite at existing key" is a no-op.

`delete_4` and `delete_5` similarly mirror BST. In `delete_4` both recursive branches lose their surrounding `delLeft`/`delRight` wrappers, so deleting from one side discards the other side and the current node. `delete_5` swaps the `<??` comparisons, flipping left/right. The counterexamples for each can be read directly from the JSON; unlike the RBT-specific mutations below, these do **not** break `InsertValid` / `DeleteValid` — the result is still a structurally valid RBT, just the wrong one. That's why the tasks for these mutations only list `Post`, `Model`, and pair properties.

## Color mutations in `balance`

Red–black balance rebuilds a subtree to restore the no-red-red invariant. The canonical forms are kept intact, but two of them are instrumented:

### `swap_cd` (shape bug in `balance`, used only internally, not in the task list)

Left-left red grandchild case:

```coq
(* original *)
| B, (T R (T R a x vx b) y vy c), z, vz, d =>
    T R (T B a x vx b) y vy (T B c z vz d)
(* mutant swap_cd *)
| B, (T R (T R a x vx b) y vy c), z, vz, d =>
    T R (T B a x vx b) y vy (T B d z vz c)
```

`c` and `d` get swapped around `z`, violating BST order. Not referenced from the JSON task list but the source shows the mutation is wired.

### `swap_bc` (symmetric right-right case)

```coq
(* original *)
| B, a, x, vx, (T R (T R b y vy c) z vz d) =>
    T R (T B a x vx b) y vy (T B c z vz d)
(* mutant *)
| B, a, x, vx, (T R (T R b y vy c) z vz d) =>
    T R (T B a x vx c) y vy (T B b z vz d)
```

## `miscolor_insert` — leaves on insert are black

Original:

```coq
| x, vx, E => T R E x vx E
```

Mutant:

```coq
| x, vx, E => T B E x vx E
```

A freshly inserted leaf is painted black instead of red. The surrounding `insert` wraps the whole result in `blacken`, so the root stays black either way, but the black-height invariant fails as soon as one side of a node has a new black leaf and the other side does not. `InsertValid` on `((T B E 0 0 E) 1 0)` is the minimum: inserting key=1 under the root produces `T B E 0 0 (T B E 1 0 E)`, whose two root-to-leaf paths are 2 black and 1 black, respectively.

## `miscolor_delete` — skips the final blacken

```coq
(* original *)
Definition delete (x: Z) (t: Tree) : option Tree :=
  t' <- del x t fuel ;;
  Some (blacken t')

(* mutant *)
Definition delete (x: Z) (t: Tree) : option Tree :=
  del x t fuel.
```

`del` can legitimately return a red root; the spec requires the root to be black, which `blacken` enforces. The mutant skips it. `DeleteValid` on `((T B E 0 0 (T R E 1 1 E)) 2)` — deleting a missing key from a tree whose `del` run produces a red root — fails.

## `miscolor_balLeft` — forgets to redden `c` before rebalancing

In the recursive rebalancing helpers, a subtree whose color needs flipping before it is passed to `balance` is wrapped with `redden`:

```coq
(* original balLeft, third clause *)
| bl, x, vx, (T R (T B a y vy b) z vz c) =>
    c' <- (redden c) ;;
    Some (T R (T B bl x vx a) y vy (balance B b z vz c'))
(* mutant *)
| bl, x, vx, (T R (T B a y vy b) z vz c) =>
    Some (T R (T B bl x vx a) y vy (balance B b z vz c))
```

`redden c` pushes one black level off `c`; skipping it leaves `c` one black-height too tall, and the resulting subtree violates the equal-black-height invariant. Triggers `DeleteValid` and `DeleteDelete` once the delete path reaches the left-rebalance branch — the counterexamples in the JSON are tall (~7-node) trees because the delete has to land in the exactly-this-case branch of `delLeft`.

## `miscolor_balRight`

Symmetric mutation on the right-rebalance helper:

```coq
(* original balRight, third clause *)
| (T R a x vx (T B b y vy c)), z, vz, bl =>
    a' <- redden a ;;
    Some (T R (balance B a' x vx b) y vy (T B c z vz bl))
(* mutant *)
| (T R a x vx (T B b y vy c)), z, vz, bl =>
    Some (T R (balance B a x vx b) y vy (T B c z vz bl))
```

Same failure mode on the right side, same counterexample shape.

## `miscolor_join_1` — wrong colors when both subtrees are red

`_join` merges two subtrees of equal black-height. When both are red and the recursive join yields a red-rooted tree, the original produces a red-red-red chain on the spine:

```coq
(* original, R/R case *)
| Some (T R b' z vz c') =>
    Some (T R (T R a x vx b') z vz (T R c' y vy d))
(* mutant miscolor_join_1 *)
| Some (T R b' z vz c') =>
    Some (T R (T B a x vx b') z vz (T B c' y vy d))
```

The middle nodes are painted black instead of red. Because the outer context relies on those being red to keep the black-heights equal, `DeleteValid` fails on inputs that force delete to go through this join branch; the counterexample is a 10+ node tree because every earlier branch of `_join` has to be skipped.

## `miscolor_join_2` — wrong colors when both subtrees are black

Symmetric. B/B branch, flipping blacks to reds:

```coq
(* original *)
| Some (T R b' z vz c') =>
    Some (T R (T B a x vx b') z vz (T B c' y vy d))
(* mutant miscolor_join_2 *)
| Some (T R b' z vz c') =>
    Some (T R (T R a x vx b') z vz (T R c' y vy d))
```

Fails `DeleteValid` and `DeleteDelete`.

## No-balance insert mutations

The two variants of `no_balance_insert_*` skip the rebalancing call on one side:

```coq
(* no_balance_insert_1 *)
if x <?? y then T rb (ins x vx a) y vy b
else if y <?? x then balance rb a y vy (ins x vx b)
else T rb a y vx b
(* no_balance_insert_2 *)
if x <?? y then balance rb (ins x vx a) y vy b
else if y <?? x then T rb a y vy (insert x vx b)
else T rb a y vx b
```

Without `balance`, a red-red chain slips through whenever insert lands on that side. Both fail `InsertValid` on 2-node trees (the minimum size where an unbalanced insert causes a red-red parent/child). They also fail `DeleteInsert` and `InsertDelete`, because the mispainted result leaks into subsequent operations.

## Reading the counterexamples

`docs/workloads/rbt.json` tasks use the S-expression encoding defined by `ShowTree` in `Impl.v`:

- Colors as `R` or `B` — sometimes spelled `(R)` / `(B)` when the encoder output is re-parsed.
- `(T c l k v r)` — a node with color `c`, subtree `l`, key `k`, value `v`, subtree `r`.
- Operation arguments follow the tree, e.g. `((T B E -5 0 E) -5)` is "tree, then key=-5" for `DeleteValid`.

A task is solved when the strategy finds any RBT input on which the property returns `false` against the mutant.
