From QuickChick Require Import QuickChick.

Set Warnings "-extraction-opaque-accessed,-extraction".

From STLC Require Import TypeBasedFuzzer.

Axiom num_tests : nat. 
Extract Constant num_tests => "max_int".

Definition qctest_test_prop_SinglePreserve := (fun _ : unit => print_extracted_coq_string ("[|{" ++ show (withTime (fun tt => (test_prop_SinglePreserve_fuzzer tt))) ++ "}|]")).
Definition qctest_test_prop_MultiPreserve := (fun _ : unit => print_extracted_coq_string ("[|{" ++ show (withTime (fun tt => (test_prop_MultiPreserve_fuzzer tt))) ++ "}|]")).

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


Extraction "TypeBasedFuzzer_test_runner.ml" qctest_test_prop_SinglePreserve qctest_test_prop_MultiPreserve  qctest_map.
