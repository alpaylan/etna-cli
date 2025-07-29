From QuickChick Require Import QuickChick.

From PropLang Require Import PropLang.
Set Warnings "-extraction-opaque-accessed,-extraction".

From STLC Require Import ProplangTypeBasedFuzzer.

Axiom num_tests : nat. 
Extract Constant num_tests => "max_int".

Definition qctest_test_prop_SinglePreserve := (fun _ : unit => print_extracted_coq_string ("[|{" ++ show (withTime(fun tt => (invoke test_prop_SinglePreserve))) ++ "}|]")).
Definition qctest_test_prop_MultiPreserve := (fun _ : unit => print_extracted_coq_string ("[|{" ++ show (withTime(fun tt => (invoke test_prop_MultiPreserve))) ++ "}|]")).

Parameter OCamlString : Type.
Extract Constant OCamlString => "string".

Axiom qctest_map : OCamlString -> unit.
Extract Constant qctest_map => "
fun test_name ->
  let test_map = [
    (""SinglePreserve"", qctest_test_prop_SinglePreserve);
    (""MultiPreserve"", qctest_test_prop_MultiPreserve)
  ] in
  let test = List.assoc test_name test_map in
  test ()


let () =
  Printf.printf ""Entering main of qc_exec\\n""; flush stdout;
  setup_shm_aux ();
  Sys.argv.(1) |> qctest_map ; flush stdout;
".


Extraction "ProplangTypeBasedFuzzer_test_runner.ml" sample1 runLoop qctest_test_prop_SinglePreserve qctest_test_prop_MultiPreserve  qctest_map.
