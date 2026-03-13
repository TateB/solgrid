//! Go-to-definition — single-file symbol resolution for Solidity.

use solgrid_parser::solar_ast::{
    CallArgsKind, ContractKind, Expr, ExprKind, FunctionKind, Item, ItemKind, Stmt, StmtKind,
};
use solgrid_parser::solar_interface::SpannedOption;
use solgrid_parser::with_parsed_ast_sequential;
use std::ops::Range;

/// A resolved symbol with its definition location and metadata for hover.
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    /// Byte range of the name at the definition site.
    pub name_range: Range<usize>,
    /// Byte range of the full item (for signature extraction).
    pub item_range: Range<usize>,
}

/// Find the definition of the symbol at the given byte offset.
pub fn find_definition(source: &str, offset: usize) -> Option<ResolvedSymbol> {
    // First pass: collect declarations using owned data only.
    let decls = with_parsed_ast_sequential(source, "<definition>", |source_unit| {
        source_unit
            .items
            .iter()
            .filter_map(|item| decl_from_item(item))
            .collect::<Vec<Declaration>>()
    })
    .ok()?;

    // Second pass: find the item containing offset and resolve.
    with_parsed_ast_sequential(source, "<definition>", |source_unit| {
        for item in source_unit.items.iter() {
            let r = span(item.span);
            if offset >= r.start && offset < r.end {
                return find_in_item(source, offset, item, &decls);
            }
        }
        None
    })
    .ok()
    .flatten()
}

// -- Declaration collection --

#[derive(Debug, Clone)]
struct Declaration {
    name: String,
    name_range: Range<usize>,
    item_range: Range<usize>,
    members: Vec<Declaration>,
    kind: DeclKind,
}

#[derive(Debug, Clone, PartialEq)]
enum DeclKind {
    Contract,
    Interface,
    Library,
    Function,
    Modifier,
    Event,
    Error,
    Struct,
    Enum,
    Udvt,
    Variable,
}

fn span(s: solgrid_parser::solar_interface::Span) -> Range<usize> {
    solgrid_ast::span_to_range(s)
}

fn find_in_item(
    source: &str,
    offset: usize,
    item: &Item<'_>,
    decls: &[Declaration],
) -> Option<ResolvedSymbol> {
    match &item.kind {
        ItemKind::Contract(contract) => {
            let nr = span(contract.name.span);
            if offset >= nr.start && offset < nr.end {
                return None; // already at definition
            }
            for base in contract.bases.iter() {
                let br = span(base.name.span());
                if offset >= br.start && offset < br.end {
                    return lookup_type(&source[br.clone()], decls);
                }
            }
            find_in_contract_body(source, offset, contract, decls)
        }
        ItemKind::Function(func) => find_in_function(source, offset, func, item, decls, None),
        _ => None,
    }
}

fn decl_from_item(item: &Item<'_>) -> Option<Declaration> {
    match &item.kind {
        ItemKind::Contract(contract) => {
            let kind = match contract.kind {
                ContractKind::Interface => DeclKind::Interface,
                ContractKind::Library => DeclKind::Library,
                _ => DeclKind::Contract,
            };
            let members = collect_contract_members(contract.body);
            Some(Declaration {
                name: contract.name.as_str().to_string(),
                name_range: span(contract.name.span),
                item_range: span(item.span),
                members,
                kind,
            })
        }
        ItemKind::Function(func) => {
            let name_ident = func.header.name?;
            Some(Declaration {
                name: name_ident.as_str().to_string(),
                name_range: span(name_ident.span),
                item_range: span(item.span),
                members: vec![],
                kind: DeclKind::Function,
            })
        }
        ItemKind::Struct(s) => Some(Declaration {
            name: s.name.as_str().to_string(),
            name_range: span(s.name.span),
            item_range: span(item.span),
            members: vec![],
            kind: DeclKind::Struct,
        }),
        ItemKind::Enum(e) => Some(Declaration {
            name: e.name.as_str().to_string(),
            name_range: span(e.name.span),
            item_range: span(item.span),
            members: vec![],
            kind: DeclKind::Enum,
        }),
        ItemKind::Error(e) => Some(Declaration {
            name: e.name.as_str().to_string(),
            name_range: span(e.name.span),
            item_range: span(item.span),
            members: vec![],
            kind: DeclKind::Error,
        }),
        ItemKind::Udvt(u) => Some(Declaration {
            name: u.name.as_str().to_string(),
            name_range: span(u.name.span),
            item_range: span(item.span),
            members: vec![],
            kind: DeclKind::Udvt,
        }),
        ItemKind::Event(e) => Some(Declaration {
            name: e.name.as_str().to_string(),
            name_range: span(e.name.span),
            item_range: span(item.span),
            members: vec![],
            kind: DeclKind::Event,
        }),
        _ => None,
    }
}

fn collect_contract_members(items: &[Item<'_>]) -> Vec<Declaration> {
    let mut members = Vec::new();
    for item in items {
        match &item.kind {
            ItemKind::Function(func) => {
                let (name, kind) = if let Some(n) = func.header.name {
                    let k = if func.kind == FunctionKind::Modifier {
                        DeclKind::Modifier
                    } else {
                        DeclKind::Function
                    };
                    (n.as_str().to_string(), k)
                } else {
                    let n = match func.kind {
                        FunctionKind::Constructor => "constructor",
                        FunctionKind::Fallback => "fallback",
                        FunctionKind::Receive => "receive",
                        _ => continue,
                    };
                    (n.to_string(), DeclKind::Function)
                };
                let name_range = func
                    .header
                    .name
                    .map(|n| span(n.span))
                    .unwrap_or_else(|| {
                        let start = span(item.span).start;
                        start..start + name.len()
                    });
                members.push(Declaration {
                    name,
                    name_range,
                    item_range: span(item.span),
                    members: vec![],
                    kind,
                });
            }
            ItemKind::Variable(var) => {
                if let Some(n) = var.name {
                    members.push(Declaration {
                        name: n.as_str().to_string(),
                        name_range: span(n.span),
                        item_range: span(item.span),
                        members: vec![],
                        kind: DeclKind::Variable,
                    });
                }
            }
            ItemKind::Event(e) => members.push(Declaration {
                name: e.name.as_str().to_string(),
                name_range: span(e.name.span),
                item_range: span(item.span),
                members: vec![],
                kind: DeclKind::Event,
            }),
            ItemKind::Error(e) => members.push(Declaration {
                name: e.name.as_str().to_string(),
                name_range: span(e.name.span),
                item_range: span(item.span),
                members: vec![],
                kind: DeclKind::Error,
            }),
            ItemKind::Struct(s) => members.push(Declaration {
                name: s.name.as_str().to_string(),
                name_range: span(s.name.span),
                item_range: span(item.span),
                members: vec![],
                kind: DeclKind::Struct,
            }),
            ItemKind::Enum(e) => members.push(Declaration {
                name: e.name.as_str().to_string(),
                name_range: span(e.name.span),
                item_range: span(item.span),
                members: vec![],
                kind: DeclKind::Enum,
            }),
            ItemKind::Udvt(u) => members.push(Declaration {
                name: u.name.as_str().to_string(),
                name_range: span(u.name.span),
                item_range: span(item.span),
                members: vec![],
                kind: DeclKind::Udvt,
            }),
            _ => {}
        }
    }
    members
}

// -- Symbol resolution --

fn find_in_contract_body(
    source: &str,
    offset: usize,
    contract: &solgrid_parser::solar_ast::ItemContract<'_>,
    decls: &[Declaration],
) -> Option<ResolvedSymbol> {
    let cname = contract.name.as_str();
    for body_item in contract.body.iter() {
        let r = span(body_item.span);
        if offset < r.start || offset >= r.end {
            continue;
        }
        match &body_item.kind {
            ItemKind::Function(func) => {
                return find_in_function(source, offset, func, body_item, decls, Some(cname));
            }
            ItemKind::Variable(var) => {
                let tr = span(var.ty.span);
                if offset >= tr.start && offset < tr.end {
                    let t = &source[tr.clone()];
                    return lookup_type(t.split('[').next().unwrap_or(t).trim(), decls);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_in_function(
    source: &str,
    offset: usize,
    func: &solgrid_parser::solar_ast::ItemFunction<'_>,
    _item: &Item<'_>,
    decls: &[Declaration],
    enclosing: Option<&str>,
) -> Option<ResolvedSymbol> {
    if let Some(n) = func.header.name {
        let nr = span(n.span);
        if offset >= nr.start && offset < nr.end {
            return None; // at definition
        }
    }

    for param in func.header.parameters.iter() {
        let tr = span(param.ty.span);
        if offset >= tr.start && offset < tr.end {
            let t = &source[tr.clone()];
            return lookup_type(t.split('[').next().unwrap_or(t).trim(), decls);
        }
    }

    if let Some(returns) = &func.header.returns {
        for ret in returns.iter() {
            let tr = span(ret.ty.span);
            if offset >= tr.start && offset < tr.end {
                let t = &source[tr.clone()];
                return lookup_type(t.split('[').next().unwrap_or(t).trim(), decls);
            }
        }
    }

    for modifier in func.header.modifiers.iter() {
        let mr = span(modifier.name.span());
        if offset >= mr.start && offset < mr.end {
            let mod_name = &source[mr.clone()];
            if let Some(cname) = enclosing {
                if let Some(r) = lookup_member(mod_name, cname, decls) {
                    return Some(r);
                }
            }
            return lookup_name(mod_name, decls);
        }
    }

    if let Some(body) = &func.body {
        let mut locals = Vec::new();
        // Add function parameters as locals so resolve_expr_type can find their types.
        for param in func.header.parameters.iter() {
            if let Some(name) = param.name {
                locals.push(Declaration {
                    name: name.as_str().to_string(),
                    name_range: span(name.span),
                    item_range: span(param.ty.span),
                    members: vec![],
                    kind: DeclKind::Variable,
                });
            }
        }
        collect_local_vars(body.stmts, &mut locals);
        return find_in_stmts(source, offset, body.stmts, decls, enclosing, &locals);
    }

    None
}

// -- Local variable collection --

fn collect_local_vars(stmts: &[Stmt<'_>], locals: &mut Vec<Declaration>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::DeclSingle(var) => {
                if let Some(name) = var.name {
                    locals.push(Declaration {
                        name: name.as_str().to_string(),
                        name_range: span(name.span),
                        item_range: span(stmt.span),
                        members: vec![],
                        kind: DeclKind::Variable,
                    });
                }
            }
            StmtKind::DeclMulti(vars, _) => {
                for v in vars.iter() {
                    if let SpannedOption::Some(vd) = v {
                        if let Some(name) = vd.name {
                            locals.push(Declaration {
                                name: name.as_str().to_string(),
                                name_range: span(name.span),
                                item_range: span(stmt.span),
                                members: vec![],
                                kind: DeclKind::Variable,
                            });
                        }
                    }
                }
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                collect_local_vars(block.stmts, locals);
            }
            StmtKind::If(_, then_s, else_s) => {
                collect_local_vars_stmt(then_s, locals);
                if let Some(e) = else_s {
                    collect_local_vars_stmt(e, locals);
                }
            }
            StmtKind::For { body, .. } => collect_local_vars_stmt(body, locals),
            StmtKind::While(_, body) | StmtKind::DoWhile(body, _) => {
                collect_local_vars_stmt(body, locals);
            }
            _ => {}
        }
    }
}

fn collect_local_vars_stmt(stmt: &Stmt<'_>, locals: &mut Vec<Declaration>) {
    match &stmt.kind {
        StmtKind::DeclSingle(var) => {
            if let Some(name) = var.name {
                locals.push(Declaration {
                    name: name.as_str().to_string(),
                    name_range: span(name.span),
                    item_range: span(stmt.span),
                    members: vec![],
                    kind: DeclKind::Variable,
                });
            }
        }
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
            collect_local_vars(block.stmts, locals);
        }
        _ => {}
    }
}

// -- Statement traversal --

fn find_in_stmt(
    source: &str,
    offset: usize,
    stmt: &Stmt<'_>,
    decls: &[Declaration],
    enclosing: Option<&str>,
    locals: &[Declaration],
) -> Option<ResolvedSymbol> {
    find_in_stmts(source, offset, std::slice::from_ref(stmt), decls, enclosing, locals)
}

fn find_in_stmts(
    source: &str,
    offset: usize,
    stmts: &[Stmt<'_>],
    decls: &[Declaration],
    enclosing: Option<&str>,
    locals: &[Declaration],
) -> Option<ResolvedSymbol> {
    for stmt in stmts {
        let sr = span(stmt.span);
        if offset < sr.start || offset >= sr.end {
            continue;
        }
        match &stmt.kind {
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                return find_in_stmts(source, offset, block.stmts, decls, enclosing, locals);
            }
            StmtKind::DeclSingle(var) => {
                let tr = span(var.ty.span);
                if offset >= tr.start && offset < tr.end {
                    let t = &source[tr.clone()];
                    return lookup_type(t.split('[').next().unwrap_or(t).trim(), decls);
                }
                if let Some(init) = &var.initializer {
                    return find_in_expr(source, offset, init, decls, enclosing, locals);
                }
            }
            StmtKind::DeclMulti(_, init) => {
                return find_in_expr(source, offset, init, decls, enclosing, locals);
            }
            StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => {
                return find_in_expr(source, offset, expr, decls, enclosing, locals);
            }
            StmtKind::Emit(_, args) | StmtKind::Revert(_, args) => {
                return find_in_call_args(source, offset, args, decls, enclosing, locals);
            }
            StmtKind::If(cond, then_s, else_s) => {
                if let Some(r) = find_in_expr(source, offset, cond, decls, enclosing, locals) {
                    return Some(r);
                }
                if let Some(r) = find_in_stmt(source, offset, then_s, decls, enclosing, locals) {
                    return Some(r);
                }
                if let Some(e) = else_s {
                    return find_in_stmt(source, offset, e, decls, enclosing, locals);
                }
            }
            StmtKind::For {
                init,
                cond,
                next,
                body,
            } => {
                if let Some(i) = init {
                    if let Some(r) = find_in_stmt(source, offset, i, decls, enclosing, locals) {
                        return Some(r);
                    }
                }
                if let Some(c) = cond {
                    if let Some(r) = find_in_expr(source, offset, c, decls, enclosing, locals) {
                        return Some(r);
                    }
                }
                if let Some(n) = next {
                    if let Some(r) = find_in_expr(source, offset, n, decls, enclosing, locals) {
                        return Some(r);
                    }
                }
                return find_in_stmt(source, offset, body, decls, enclosing, locals);
            }
            StmtKind::While(cond, body) => {
                if let Some(r) = find_in_expr(source, offset, cond, decls, enclosing, locals) {
                    return Some(r);
                }
                return find_in_stmt(source, offset, body, decls, enclosing, locals);
            }
            StmtKind::DoWhile(body, cond) => {
                if let Some(r) = find_in_stmt(source, offset, body, decls, enclosing, locals) {
                    return Some(r);
                }
                return find_in_expr(source, offset, cond, decls, enclosing, locals);
            }
            StmtKind::Try(try_stmt) => {
                if let Some(r) =
                    find_in_expr(source, offset, try_stmt.expr, decls, enclosing, locals)
                {
                    return Some(r);
                }
                for clause in try_stmt.clauses.iter() {
                    if let Some(r) = find_in_stmts(
                        source,
                        offset,
                        clause.block.stmts,
                        decls,
                        enclosing,
                        locals,
                    ) {
                        return Some(r);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// -- Expression traversal --

fn find_in_call_args(
    source: &str,
    offset: usize,
    args: &solgrid_parser::solar_ast::CallArgs<'_>,
    decls: &[Declaration],
    enclosing: Option<&str>,
    locals: &[Declaration],
) -> Option<ResolvedSymbol> {
    match &args.kind {
        CallArgsKind::Unnamed(exprs) => {
            for e in exprs.iter() {
                if let Some(r) = find_in_expr(source, offset, e, decls, enclosing, locals) {
                    return Some(r);
                }
            }
        }
        CallArgsKind::Named(named) => {
            for arg in named.iter() {
                if let Some(r) = find_in_expr(source, offset, arg.value, decls, enclosing, locals)
                {
                    return Some(r);
                }
            }
        }
    }
    None
}

fn find_in_expr(
    source: &str,
    offset: usize,
    expr: &Expr<'_>,
    decls: &[Declaration],
    enclosing: Option<&str>,
    locals: &[Declaration],
) -> Option<ResolvedSymbol> {
    let er = span(expr.span);
    if offset < er.start || offset >= er.end {
        return None;
    }

    match &expr.kind {
        ExprKind::Ident(ident) => {
            let ir = span(ident.span);
            if offset >= ir.start && offset < ir.end {
                let name = ident.as_str();
                if let Some(r) = lookup_in_locals(name, locals) {
                    return Some(r);
                }
                if let Some(cname) = enclosing {
                    if let Some(r) = lookup_member(name, cname, decls) {
                        return Some(r);
                    }
                }
                return lookup_name(name, decls);
            }
        }
        ExprKind::Member(base, member) => {
            let mr = span(member.span);
            if offset >= mr.start && offset < mr.end {
                if let Some(base_type) =
                    resolve_expr_type(source, base, decls, enclosing, locals)
                {
                    return lookup_member(member.as_str(), &base_type, decls);
                }
            }
            return find_in_expr(source, offset, base, decls, enclosing, locals);
        }
        ExprKind::Call(callee, args) => {
            if let Some(r) = find_in_expr(source, offset, callee, decls, enclosing, locals) {
                return Some(r);
            }
            return find_in_call_args(source, offset, args, decls, enclosing, locals);
        }
        ExprKind::CallOptions(callee, options) => {
            if let Some(r) = find_in_expr(source, offset, callee, decls, enclosing, locals) {
                return Some(r);
            }
            for opt in options.iter() {
                if let Some(r) = find_in_expr(source, offset, opt.value, decls, enclosing, locals)
                {
                    return Some(r);
                }
            }
        }
        ExprKind::Index(base, _index) => {
            return find_in_expr(source, offset, base, decls, enclosing, locals);
        }
        ExprKind::Binary(lhs, _, rhs) | ExprKind::Assign(lhs, _, rhs) => {
            if let Some(r) = find_in_expr(source, offset, lhs, decls, enclosing, locals) {
                return Some(r);
            }
            return find_in_expr(source, offset, rhs, decls, enclosing, locals);
        }
        ExprKind::Unary(_, operand) | ExprKind::Delete(operand) => {
            return find_in_expr(source, offset, operand, decls, enclosing, locals);
        }
        ExprKind::Ternary(cond, if_true, if_false) => {
            if let Some(r) = find_in_expr(source, offset, cond, decls, enclosing, locals) {
                return Some(r);
            }
            if let Some(r) = find_in_expr(source, offset, if_true, decls, enclosing, locals) {
                return Some(r);
            }
            return find_in_expr(source, offset, if_false, decls, enclosing, locals);
        }
        ExprKind::Tuple(elements) => {
            for elem in elements.iter() {
                if let SpannedOption::Some(e) = elem {
                    if let Some(r) = find_in_expr(source, offset, e, decls, enclosing, locals) {
                        return Some(r);
                    }
                }
            }
        }
        ExprKind::Array(elements) => {
            for e in elements.iter() {
                if let Some(r) = find_in_expr(source, offset, e, decls, enclosing, locals) {
                    return Some(r);
                }
            }
        }
        ExprKind::New(ty) | ExprKind::Type(ty) | ExprKind::TypeCall(ty) => {
            let tr = span(ty.span);
            if offset >= tr.start && offset < tr.end {
                let t = &source[tr.clone()];
                return lookup_type(t.split('[').next().unwrap_or(t).trim(), decls);
            }
        }
        ExprKind::Payable(args) => {
            return find_in_call_args(source, offset, args, decls, enclosing, locals);
        }
        _ => {}
    }
    None
}

// -- Type resolution for member access --

fn resolve_expr_type(
    source: &str,
    expr: &Expr<'_>,
    decls: &[Declaration],
    enclosing: Option<&str>,
    locals: &[Declaration],
) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(ident) => {
            let name = ident.as_str();
            for decl in decls {
                if decl.name == name
                    && matches!(
                        decl.kind,
                        DeclKind::Contract | DeclKind::Interface | DeclKind::Library
                    )
                {
                    return Some(name.to_string());
                }
            }
            for local in locals.iter().rev() {
                if local.name == name {
                    return extract_type_from_var_decl(&source[local.item_range.clone()]);
                }
            }
            if let Some(cname) = enclosing {
                for decl in decls {
                    if decl.name == cname {
                        for member in &decl.members {
                            if member.name == name && member.kind == DeclKind::Variable {
                                return extract_type_from_var_decl(
                                    &source[member.item_range.clone()],
                                );
                            }
                        }
                    }
                }
            }
            None
        }
        ExprKind::New(ty) => {
            let t = &source[span(ty.span)];
            Some(t.trim().to_string())
        }
        _ => None,
    }
}

fn extract_type_from_var_decl(decl_text: &str) -> Option<String> {
    let first_word = decl_text.split_whitespace().next()?;
    Some(first_word.split('[').next().unwrap_or(first_word).to_string())
}

// -- Lookup helpers --

fn lookup_name(name: &str, decls: &[Declaration]) -> Option<ResolvedSymbol> {
    for decl in decls {
        if decl.name == name {
            return Some(ResolvedSymbol {
                name_range: decl.name_range.clone(),
                item_range: decl.item_range.clone(),
            });
        }
        for member in &decl.members {
            if member.name == name {
                return Some(ResolvedSymbol {
                    name_range: member.name_range.clone(),
                    item_range: member.item_range.clone(),
                });
            }
        }
    }
    None
}

fn lookup_in_locals(name: &str, locals: &[Declaration]) -> Option<ResolvedSymbol> {
    for local in locals.iter().rev() {
        if local.name == name {
            return Some(ResolvedSymbol {
                name_range: local.name_range.clone(),
                item_range: local.item_range.clone(),
            });
        }
    }
    None
}

fn lookup_type(name: &str, decls: &[Declaration]) -> Option<ResolvedSymbol> {
    for decl in decls {
        if decl.name == name {
            return Some(ResolvedSymbol {
                name_range: decl.name_range.clone(),
                item_range: decl.item_range.clone(),
            });
        }
        for member in &decl.members {
            if member.name == name
                && matches!(
                    member.kind,
                    DeclKind::Struct | DeclKind::Enum | DeclKind::Error | DeclKind::Udvt
                )
            {
                return Some(ResolvedSymbol {
                    name_range: member.name_range.clone(),
                    item_range: member.item_range.clone(),
                });
            }
        }
    }
    None
}

fn lookup_member(
    member_name: &str,
    contract_name: &str,
    decls: &[Declaration],
) -> Option<ResolvedSymbol> {
    for decl in decls {
        if decl.name == contract_name
            && matches!(
                decl.kind,
                DeclKind::Contract | DeclKind::Interface | DeclKind::Library
            )
        {
            for member in &decl.members {
                if member.name == member_name {
                    return Some(ResolvedSymbol {
                        name_range: member.name_range.clone(),
                        item_range: member.item_range.clone(),
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_definition_simple_function() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public pure returns (uint256) {
        return 42;
    }

    function bar() public pure returns (uint256) {
        return foo();
    }
}"#;
        let foo_call = source.find("return foo()").unwrap() + 7;
        let result = find_definition(source, foo_call);
        assert!(result.is_some(), "should find definition of foo");
        assert_eq!(&source[result.unwrap().name_range], "foo");
    }

    #[test]
    fn test_find_definition_member_access() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {
    function balanceOf(address) external pure returns (uint256) {
        return 0;
    }
}

contract Main {
    function check(Token token) public pure returns (uint256) {
        return token.balanceOf(address(0));
    }
}"#;
        let pos = source.find("token.balanceOf").unwrap() + 6;
        let result = find_definition(source, pos);
        assert!(result.is_some(), "should find definition of balanceOf");
        assert_eq!(&source[result.unwrap().name_range], "balanceOf");
    }

    #[test]
    fn test_find_definition_contract_type() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Token {
    uint256 public supply;
}

contract Main {
    Token public token;
}"#;
        let pos = source.rfind("Token").unwrap();
        let result = find_definition(source, pos);
        assert!(result.is_some(), "should find definition of Token");
        assert_eq!(&source[result.unwrap().name_range], "Token");
    }

    #[test]
    fn test_find_definition_at_definition_returns_none() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public pure {}
}"#;
        let pos = source.find("function foo").unwrap() + 9;
        let result = find_definition(source, pos);
        assert!(result.is_none());
    }
}
