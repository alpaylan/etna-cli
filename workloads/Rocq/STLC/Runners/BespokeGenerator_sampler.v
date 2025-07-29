From QuickChick Require Import QuickChick.

Set Warnings "-extraction-opaque-accessed,-extraction".

From STLC Require Import Impl Spec BespokeGeneration.

Local Open Scope string_scope.

#[local] Instance showTimedResult {A: Type} `{Show A} : Show (@TimedResult A) := {|
  show result := 
    let '(TResult result time start ending) := result in
     "{ ""time"": """ ++ show time ++ """, ""value"": """ ++ show result ++ """ }"
|}.


Definition showTimedResultList {A: Type} `{Show A} (results : list (@TimedResult A)) : string :=
  "[" ++ String.concat ", " (List.map (fun r => show r) results) ++ "]".

Inductive Args1 {A : Type} : Type :=
  | Args1Mk : A -> Args1.

#[local] Instance ShowArgs1 {A} `{Show A} : Show (@Args1 A) :=
  { show := fun '(Args1Mk a) => show a }.

#[local] Instance ShowExprOpt : Show (option Expr) :=
  { show := fun e =>
    match e with
    | Some e' => show e'
    | None => ""
    end
  }.

Definition sample_SinglePreserve    :=
  bindGen gSized (fun (e: option Expr)  =>
  ret (Args1Mk e))
.

Definition qctest_sample_SinglePreserve := (fun num_tests: nat => print_extracted_coq_string (showTimedResultList (quickSample (updMaxDiscard (updMaxSuccess (updAnalysis stdArgs true) num_tests) num_tests) sample_SinglePreserve))).


Definition sample_MultiPreserve    :=
    bindGen gSized (fun (e: option Expr)  =>
    ret (Args1Mk e))
.

Definition qctest_sample_MultiPreserve := (fun num_tests: nat => print_extracted_coq_string (showTimedResultList (quickSample (updMaxDiscard (updMaxSuccess (updAnalysis stdArgs true) num_tests) num_tests) sample_MultiPreserve))).


Parameter OCamlString : Type.
Extract Constant OCamlString => "string".

Axiom test_map : list (OCamlString * (nat -> unit)).
Extract Constant test_map => "[
    (""SinglePreserve"", qctest_sample_SinglePreserve);
    (""MultiPreserve"", qctest_sample_MultiPreserve);
]".

Axiom qctest_map : OCamlString -> nat -> unit.
Extract Constant qctest_map => "
fun property num_tests ->
  let test = List.assoc property test_map in
  test num_tests


let () =
  let args = Sys.argv in
  if Array.length args <> 3 then
    Printf.eprintf ""Usage: %s <property> <#tests>\n"" args.(0)
  else
    let property = args.(1) in
    let num_tests = int_of_string args.(2) in
    if not (List.mem_assoc property test_map) then
      Printf.eprintf ""Unknown test name: %s\n"" property
    else
      qctest_map property num_tests
".


Extraction "BespokeGenerator_sampler.ml" test_map qctest_map qctest_sample_SinglePreserve qctest_sample_MultiPreserve.
