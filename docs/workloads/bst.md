# BST

The BST workload tests a binary search tree supporting `insert`, `delete`, and `union` over nat keys and nat values. The specification treats a BST as an association list under the map laws, and each mutation is a specific edit to the workload's `Impl` module. Each `bst-*` workload repo carries its witnesses (minimum counterexamples per (mutation, property) pair) in `etna.toml` under `[[tasks]]`.

Tree syntax used throughout this doc:

```coq
Inductive Tree :=
| E
| T : Tree -> nat -> nat -> Tree -> Tree.
```

i.e. `T left key val right`, with `E` the empty tree.

## Properties (from `Spec.v`)

- `prop_InsertValid` / `prop_DeleteValid` / `prop_UnionValid` — the resulting tree is still a BST (sorted, strict inequalities on both sides).
- `prop_InsertPost` / `prop_DeletePost` / `prop_UnionPost` — `find k'` on the result matches the map-level post-condition.
- `prop_InsertModel` / `prop_DeleteModel` / `prop_UnionModel` — `toList (op t)` matches the corresponding list-level operation on `toList t`.
- `prop_InsertInsert`, `prop_InsertDelete`, `prop_DeleteInsert`, `prop_DeleteDelete`, `prop_InsertUnion`, `prop_DeleteUnion`, `prop_UnionDeleteInsert`, `prop_UnionUnionAssoc` — metamorphic laws between pairs of operations.

Each property is guarded by `isBST t -=>`, so the tester only counts false witnesses whose inputs were already valid BSTs.

## Insert mutations

Original clause in `Impl.v`:

```coq
Fixpoint insert (k : nat) (v: nat) (t : Tree) :=
  match t with
  | E => T E k v E
  | T l k' v' r =>
    if k <? k' then T (insert k v l) k' v' r
    else if k' <? k then T l k' v' (insert k v r)
    else T l k' v r
  end.
```

### `insert_1` — returns a singleton, drops the rest of the tree

```coq
| T l k' v' r => T E k v E
```

The non-empty case ignores `l`, `k'`, `v'`, `r` and returns a fresh singleton. Every call to `insert k v t` with a non-empty `t` therefore discards `t`.

How the counterexamples work (all valid BSTs, inserts highlighted):

- `prop_InsertPost` on `t = (T E 2 1 E), k = 0, k' = 2, v = 0` — `insert 0 0 (T E 2 1 E)` should produce a tree in which `find 2 = Some 1` (since `k ≠ k'`), but the mutant returns `T E 0 0 E`, where `find 2 = None`.
- `prop_InsertModel` on `t = (T E 1 0 E), k = 0` — `toList` should be `[(0,0);(1,0)]`, but the mutant returns `[(0,0)]`.
- `prop_DeleteInsert` on `t = (T E 1 0 E)` — deleting then inserting should equal inserting then deleting (modulo the map); the mutant collapses the tree.
- `prop_InsertInsert` on `(E 1 0 0 0)` — the second insert clobbers the first because the mutant always overwrites with a singleton.
- `prop_InsertUnion`, `prop_UnionDeleteInsert` — same root cause, just pushed through `union`.

### `insert_2` — drops the right-subtree branch

```coq
| T l k' v' r =>
  if k <? k' then T (insert k v l) k' v' r
  else T l k' v r
```

The `k' <? k` branch is missing. Every key `≥ k'` ends up replacing the current node's value instead of recursing right — the key on the right-hand side of the tree is effectively thrown away.

- `prop_InsertPost` on `((T E 0 1 E) 1 0 0)`: inserting key=1 into `(T E 0 1 E)` should yield `T E 0 1 (T E 1 0 E)`, but the mutant produces `T E 0 0 E`, silently dropping the right child.
- `prop_InsertDelete` on `((T E 0 2 (T E 1 0 E)) 0 0 0)`: insert then delete of distinct keys; the right subtree vanishes at the insert step.
- `prop_InsertModel` etc. follow from the same erasure.

### `insert_3` — replaces the key's value with the old value

```coq
| T l k' v' r =>
  if k <? k' then T (insert k v l) k' v' r
  else if k' <? k then T l k' v' (insert k v r)
  else T l k' v' r
```

Identical to the original except the equal-key case keeps `v'` (the old value) instead of `v` (the new one). A pure "update" on an existing key is a no-op.

- `prop_InsertPost` on `((T E 3 0 E) 3 3 1)`: inserting key=3 val=1 into a tree that already has key=3 val=0 should make `find 3 = Some 1`; the mutant leaves it at `Some 0`.
- `prop_InsertInsert` on `(E 0 0 1 0)`: two inserts at the same key — the second one should win, but the mutant keeps the first.

## Delete mutations

Original clause:

```coq
Fixpoint delete (k: nat) (t: Tree) :=
  match t with
  | E => E
  | T l k' v' r =>
    if k <? k' then T (delete k l) k' v' r
    else if k' <? k then T l k' v' (delete k r)
    else join l r
  end.
```

### `delete_4` — drops the surrounding node on every recursive branch

```coq
| T l k' v' r =>
  if k <? k' then delete k l
  else if k' <? k then delete k r
  else join l r
```

Both recursive branches return the subtree without the `T l k' v' _` / `T _ k' v' r` wrapper. Deleting from the left subtree throws away the right subtree and the current node entirely, and vice-versa. Every delete-related property fails on very small inputs:

- `prop_DeleteModel` on `((T E 0 0 E) 1)`: deleting key=1 (not present) should leave the tree unchanged, but the mutant recurses into `E` and returns `E`.
- `prop_DeletePost` on `((T E 0 0 E) 1 0)`: `find 0` after deleting 1 should return `Some 0`; the mutant returns `None`.
- `prop_InsertDelete` on `(E 0 1 0)` — same mechanism.

### `delete_5` — swaps the two comparison branches

```coq
| T l k' v' r =>
  if y <? k then T (delete k l) k' v' r
  else if k <? y then T l k' v' (delete k r)
  else join l r
```

(where `y` is the inner name for `k'`). The `<?` tests are flipped so "recurse left" and "recurse right" are swapped. On keys where left-vs-right doesn't matter (e.g. a tree that already has the key at the root), the bug is invisible, which is why `InsertDelete` on `(E 0 1 0)` still passes but every non-trivial case fails — e.g. `DeleteModel` on `((T (T E 1 0 E) 3 0 E) 1)` tries to delete `1`, which the original sends left and the mutant sends right, where it disappears.

## Union mutations

`union` is implemented through fuel-bounded `union_`. Original clause:

```coq
Fixpoint union_ (l: Tree) (r: Tree) (f: nat) :=
  match f with
  | 0 => E
  | S f' =>
    match l, r with
    | E, _ => r
    | _, E => l
    | (T l k v r), t =>
        T (union_ l (below k t) f') k v (union_ r (above k t) f')
    end
  end.
```

`below k t` / `above k t` split `t` around key `k`. The original left-biases on equal keys (picks `v` from the left tree's node).

### `union_6` — ignores the right-subtree structure

```coq
| (T l k v r), (T l' k' v' r') =>
    T l k v (T (union_ r l' f') k' v' r')
```

Instead of splitting the right tree around `k`, the mutant just grafts the right tree wholesale under the left root. Keys on the right tree that should compare against `k` no longer do, so the result is not a BST when those keys straddle `k`.

- `prop_UnionValid` on two singletons `((T E 0 0 E) (T E 0 0 E))` is enough: the second tree gets placed under `key = 0` on the right, duplicating the key and breaking `isBST`.
- `prop_UnionUnionAssoc` on `((T E 0 1 E) (T E 0 1 E) (T E 0 0 E))` — associativity breaks once the shape is non-canonical.

### `union_7` — right-biases on equal keys and mishandles unequal ones

```coq
| (T l k v r), (T l' k' v' r') =>
    if k =? k' then T (union_ l l' f') k v (union_ r r' f')
    else if k <? k' then T l k v (T (union_ r l' f') k' v' r')
    else union_ (T l' k' v' r') (T l k v r) f'
```

Only branches when `k <? k'` is exactly right; the `else` swaps sides and recurses, which picks the right tree's value on the next equality. The tree shape is preserved when keys are non-overlapping, so `UnionValid` does not fail, but any metamorphic property that depends on *which* value wins at a duplicate key (`UnionModel`, `UnionPost`) flips. The counterexample `((T E -1 0 E) (T (T E -2 1 E) 0 0 E))` for `UnionModel` shows the discrepancy: the expected toList differs from the computed one.

### `union_8` — swaps sides on the fallthrough

```coq
| (T l k v r), (T l' k' v' r') =>
    if k =? k'  then T (union_ l l' f') k v (union_ r r' f')
    else if k <? k'   then T (union_ l (below k l') f') k v
                             (union_ r (T (above k l') k' v' r') f')
    else union_ (T l' k' v' r') (T l k v r) f'
```

A subtler variant of `union_7`: the `k <? k'` branch is rewritten with explicit `below`/`above`, but the final `else` still flips arguments, so every `k >? k'` call picks the wrong value on a duplicate key deep in the recursion. `UnionValid` no longer fails (the shape ends up right in these cases) but `UnionPost`, `UnionModel`, `DeleteUnion`, `InsertUnion`, `UnionDeleteInsert`, `UnionUnionAssoc` all still fail on keyed overlap.

## Reading the counterexamples

Each workload's `etna.toml` stores every task as `{ property = ..., witnesses = [{ input = "..." }] }` in S-expression form. Conventions:

- `E` — empty tree.
- `(T l k v r)` — tree node.
- Extra numbers at the end are operation arguments in the order declared by the property (see `Spec.v`). For example, `prop_InsertPost` has signature `(t, k, k', v)`, so `((T E 2 1 E) 0 2 0)` reads as "tree, then k=0, k'=2, v=0".

A strategy "solves" a task when it produces any valid-BST input on which the property returns `false` against the mutant; the witnesses in `etna.toml` are reference minimums, not required outputs.
