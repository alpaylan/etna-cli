use crate::{
    implementation::{Ctx, Expr, Typ},
    spec,
    strategies::bespoke::ExprOpt,
};
use hegel::{
    TestCase,
    composite,
    generators::{booleans, integers},
};

fn draw_bool(tc: &TestCase) -> bool {
    tc.draw(booleans())
}

#[composite]
fn usizes(tc: TestCase, min: usize, max: usize) -> usize {
    tc.draw(integers::<usize>().min_value(min).max_value(max))
}

fn choose_index(tc: &TestCase, len: usize) -> usize {
    if len <= 1 {
        0
    } else {
        tc.draw(usizes(0, len - 1))
    }
}

fn gen_var(ctx: &Ctx, t: &Typ, tc: &TestCase) -> Option<Expr> {
    let candidates: Vec<usize> = ctx
        .iter()
        .enumerate()
        .filter_map(|(i, t2)| if t2 == t { Some(i) } else { None })
        .collect();

    if candidates.is_empty() {
        None
    } else {
        let i = choose_index(tc, candidates.len());
        Some(Expr::Var(candidates[i] as i32))
    }
}

fn gen_typ(tc: &TestCase, size: usize) -> Typ {
    if size == 0 || tc.draw(usizes(0, size)) == 0 {
        Typ::TBool
    } else {
        Typ::TFun(
            Box::new(gen_typ(tc, size / 2)),
            Box::new(gen_typ(tc, size / 2)),
        )
    }
}

fn gen_one(ctx: &Ctx, t: &Typ, tc: &TestCase) -> Expr {
    match t {
        Typ::TBool => Expr::Bool(draw_bool(tc)),
        Typ::TFun(t1, t2) => {
            let mut ctx1 = ctx.clone();
            ctx1.insert(0, *t1.clone());
            let e = gen_one(&ctx1, t2, tc);
            Expr::Abs(*t1.clone(), Box::new(e))
        }
    }
}

fn gen_abs(ctx: &Ctx, t1: Typ, t2: Typ, tc: &TestCase, size: usize) -> Expr {
    let mut ctx1 = ctx.clone();
    ctx1.insert(0, t1.clone());
    let e = gen_exact_expr(ctx1, t2, tc, size);
    Expr::Abs(t1, Box::new(e))
}

fn gen_app(ctx: &Ctx, t: &Typ, tc: &TestCase, size: usize) -> Expr {
    let t_prime = gen_typ(tc, 5);
    let e1 = gen_exact_expr(
        ctx.clone(),
        Typ::TFun(Box::new(t_prime.clone()), Box::new(t.clone())),
        tc,
        size / 2,
    );
    let e2 = gen_exact_expr(ctx.clone(), t_prime, tc, size / 2);
    Expr::App(Box::new(e1), Box::new(e2))
}

fn gen_exact_expr(ctx: Ctx, t: Typ, tc: &TestCase, size: usize) -> Expr {
    if size == 0 {
        if let Some(v) = gen_var(&ctx, &t, tc) {
            if draw_bool(tc) {
                v
            } else {
                gen_one(&ctx, &t, tc)
            }
        } else {
            gen_one(&ctx, &t, tc)
        }
    } else {
        let mut options = vec![0_u8, 1_u8];
        if let Typ::TFun(_, _) = t {
            options.push(2_u8);
        }
        let maybe_var = gen_var(&ctx, &t, tc);
        if maybe_var.is_some() {
            options.push(3_u8);
        }

        let choice = options[choose_index(tc, options.len())];
        match choice {
            0 => gen_one(&ctx, &t, tc),
            1 => gen_app(&ctx, &t, tc, size),
            2 => {
                let Typ::TFun(t1, t2) = t else {
                    unreachable!("option 2 is only enabled for function types")
                };
                gen_abs(&ctx, *t1, *t2, tc, size.saturating_sub(1))
            }
            3 => maybe_var.expect("option 3 is only enabled when a variable exists"),
            _ => unreachable!("invalid generator choice"),
        }
    }
}

fn draw_expr(tc: &TestCase) -> Expr {
    let typ = gen_typ(tc, 5);
    let size = tc.draw(usizes(0, 10));
    gen_exact_expr(vec![], typ, tc, size)
}

pub fn draw_case(property: &str, tc: &TestCase) -> Option<(String, Option<bool>)> {
    let expr = draw_expr(tc);
    let sample = format!("{}", expr);
    let wrapped = ExprOpt(Some(expr));
    let result = match property {
        "SinglePreserve" => spec::prop_single_preserve(wrapped),
        "MultiPreserve" => spec::prop_multi_preserve(wrapped),
        _ => return None,
    };

    Some((sample, result))
}
