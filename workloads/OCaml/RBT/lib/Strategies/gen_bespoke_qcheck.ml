open QCheck2
open Impl

let blacken_correct (t : ('a, 'b) tree) : ('a, 'b) tree =
  match t with E -> E | T (_, a, k, v, b) -> T (B, a, k, v, b)

let balance_correct (col : color) (tl : ('a, 'b) tree) (k : 'a) (v : 'b)
    (tr : ('a, 'b) tree) : ('a, 'b) tree =
  match (col, tl, k, v, tr) with
  | B, T (R, T (R, a, x, vx, b), y, vy, c), z, vz, d ->
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
  | B, T (R, a, x, vx, T (R, b, y, vy, c)), z, vz, d ->
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
  | B, a, x, vx, T (R, T (R, b, y, vy, c), z, vz, d) ->
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
  | B, a, x, vx, T (R, b, y, vy, T (R, c, z, vz, d)) ->
      T (R, T (B, a, x, vx, b), y, vy, T (B, c, z, vz, d))
  | rb, a, x, vx, b -> T (rb, a, x, vx, b)

let insert_correct (k : 'a) (vk : 'b) (s : ('a, 'b) tree) : ('a, 'b) tree =
  let rec ins x vx t =
    match t with
    | E -> T (R, E, x, vx, E)
    | T (rb, a, y, vy, b) ->
        if x < y then balance_correct rb (ins x vx a) y vy b
        else if x > y then balance_correct rb a y vy (ins x vx b)
        else T (rb, a, y, vx, b)
  in
  blacken_correct (ins k vk s)

let gen_Q_Bespoke =
  let open Gen in
  let* xs =
    list_size (int_bound 20) (pair (int_bound 100) (int_bound 100))
  in
  let xs = List.sort_uniq (fun (k1, _) (k2, _) -> (compare k1 k2)) xs in
  let tree = List.fold_left (fun acc (k, v) -> insert_correct k v acc) E xs in
  return tree




(* let gen_Q_Bespoke = QCheck.make bespoke ~print:Display.string_of_tree *)