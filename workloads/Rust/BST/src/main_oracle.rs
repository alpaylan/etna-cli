use bst::{implementation::Tree, spec};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const KNOWN_MUTATIONS: &[&str] = &[
    "insert_1", "insert_2", "insert_3", "delete_4", "delete_5", "union_6", "union_7", "union_8",
];

enum Case {
    InsertValid((Tree, i32, i32)),
    DeleteValid((Tree, i32)),
    UnionValid((Tree, Tree)),
    InsertPost((Tree, i32, i32, i32)),
    DeletePost((Tree, i32, i32)),
    UnionPost((Tree, Tree, i32)),
    InsertModel((Tree, i32, i32)),
    DeleteModel((Tree, i32)),
    UnionModel((Tree, Tree)),
    InsertInsert((Tree, i32, i32, i32, i32)),
    InsertDelete((Tree, i32, i32, i32)),
    InsertUnion((Tree, Tree, i32, i32)),
    DeleteInsert((Tree, i32, i32, i32)),
    DeleteDelete((Tree, i32, i32)),
    DeleteUnion((Tree, Tree, i32)),
    UnionDeleteInsert((Tree, Tree, i32, i32)),
    UnionUnionIdempotent((Tree,)),
    UnionUnionAssoc((Tree, Tree, Tree)),
}

impl Case {
    fn eval(&self) -> Option<bool> {
        match self {
            Case::InsertValid((t, k, v)) => spec::prop_insert_valid(t.clone(), *k, *v),
            Case::DeleteValid((t, k)) => spec::prop_delete_valid(t.clone(), *k),
            Case::UnionValid((t1, t2)) => spec::prop_union_valid(t1.clone(), t2.clone()),
            Case::InsertPost((t, k, k2, v)) => spec::prop_insert_post(t.clone(), *k, *k2, *v),
            Case::DeletePost((t, k, k2)) => spec::prop_delete_post(t.clone(), *k, *k2),
            Case::UnionPost((t1, t2, k)) => spec::prop_union_post(t1.clone(), t2.clone(), *k),
            Case::InsertModel((t, k, v)) => spec::prop_insert_model(t.clone(), *k, *v),
            Case::DeleteModel((t, k)) => spec::prop_delete_model(t.clone(), *k),
            Case::UnionModel((t1, t2)) => spec::prop_union_model(t1.clone(), t2.clone()),
            Case::InsertInsert((t, k, k2, v, v2)) => {
                spec::prop_insert_insert(t.clone(), *k, *k2, *v, *v2)
            }
            Case::InsertDelete((t, k, k2, v)) => spec::prop_insert_delete(t.clone(), *k, *k2, *v),
            Case::InsertUnion((t, t2, k, v)) => spec::prop_insert_union(t.clone(), t2.clone(), *k, *v),
            Case::DeleteInsert((t, k, k2, v)) => spec::prop_delete_insert(t.clone(), *k, *k2, *v),
            Case::DeleteDelete((t, k, k2)) => spec::prop_delete_delete(t.clone(), *k, *k2),
            Case::DeleteUnion((t1, t2, k)) => spec::prop_delete_union(t1.clone(), t2.clone(), *k),
            Case::UnionDeleteInsert((t1, t2, k, v)) => {
                spec::prop_union_delete_insert(t1.clone(), t2.clone(), *k, *v)
            }
            Case::UnionUnionIdempotent((t,)) => spec::prop_union_union_idempotent(t.clone()),
            Case::UnionUnionAssoc((t1, t2, t3)) => {
                spec::prop_union_union_assoc(t1.clone(), t2.clone(), t3.clone())
            }
        }
    }
}

fn parse_case(property: &str, value: &str) -> Result<Case, String> {
    match property {
        "InsertValid" => serde_lexpr::from_str::<(Tree, i32, i32)>(value)
            .map(Case::InsertValid)
            .map_err(|e| e.to_string()),
        "DeleteValid" => serde_lexpr::from_str::<(Tree, i32)>(value)
            .map(Case::DeleteValid)
            .map_err(|e| e.to_string()),
        "UnionValid" => serde_lexpr::from_str::<(Tree, Tree)>(value)
            .map(Case::UnionValid)
            .map_err(|e| e.to_string()),
        "InsertPost" => serde_lexpr::from_str::<(Tree, i32, i32, i32)>(value)
            .map(Case::InsertPost)
            .map_err(|e| e.to_string()),
        "DeletePost" => serde_lexpr::from_str::<(Tree, i32, i32)>(value)
            .map(Case::DeletePost)
            .map_err(|e| e.to_string()),
        "UnionPost" => serde_lexpr::from_str::<(Tree, Tree, i32)>(value)
            .map(Case::UnionPost)
            .map_err(|e| e.to_string()),
        "InsertModel" => serde_lexpr::from_str::<(Tree, i32, i32)>(value)
            .map(Case::InsertModel)
            .map_err(|e| e.to_string()),
        "DeleteModel" => serde_lexpr::from_str::<(Tree, i32)>(value)
            .map(Case::DeleteModel)
            .map_err(|e| e.to_string()),
        "UnionModel" => serde_lexpr::from_str::<(Tree, Tree)>(value)
            .map(Case::UnionModel)
            .map_err(|e| e.to_string()),
        "InsertInsert" => serde_lexpr::from_str::<(Tree, i32, i32, i32, i32)>(value)
            .map(Case::InsertInsert)
            .map_err(|e| e.to_string()),
        "InsertDelete" => serde_lexpr::from_str::<(Tree, i32, i32, i32)>(value)
            .map(Case::InsertDelete)
            .map_err(|e| e.to_string()),
        "InsertUnion" => serde_lexpr::from_str::<(Tree, Tree, i32, i32)>(value)
            .map(Case::InsertUnion)
            .map_err(|e| e.to_string()),
        "DeleteInsert" => serde_lexpr::from_str::<(Tree, i32, i32, i32)>(value)
            .map(Case::DeleteInsert)
            .map_err(|e| e.to_string()),
        "DeleteDelete" => serde_lexpr::from_str::<(Tree, i32, i32)>(value)
            .map(Case::DeleteDelete)
            .map_err(|e| e.to_string()),
        "DeleteUnion" => serde_lexpr::from_str::<(Tree, Tree, i32)>(value)
            .map(Case::DeleteUnion)
            .map_err(|e| e.to_string()),
        "UnionDeleteInsert" => serde_lexpr::from_str::<(Tree, Tree, i32, i32)>(value)
            .map(Case::UnionDeleteInsert)
            .map_err(|e| e.to_string()),
        "UnionUnionIdempotent" => serde_lexpr::from_str::<(Tree,)>(value)
            .map(Case::UnionUnionIdempotent)
            .map_err(|e| e.to_string()),
        "UnionUnionAssoc" => serde_lexpr::from_str::<(Tree, Tree, Tree)>(value)
            .map(Case::UnionUnionAssoc)
            .map_err(|e| e.to_string()),
        _ => Err(format!("Unknown property: {}", property)),
    }
}

fn mutation_env_key(mutation: &str) -> String {
    format!("M_{}", mutation)
}

fn clear_known_mutation_env() {
    for mutation in KNOWN_MUTATIONS {
        let key = mutation_env_key(mutation);
        // SAFETY: This process is single-threaded and controls all env writes/reads.
        unsafe { std::env::remove_var(key) };
    }
}

fn activate_mutation(mutation: &str) {
    let key = mutation_env_key(mutation);
    // SAFETY: This process is single-threaded and controls all env writes/reads.
    unsafe { std::env::set_var(key, "active") };
}

fn outcome_json(outcome: Option<bool>) -> Value {
    match outcome {
        Some(true) => json!({"status": "passed", "failed": false}),
        Some(false) => json!({"status": "failed", "failed": true}),
        None => json!({"status": "discarded", "failed": false}),
    }
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("Usage: {} <property> <value> [mutation...]", args[0]);
        std::process::exit(2);
    }

    let property = args[1].as_str();
    let value = args[2].as_str();
    let mutations: Vec<String> = if args.len() > 3 {
        args[3..].to_vec()
    } else {
        KNOWN_MUTATIONS.iter().map(|m| (*m).to_string()).collect()
    };

    let known: BTreeSet<String> = KNOWN_MUTATIONS.iter().map(|m| (*m).to_string()).collect();
    if let Some(unknown) = mutations.iter().find(|m| !known.contains(*m)) {
        let out = json!({
            "ok": false,
            "error": format!("Unknown mutation: {}", unknown),
        });
        println!("{}", serde_json::to_string(&out).expect("serialize failure"));
        std::process::exit(1);
    }

    let case = match parse_case(property, value) {
        Ok(case) => case,
        Err(err) => {
            let out = json!({
                "ok": false,
                "error": err,
            });
            println!("{}", serde_json::to_string(&out).expect("serialize failure"));
            std::process::exit(1);
        }
    };

    clear_known_mutation_env();
    let mut results = BTreeMap::<String, Value>::new();

    for mutation in &mutations {
        clear_known_mutation_env();
        activate_mutation(mutation);
        let outcome = case.eval();
        results.insert(mutation.clone(), outcome_json(outcome));
    }
    clear_known_mutation_env();

    let out = json!({
        "ok": true,
        "property": property,
        "value": value,
        "results": results,
    });
    println!("{}", serde_json::to_string(&out).expect("serialize failure"));
}
