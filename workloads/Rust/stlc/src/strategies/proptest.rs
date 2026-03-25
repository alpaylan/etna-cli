use crate::{
    implementation::{Ctx, Expr, Typ},
    spec,
    strategies::bespoke::ExprOpt,
};
use proptest::{collection::vec, prelude::*};

struct ChoiceStream {
    nums: Vec<u64>,
    bools: Vec<bool>,
    num_idx: usize,
    bool_idx: usize,
}

impl ChoiceStream {
    fn new(nums: Vec<u64>, bools: Vec<bool>) -> Self {
        Self {
            nums,
            bools,
            num_idx: 0,
            bool_idx: 0,
        }
    }

    fn next_u64(&mut self) -> u64 {
        if self.num_idx < self.nums.len() {
            let n = self.nums[self.num_idx];
            self.num_idx += 1;
            n
        } else {
            0
        }
    }

    fn next_bool(&mut self) -> bool {
        if self.bool_idx < self.bools.len() {
            let b = self.bools[self.bool_idx];
            self.bool_idx += 1;
            b
        } else {
            self.next_u64() & 1 == 1
        }
    }
}

fn draw_usize(stream: &mut ChoiceStream, min: usize, max: usize) -> usize {
    if max <= min {
        return min;
    }
    let span = max - min + 1;
    min + (stream.next_u64() as usize % span)
}

fn choose_index(stream: &mut ChoiceStream, len: usize) -> usize {
    if len <= 1 {
        0
    } else {
        draw_usize(stream, 0, len - 1)
    }
}

fn gen_var(ctx: &Ctx, t: &Typ, stream: &mut ChoiceStream) -> Option<Expr> {
    let candidates: Vec<usize> = ctx
        .iter()
        .enumerate()
        .filter_map(|(i, t2)| if t2 == t { Some(i) } else { None })
        .collect();

    if candidates.is_empty() {
        None
    } else {
        let i = choose_index(stream, candidates.len());
        Some(Expr::Var(candidates[i] as i32))
    }
}

fn gen_typ(stream: &mut ChoiceStream, size: usize) -> Typ {
    if size == 0 || draw_usize(stream, 0, size) == 0 {
        Typ::TBool
    } else {
        Typ::TFun(
            Box::new(gen_typ(stream, size / 2)),
            Box::new(gen_typ(stream, size / 2)),
        )
    }
}

fn gen_one(ctx: &Ctx, t: &Typ, stream: &mut ChoiceStream) -> Expr {
    match t {
        Typ::TBool => Expr::Bool(stream.next_bool()),
        Typ::TFun(t1, t2) => {
            let mut ctx1 = ctx.clone();
            ctx1.insert(0, *t1.clone());
            let e = gen_one(&ctx1, t2, stream);
            Expr::Abs(*t1.clone(), Box::new(e))
        }
    }
}

fn gen_abs(ctx: &Ctx, t1: Typ, t2: Typ, stream: &mut ChoiceStream, size: usize) -> Expr {
    let mut ctx1 = ctx.clone();
    ctx1.insert(0, t1.clone());
    let e = gen_exact_expr(ctx1, t2, stream, size);
    Expr::Abs(t1, Box::new(e))
}

fn gen_app(ctx: &Ctx, t: &Typ, stream: &mut ChoiceStream, size: usize) -> Expr {
    let t_prime = gen_typ(stream, 5);
    let e1 = gen_exact_expr(
        ctx.clone(),
        Typ::TFun(Box::new(t_prime.clone()), Box::new(t.clone())),
        stream,
        size / 2,
    );
    let e2 = gen_exact_expr(ctx.clone(), t_prime, stream, size / 2);
    Expr::App(Box::new(e1), Box::new(e2))
}

fn gen_exact_expr(ctx: Ctx, t: Typ, stream: &mut ChoiceStream, size: usize) -> Expr {
    if size == 0 {
        if let Some(v) = gen_var(&ctx, &t, stream) {
            if stream.next_bool() {
                v
            } else {
                gen_one(&ctx, &t, stream)
            }
        } else {
            gen_one(&ctx, &t, stream)
        }
    } else {
        let mut options = vec![0_u8, 1_u8];
        if let Typ::TFun(_, _) = t {
            options.push(2_u8);
        }
        let maybe_var = gen_var(&ctx, &t, stream);
        if maybe_var.is_some() {
            options.push(3_u8);
        }

        let choice = options[choose_index(stream, options.len())];
        match choice {
            0 => gen_one(&ctx, &t, stream),
            1 => gen_app(&ctx, &t, stream, size),
            2 => {
                let Typ::TFun(t1, t2) = t else {
                    unreachable!("option 2 is only enabled for function types")
                };
                gen_abs(&ctx, *t1, *t2, stream, size.saturating_sub(1))
            }
            3 => maybe_var.expect("option 3 is only enabled when a variable exists"),
            _ => unreachable!("invalid generator choice"),
        }
    }
}

fn draw_expr(stream: &mut ChoiceStream) -> Expr {
    let typ = gen_typ(stream, 5);
    let size = draw_usize(stream, 0, 10);
    gen_exact_expr(vec![], typ, stream, size)
}

fn expr_strategy() -> BoxedStrategy<Expr> {
    (vec(any::<u64>(), 64..256), vec(any::<bool>(), 64..256))
        .prop_map(|(nums, bools)| {
            let mut stream = ChoiceStream::new(nums, bools);
            draw_expr(&mut stream)
        })
        .boxed()
}

pub fn strategy_for(property: &str) -> Option<BoxedStrategy<(String, Option<bool>)>> {
    let strategy = match property {
        "SinglePreserve" => expr_strategy()
            .prop_map(|expr| {
                let sample = format!("{}", expr);
                (sample, spec::prop_single_preserve(ExprOpt(Some(expr))))
            })
            .boxed(),
        "MultiPreserve" => expr_strategy()
            .prop_map(|expr| {
                let sample = format!("{}", expr);
                (sample, spec::prop_multi_preserve(ExprOpt(Some(expr))))
            })
            .boxed(),
        _ => return None,
    };

    Some(strategy)
}
