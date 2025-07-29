use crate::implementation::{Expr, Typ};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = vec![];
    let mut current = String::new();
    for ch in input.chars() {
        match ch {
            '(' | ')' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                tokens.push(ch.to_string());
                current.clear();
            }
            ' ' | '\n' | '\t' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn parse_sexp(tokens: &[String]) -> Result<(Sexp, &[String]), String> {
    if tokens.is_empty() {
        return Err("Empty token stream".to_string());
    }

    match &tokens[0][..] {
        "(" => {
            let mut rest = &tokens[1..];
            let mut list = vec![];
            while !rest.is_empty() && rest[0] != ")" {
                let (elem, new_rest) = parse_sexp(rest)?;
                list.push(elem);
                rest = new_rest;
            }
            if rest.is_empty() || rest[0] != ")" {
                return Err("Unmatched (".to_string());
            }
            Ok((Sexp::List(list), &rest[1..]))
        }
        ")" => Err("Unmatched )".to_string()),
        atom => Ok((Sexp::Atom(atom.to_string()), &tokens[1..])),
    }
}

fn sexp_to_typ(sexp: &Sexp) -> Result<Typ, String> {
    match sexp {
        Sexp::Atom(s) if s == "TBool" => Ok(Typ::TBool),
        Sexp::List(atom) if atom.len() == 1 => sexp_to_typ(&atom[0]),
        Sexp::List(items) => {
            assert!(
                items.len() == 3,
                "Expected 3 items for Typ, got {:?}",
                items
            );
            if items.len() == 3 {
                if let Sexp::Atom(tag) = &items[0] {
                    if tag == "TFun" {
                        let t1 = sexp_to_typ(&items[1])?;
                        let t2 = sexp_to_typ(&items[2])?;
                        return Ok(Typ::TFun(Box::new(t1), Box::new(t2)));
                    }
                }
            }
            Err("Invalid TFun syntax".to_string())
        }
        _ => Err("Invalid Typ expression".to_string()),
    }
}

fn sexp_to_expr(sexp: &Sexp) -> Result<Expr, String> {
    match sexp {
        Sexp::List(items) if !items.is_empty() => match &items[0] {
            Sexp::Atom(tag) => match tag.as_str() {
                "Var" => {
                    if items.len() != 2 {
                        return Err("Var expects 1 argument".to_string());
                    }
                    if let Sexp::Atom(n) = &items[1] {
                        let v = n.parse::<i32>().map_err(|_| "Invalid int")?;
                        Ok(Expr::Var(v))
                    } else {
                        Err("Var expects atom".to_string())
                    }
                }
                "Bool" => {
                    if items.len() != 2 {
                        return Err("Bool expects 1 argument".to_string());
                    }
                    if let Sexp::Atom(b) = &items[1] {
                        match b.as_str() {
                            "#t" => Ok(Expr::Bool(true)),
                            "#f" => Ok(Expr::Bool(false)),
                            _ => Err("Invalid bool".to_string()),
                        }
                    } else {
                        Err("Bool expects atom".to_string())
                    }
                }
                "Abs" => {
                    if items.len() != 3 {
                        return Err("Abs expects 2 arguments".to_string());
                    }
                    let typ = sexp_to_typ(&items[1])?;
                    let body = sexp_to_expr(&items[2])?;
                    Ok(Expr::Abs(typ, Box::new(body)))
                }
                "App" => {
                    if items.len() != 3 {
                        return Err("App expects 2 arguments".to_string());
                    }
                    let f = sexp_to_expr(&items[1])?;
                    let x = sexp_to_expr(&items[2])?;
                    Ok(Expr::App(Box::new(f), Box::new(x)))
                }
                _ => Err(format!("Unknown constructor: {}", tag)),
            },
            _ => Err("Expected atom as tag".to_string()),
        },
        _ => Err("Expected list".to_string()),
    }
}

pub fn parse_expr(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input);
    let (sexp, rest) = parse_sexp(&tokens)?;
    if !rest.is_empty() {
        return Err("Trailing tokens".to_string());
    }
    sexp_to_expr(&sexp)
}

pub fn parse(input: &str) -> Result<Vec<Expr>, String> {
    let tokens = tokenize(input);
    assert!(tokens.len() >= 2 && tokens[0] == "(" && tokens[tokens.len() - 1] == ")");
    let mut rest = &tokens[1..&tokens.len() - 1]; // Skip the outermost parentheses
    let mut exprs = vec![];

    while !rest.is_empty() {
        let (sexp, new_rest) = parse_sexp(rest)?;
        rest = new_rest;
        let expr = sexp_to_expr(&sexp)?;
        exprs.push(expr);
    }

    Ok(exprs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::implementation::Expr;
    use crate::implementation::Typ;

    #[test]
    fn test_tokenize() {
        let input = "(Var 42) (Bool #t) (Abs TBool (Var 1)) (App (Var 1) (Bool #f))";
        let tokens = tokenize(input);
        assert_eq!(
            tokens,
            vec![
                "(", "Var", "42", ")", "(", "Bool", "#t", ")", "(", "Abs", "TBool", "(", "Var",
                "1", ")", ")", "(", "App", "(", "Var", "1", ")", "(", "Bool", "#f", ")", ")"
            ]
        );
    }

    #[test]
    fn test_parse_sexp() {
        let tokens = tokenize("((Var 42) (Bool #t) (Abs TBool (Var 1)))");
        let (sexp, rest) = parse_sexp(&tokens).expect("Failed to parse S-expression");
        assert!(rest.is_empty());
        assert_eq!(
            sexp,
            Sexp::List(vec![
                Sexp::List(vec![
                    Sexp::Atom("Var".to_string()),
                    Sexp::Atom("42".to_string())
                ]),
                Sexp::List(vec![
                    Sexp::Atom("Bool".to_string()),
                    Sexp::Atom("#t".to_string())
                ]),
                Sexp::List(vec![
                    Sexp::Atom("Abs".to_string()),
                    Sexp::Atom("TBool".to_string()),
                    Sexp::List(vec![
                        Sexp::Atom("Var".to_string()),
                        Sexp::Atom("1".to_string())
                    ])
                ])
            ])
        );
    }

    #[test]
    fn test_sexp_to_typ() {
        let sexp = Sexp::List(vec![
            Sexp::Atom("TFun".to_string()),
            Sexp::Atom("TBool".to_string()),
            Sexp::Atom("TBool".to_string()),
        ]);
        let typ = sexp_to_typ(&sexp).expect("Failed to convert S-expression to Typ");
        assert_eq!(typ, Typ::TFun(Box::new(Typ::TBool), Box::new(Typ::TBool)));
    }

    #[test]
    fn test_sexp_to_expr() {
        let sexp = Sexp::List(vec![
            Sexp::Atom("App".to_string()),
            Sexp::List(vec![
                Sexp::Atom("Var".to_string()),
                Sexp::Atom("1".to_string()),
            ]),
            Sexp::List(vec![
                Sexp::Atom("Bool".to_string()),
                Sexp::Atom("#t".to_string()),
            ]),
        ]);
        let expr = sexp_to_expr(&sexp).expect("Failed to convert S-expression to Expr");
        assert_eq!(
            expr,
            Expr::App(Box::new(Expr::Var(1)), Box::new(Expr::Bool(true)))
        );
    }

    #[test]
    fn test_parse_expr() {
        let input = "(App (Var 1) (Bool #t))";
        let expr = parse_expr(input).expect("Failed to parse expression");
        assert_eq!(
            expr,
            Expr::App(Box::new(Expr::Var(1)), Box::new(Expr::Bool(true)))
        );
    }

    #[test]
    fn test_parse() {
        let input = "((Var 1) (Bool #t) (Abs TBool (Var 2)))";
        let exprs = parse(input).expect("Failed to parse expressions");
        assert_eq!(
            exprs,
            vec![
                Expr::Var(1),
                Expr::Bool(true),
                Expr::Abs(Typ::TBool, Box::new(Expr::Var(2)))
            ]
        );
    }
}
