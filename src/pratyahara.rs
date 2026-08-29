use crate::ast::Expr;

pub const MASK_AL: u64 = 0x000003FFFFFFFFFF;

pub fn get_pratyahara_mask(name: &str) -> Option<u64> {
    match name {
        "ac" => Some(511),            // 0x00000000000001FF
        "ak" => Some(31),             // 0x000000000000001F
        "ik" => Some(30),             // 0x000000000000001E
        "uk" => Some(28),             // 0x000000000000001C
        "eN" => Some(96),             // 0x0000000000000060
        "ec" => Some(480),            // 0x00000000000001E0
        "Ec" => Some(384),            // 0x0000000000000180
        "al" => Some(4398046511103),  // 0x000003FFFFFFFFFF
        "hal" => Some(4398046510592), // 0x000003FFFFFFFE00
        "val" => Some(4398046510080), // 0x000003FFFFFFFC00
        "ral" => Some(4398046507008), // 0x000003FFFFFFF000
        "Jal" => Some(4398045987328), // 0x000003FFFFF80200
        "Sal" => Some(3848290697728), // 0x0000038000000200
        "Sar" => Some(3848290697216), // 0x0000038000000000
        "yar" => Some(4398046510080), // 0x000003FFFFFFFC00
        "yay" => Some(549755812864),  // 0x0000007FFFFFFC00
        "yaR" => Some(15360),         // 0x0000000000003C00
        "yam" => Some(523264),        // 0x000000000007FC00
        "yaY" => Some(2096128),       // 0x00000000001FFC00
        "vaw" => Some(6144),          // 0x0000000000001800
        "may" => Some(549755781120),  // 0x0000007FFFFF8000
        "am" => Some(524287),         // 0x000000000007FFFF
        "aw" => Some(8191),           // 0x0000000000001FFF
        "iR" => Some(16382),          // 0x0000000000003FFE
        "aR" => Some(7),              // 0x0000000000000007
        "eR" => Some(96),             // 0x0000000000000060
        "nam" => Some(458752),        // 0x0000000000070000
        "JaS" => Some(536346624),     // 0x000000001FF80000
        "jaS" => Some(520093696),     // 0x000000001F000000
        "baS" => Some(503316480),     // 0x000000001E000000
        "Jaz" => Some(16252928),      // 0x0000000000F80000
        "Baz" => Some(15728640),      // 0x0000000000F00000
        "Jay" => Some(549755289600),  // 0x0000007FFFF80000
        "Kay" => Some(549218942976),  // 0x0000007FE0000000
        "xay" => Some(136902082560),  // 0x0000001FE0000000
        "car" => Some(4380866641920), // 0x000003FC00000000
        "cav" => Some(120259084288),  // 0x0000001C00000000
        "caw" => Some(51539607552),   // 0x0000000C00000000
        "Kar" => Some(4397509640192), // 0x000003FFE0000000
        "Jar" => Some(4398045986816), // 0x000003FFFFF80000
        "haS" => Some(536870400),     // 0x000000001FFFFE00
        _ => None,
    }
}

fn resolve_mask_value(expr: &Expr) -> Option<u64> {
    match expr {
        Expr::Integer(v) => Some(*v as u64),
        Expr::Symbol(s) => get_pratyahara_mask(s),
        Expr::List(list) if list.len() == 2 && list[0].is_symbol("quote") => {
            if let Expr::Symbol(s) = &list[1] {
                get_pratyahara_mask(s)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn fold_constants(expr: &Expr) -> Expr {
    match expr {
        Expr::List(items) => {
            let folded: Vec<Expr> = items.iter().map(fold_constants).collect();
            if folded.is_empty() {
                return Expr::List(folded);
            }
            if let Expr::Symbol(op) = &folded[0] {
                let args = &folded[1..];

                if op == "quote" && args.len() == 1 {
                    if let Expr::Symbol(s) = &args[0] {
                        if let Some(mask) = get_pratyahara_mask(s) {
                            return Expr::Integer(mask as i64);
                        }
                    }
                    return Expr::List(folded);
                }

                if (op == "pratyahara-mask" || op == "pratyahara") && args.len() == 1 {
                    if let Some(mask) = resolve_mask_value(&args[0]) {
                        return Expr::Integer(mask as i64);
                    }
                }

                if (op == "intersection"
                    || op == "pratyahara-intersect"
                    || op == "pratyahara-intersection"
                    || op == "bit-and"
                    || op == "and*")
                    && args.len() == 2
                {
                    if let (Some(m1), Some(m2)) =
                        (resolve_mask_value(&args[0]), resolve_mask_value(&args[1]))
                    {
                        return Expr::Integer((m1 & m2) as i64);
                    }
                }

                if (op == "union" || op == "pratyahara-union" || op == "bit-or" || op == "or*")
                    && args.len() == 2
                {
                    if let (Some(m1), Some(m2)) =
                        (resolve_mask_value(&args[0]), resolve_mask_value(&args[1]))
                    {
                        return Expr::Integer(((m1 | m2) & MASK_AL) as i64);
                    }
                }

                if (op == "diff"
                    || op == "difference"
                    || op == "pratyahara-diff"
                    || op == "pratyahara-difference")
                    && args.len() == 2
                {
                    if let (Some(m1), Some(m2)) =
                        (resolve_mask_value(&args[0]), resolve_mask_value(&args[1]))
                    {
                        return Expr::Integer((m1 & (!m2) & MASK_AL) as i64);
                    }
                }

                if (op == "complement" || op == "pratyahara-complement" || op == "bit-not")
                    && args.len() == 1
                {
                    if let Some(m) = resolve_mask_value(&args[0]) {
                        return Expr::Integer(((!m) & MASK_AL) as i64);
                    }
                }

                if (op == "subset?" || op == "pratyahara-subset?") && args.len() == 2 {
                    if let (Some(m1), Some(m2)) =
                        (resolve_mask_value(&args[0]), resolve_mask_value(&args[1]))
                    {
                        if (m1 & (!m2) & MASK_AL) == 0 {
                            return Expr::Symbol("t".to_string());
                        } else {
                            return Expr::Symbol("nil".to_string());
                        }
                    }
                }
            }
            Expr::List(folded)
        }
        Expr::DottedList(items, tail) => {
            let folded_items = items.iter().map(fold_constants).collect();
            let folded_tail = Box::new(fold_constants(tail));
            Expr::DottedList(folded_items, folded_tail)
        }
        _ => expr.clone(),
    }
}
