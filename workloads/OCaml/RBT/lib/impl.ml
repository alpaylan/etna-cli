open Option

let ( >>= ) = bind
let ( <$> ) f x = match x with None -> None | Some v -> Some (f v)
let return x = Some x

type color = R | B [@@deriving sexp, quickcheck]

type ('a, 'b) tree = E | T of color * ('a, 'b) tree * 'a * 'b * ('a, 'b) tree
[@@deriving sexp, quickcheck]

type key = int
type value = int
type rbt = (Nat.Nat.t, Nat.Nat.t) tree [@@deriving sexp, quickcheck]

let t c l k v r = T (c, l, k, v, r)

let blacken (t : rbt) : rbt =
  match t with E -> E | T (_, l, k, v, r) -> T (B, l, k, v, r)

let redden (t : rbt) : rbt option =
  match t with T (B, l, k, v, r) -> Some (T (R, l, k, v, r)) | _ -> None

let balance (col : color) (tl : rbt) (k : key) (v : value) (tr : rbt) : rbt =
  match (col, tl, k, v, tr) with
  | B, T (R, T (R, a, x, vx, b), y, vy, c), z, vz, d ->
      (*! *)
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
      (*!! swap_cd *)
      (*!
        T (R, T (B, a, x, vx, b), y, vy, T (B, d, z, vz, c))
      *)
      (* !*)
  | B, T (R, a, x, vx, T (R, b, y, vy, c)), z, vz, d ->
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
  | B, a, x, vx, T (R, T (R, b, y, vy, c), z, vz, d) ->
      (*! *)
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
      (*!! swap_bc *)
      (*!
      T (R, T (B, a, x, vx, c), y, vy, T (B, b, z, vz, d))
      *)
      (* !*)
  | B, a, x, vx, T (R, b, y, vy, T (R, c, z, vz, d)) ->
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
  | rb, a, x, vx, b -> T (rb, a, x, vx, b)

let rec insert (k : key) (v : value) (t : rbt) : rbt =
  let _ = ignore insert in
  let rec ins x vx t : rbt =
    match (x, vx, t) with
    | x, vx, E ->
        (*! *)
(*!
        T (R, E, x, vx, E)
*)
        (*!! miscolor_insert *)
          T (B, E, x, vx, E)
        (* !*)
    | x, vx, T (rb, a, y, vy, b) ->
        let _ = ignore (rb, a, y, vy, b, ins) in
        (*! *)
        if x < y then balance rb (ins x vx a) y vy b
        else if y < x then balance rb a y vy (ins x vx b)
        else T (rb, a, y, vx, b)
        (*!! insert_1 *)
        (*!
          T (R, E, x, vx, E)
        *)
        (*!! insert_2 *)
        (*!
          if x < y then balance rb (ins x vx a) y vy b
          else T (rb, a, y, vx, b)
        *)
        (*!! insert_3 *)
        (*!
          if x < y then balance rb (ins x vx a) y vy b
          else if y < x then balance rb a y vy (ins x vx b)
          else T (rb, a, y, vy, b)
        *)
        (*!! no_balance_insert_1 *)
        (*!
          if x < y then T (rb, ins x vx a, y, vy, b)
          else if y < x then balance rb a y vy (ins x vx b)
          else T (rb, a, y, vx, b)
        *)
        (*!! no_balance_insert_2 *)
        (*!
          if x < y then balance rb (ins x vx a) y vy b
          else if y < x then T (rb, a, y, vy, ins x vx b)
          else T (rb, a, y, vx, b)
        *)
        (* !*)
  in

  blacken (ins k v t)

let balLeft (tl : rbt) (k : key) (v : value) (tr : rbt) : rbt option =
  match (tl, k, v, tr) with
  | T (R, a, x, vx, b), y, vy, c -> return (T (R, T (B, a, x, vx, b), y, vy, c))
  | bl, x, vx, T (B, a, y, vy, b) ->
      return (balance B bl x vx (T (R, a, y, vy, b)))
  | bl, x, vx, T (R, T (B, a, y, vy, b), z, vz, c) ->
      (*! *)
      redden c >>= fun c' ->
      return (T (R, T (B, bl, x, vx, a), y, vy, balance B b z vz c'))
      (*!! miscolor_balLeft *)
      (*!
        return (T (R, T (B, bl, x, vx, a), y, vy, (balance B b z vz c)))
      *)
      (* !*)
  | _, _, _, _ -> None

let balRight (tl : rbt) (k : key) (v : value) (tr : rbt) : rbt option =
  match (tl, k, v, tr) with
  | a, x, vx, T (R, b, y, vy, c) -> return (T (R, a, x, vx, T (B, b, y, vy, c)))
  | T (B, a, x, vx, b), y, vy, bl ->
      return (balance B (T (R, a, x, vx, b)) y vy bl)
  | T (R, a, x, vx, T (B, b, y, vy, c)), z, vz, bl ->
      (*! *)
      redden a >>= fun a' ->
      return (T (R, balance B a' x vx b, y, vy, T (B, c, z, vz, bl)))
      (*!! miscolor_balRight *)
      (*!
          return (T (R, (balance B a x vx b), y, vy, T (B, c, z, vz, bl)))
      *)
      (* !*)
  | _, _, _, _ -> None

let rec join (t1 : rbt) (t2 : rbt) : rbt option =
  match (t1, t2) with
  | E, a -> return a
  | a, E -> return a
  | T (R, a, x, vx, b), T (R, c, y, vy, d) -> (
      join b c >>= fun t' ->
      match t' with
      | T (R, b', z, vz, c') ->
          (*! *)
          return (T (R, T (R, a, x, vx, b'), z, vz, T (R, c', y, vy, d)))
          (*!! miscolor_join_1 *)
          (*!
            return (T (R, T (B, a, x, vx, b'), z, vz, T (B, c', y, vy, d)))
          *)
          (* !*)
      | bc -> return (T (R, a, x, vx, T (R, bc, y, vy, d))))
  | T (B, a, x, vx, b), T (B, c, y, vy, d) -> (
      join b c >>= fun t' ->
      match t' with
      | T (R, b', z, vz, c') ->
          (*! *)
          return (T (R, T (B, a, x, vx, b'), z, vz, T (B, c', y, vy, d)))
          (*!! miscolor_join_2 *)
          (*!
            return (T (R, T (R, a, x, vx, b'), z, vz, T (R, c', y, vy, d)))
          *)
          (* !*)
      | bc -> balLeft a x vx (T (B, bc, y, vy, d)))
  | a, T (R, b, x, vx, c) -> join a b >>= fun t' -> return (T (R, t', x, vx, c))
  | T (R, a, x, vx, b), c -> t R a x vx <$> join b c

let delete x tr =
  let rec del _t =
    let delLeft a y vy b =
      match a with
      | T (B, _, _, _, _) -> del a >>= fun a' -> balLeft a' y vy b
      | _ -> del a >>= fun a' -> return (T (R, a', y, vy, b))
    in
    let delRight a y vy b =
      match b with
      | T (B, _, _, _, _) -> del b >>= balRight a y vy
      | _ -> t R a y vy <$> del b
    in
    match _t with
    | E -> return E
    | T (_, a, y, vy, b) ->
        let _ = ignore (vy, delLeft, delRight) in
        (*! *)
        if x < y then delLeft a y vy b
        else if x > y then delRight a y vy b
        else join a b
        (*!! delete_4 *)
        (*!
          if x < y then del a
          else if x > y then del b
          else join a b
        *)
        (*!! delete_5 *)
        (*!
          if x > y then delLeft a y vy b
          else if x < y then delRight a y vy b
          else join a b
        *)
        (* !*)
  in
  (*! *)
  blacken <$> del tr
  (*!! miscolor_delete *)
  (*!
    del tr
  *)
  (* !*)

let rec find (x : key) (t : rbt) : value option =
  match t with
  | E -> None
  | T (_, l, y, vy, r) ->
      if x < y then find x l else if y < x then find x r else Some vy

let rec size (t : rbt) : int =
  match t with E -> 0 | T (_, l, _, _, r) -> 1 + size l + size r

let color_to_string (c : color) : string =
  match c with
  | R -> "(R)"
  | B -> "(B)"

let to_string t =
  let rec aux t =
    match t with
    | E -> "(E)"
    | T (c, l, k, v, r) -> Printf.sprintf "(T %s %s %d %d %s)" (color_to_string c) (aux l) k v (aux r)
  in
  aux t

