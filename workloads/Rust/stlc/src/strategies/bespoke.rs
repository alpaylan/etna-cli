use std::fmt::Display;

use quickcheck::{Arbitrary, Gen};

use crate::implementation::{Ctx, Expr, Typ};

impl Arbitrary for Expr {
    fn arbitrary(g: &mut Gen) -> Self {
        let typ = Typ::arbitrary(g);
        gen_exact_expr(vec![typ.clone()], typ, g, g.size())
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(std::iter::empty()) // Shrinking omitted
    }
}

fn gen_exact_expr(ctx: Ctx, t: Typ, g: &mut Gen, size: usize) -> Expr {
    if size == 0 {
        let mut gens: Vec<Box<dyn Fn(&mut Gen) -> Expr>> =
            vec![Box::new(|g| gen_one(&ctx, &t.clone(), g))];

        if let Some(var_gen) = gen_var(&ctx, &t, g) {
            gens.push(Box::new(move |_| var_gen.clone()));
        }
        g.choose(&gens).unwrap()(g)
    } else {
        let mut gens: Vec<Box<dyn Fn(&mut Gen) -> Expr>> = vec![
            Box::new(|g| gen_one(&ctx, &t, g)),
            Box::new(|g| gen_app(&ctx, &t, g, size)),
        ];
        if let Typ::TFun(box t1, box t2) = &t {
            gens.push(Box::new(|g| {
                gen_abs(&ctx, t1.clone(), t2.clone(), g, size - 1)
            }));
        }
        if let Some(var_gen) = gen_var(&ctx, &t, g) {
            gens.push(Box::new(move |_| var_gen.clone()));
        }
        g.choose(&gens).unwrap()(g)
    }
}

fn gen_one(ctx: &Ctx, t: &Typ, g: &mut Gen) -> Expr {
    match t {
        Typ::TBool => Expr::Bool(bool::arbitrary(g)),
        Typ::TFun(t1, t2) => {
            let mut ctx1 = ctx.clone();
            ctx1.insert(0, *t1.clone());
            let e = gen_one(&ctx1, t2, g);
            Expr::Abs(*t1.clone(), Box::new(e))
        }
    }
}

fn gen_abs(ctx: &Ctx, t1: Typ, t2: Typ, g: &mut Gen, size: usize) -> Expr {
    let mut ctx1 = ctx.clone();
    ctx1.insert(0, t1.clone());
    let e = gen_exact_expr(ctx1, t2.clone(), g, size);
    Expr::Abs(t1, Box::new(e))
}

fn gen_app(ctx: &Ctx, t: &Typ, g: &mut Gen, size: usize) -> Expr {
    let t_prime = Typ::arbitrary(g);
    let e1 = gen_exact_expr(
        ctx.clone(),
        Typ::TFun(Box::new(t_prime.clone()), Box::new(t.clone())),
        g,
        size / 2,
    );
    let e2 = gen_exact_expr(ctx.clone(), t_prime.clone(), g, size / 2);
    Expr::App(Box::new(e1), Box::new(e2))
}

fn gen_var(ctx: &Ctx, t: &Typ, g: &mut Gen) -> Option<Expr> {
    let candidates: Vec<usize> = ctx
        .iter()
        .enumerate()
        .filter_map(|(i, t2)| if t2 == t { Some(i) } else { None })
        .collect();

    g.choose(&candidates).map(|&i| Expr::Var(i as i32))
}

fn gen_typ(g: &mut Gen, size: usize) -> Typ {
    if size == 0 {
        Typ::TBool
    } else {
        g.frequency(&[
            (1, Box::new(|_| Typ::TBool)),
            (
                size,
                Box::new(move |g| {
                    Typ::TFun(
                        Box::new(gen_typ(g, size / 2)),
                        Box::new(gen_typ(g, size / 2)),
                    )
                }),
            ),
        ])
    }
}

impl Arbitrary for Typ {
    fn arbitrary(g: &mut Gen) -> Self {
        let size = g.size();
        gen_typ(g, size)
    }
}

#[derive(Debug, Clone)]
pub struct ExprOpt(pub Option<Expr>);

impl Display for ExprOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(expr) => write!(f, "{}", expr),
            None => write!(f, "None"),
        }
    }
}

impl Arbitrary for ExprOpt {
    fn arbitrary(g: &mut Gen) -> Self {
        let typ = Typ::arbitrary(g);
        let expr = gen_exact_expr(vec![], typ, g, g.size());
        ExprOpt(Some(expr))
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::implementation::Expr;
//     use quickcheck::quickcheck;

//     #[test]
//     fn test_gen_zero1() {
//         let t = Typ::TBool;
//         let ctx: Ctx = vec![];
//         let mut g = quickcheck::Gen::new(0);
//         let expr = gen_zero(ctx, t, &mut g);
//         assert!(expr == Some(Expr::Bool(true)) || expr == Some(Expr::Bool(false)));
//     }

//     #[test]
//     fn test_gen_zero2() {
//         let t = Typ::TFun(Box::new(Typ::TBool), Box::new(Typ::TBool));
//         let ctx: Ctx = vec![];
//         let mut g = quickcheck::Gen::new(1);
//         let expr = gen_zero(ctx, t, &mut g);
//         assert!(
//             expr == Some(Expr::Abs(Typ::TBool, Box::new(Expr::Bool(true))))
//                 || expr == Some(Expr::Abs(Typ::TBool, Box::new(Expr::Bool(false))))
//         );
//     }

//     #[test]
//     fn test_gen_expr1() {
//         let t = Typ::TFun(Box::new(Typ::TBool), Box::new(Typ::TBool));
//         let ctx: Ctx = vec![];
//         let mut g = quickcheck::Gen::new(1);
//         let expr = gen_expr(ctx, t, &mut g, 1);
//         assert!(expr.is_some());
//     }

//     #[test]
//     fn test_gen_expr2() {
//         let t = Typ::TFun(Box::new(Typ::TBool), Box::new(Typ::TBool));
//         let ctx: Ctx = vec![
//             Typ::TBool,
//             Typ::TFun(Box::new(Typ::TBool), Box::new(Typ::TBool)),
//         ];
//         let mut g = quickcheck::Gen::new(5);
//         let expr = gen_expr(ctx, t, &mut g, 5);
//         // expr should have a `Var`
//         fn count_vars(expr: &Expr) -> usize {
//             match expr {
//                 Expr::Var(_) => 1,
//                 Expr::Abs(_, body) => count_vars(body),
//                 Expr::App(func, arg) => count_vars(func) + count_vars(arg),
//                 Expr::Bool(_) => 0,
//             }
//         }
//         let var_count = expr.as_ref().map_or(0, |e| count_vars(e));
//         assert!(
//             var_count > 0,
//             "Generated expression should contain at least one variable"
//         );
//         println!("Generated expression: {}", expr.unwrap());
//     }
// }
