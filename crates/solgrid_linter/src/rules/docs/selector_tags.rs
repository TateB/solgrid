//! Rule: docs/selector-tags
//!
//! Enforce canonical selector tags on interfaces and custom errors.

use crate::context::LintContext;
use crate::rule::Rule;
use sha3::{Digest, Keccak256};
use solgrid_ast::natspec::{find_attached_natspec, render_triple_slash_block, NatSpecStyle};
use solgrid_ast::resolve::ImportResolver;
use solgrid_ast::symbols::{build_symbol_table, ImportInfo, ImportedSymbols};
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{
    ContractKind, ElementaryType, FunctionKind, Item, ItemFunction, ItemKind, Type, TypeKind,
};
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

static META: RuleMeta = RuleMeta {
    id: "docs/selector-tags",
    name: "selector-tags",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "interfaces and custom errors should document their canonical selectors",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct SelectorTagsRule;

#[derive(Debug, Clone)]
struct FileTypeInfo {
    imports: Vec<ImportInfo>,
    top_level: HashMap<String, StoredTypeDef>,
    nested: HashMap<(String, String), StoredTypeDef>,
}

#[derive(Debug, Clone)]
struct StoredTypeDef {
    owner: Option<String>,
    kind: StoredTypeKind,
}

#[derive(Debug, Clone)]
enum StoredTypeKind {
    Struct(Vec<TypeShape>),
    Enum,
    ContractLike,
    Udvt(TypeShape),
}

#[derive(Debug, Clone)]
enum TypeShape {
    Elementary(String),
    Custom(Vec<String>),
    Array(Box<TypeShape>, Option<String>),
    Raw(String),
}

#[derive(Debug, Clone)]
struct ResolvedTypeDef {
    file: PathBuf,
    def: StoredTypeDef,
}

struct TypeDatabase<'a> {
    resolver: ImportResolver,
    cache: HashMap<PathBuf, FileTypeInfo>,
    inline_path: PathBuf,
    inline_source: &'a str,
}

impl Rule for SelectorTagsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let root = ctx.path.parent().map(Path::to_path_buf);
        let inline_path = canonicalize_path(ctx.path);
        let mut db = TypeDatabase {
            resolver: ImportResolver::new(root),
            cache: HashMap::new(),
            inline_path: inline_path.clone(),
            inline_source: ctx.source,
        };

        let filename = ctx.path.to_string_lossy().to_string();
        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                match &item.kind {
                    ItemKind::Error(error) => {
                        let expected = selector_tag_line(
                            "Error",
                            error_selector_hex(
                                error.name.as_str(),
                                &error
                                    .parameters
                                    .iter()
                                    .map(|param| {
                                        db.canonical_type(
                                            &inline_path,
                                            None,
                                            &type_shape_from_ast(ctx.source, &param.ty),
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                            ),
                        );
                        if let Some(diag) = selector_diagnostic(ctx, item, &expected, "Error") {
                            diagnostics.push(diag);
                        }
                    }
                    ItemKind::Contract(contract) if contract.kind == ContractKind::Interface => {
                        let functions: Vec<_> = contract
                            .body
                            .iter()
                            .filter_map(|body_item| {
                                let ItemKind::Function(func) = &body_item.kind else {
                                    return None;
                                };
                                (func.kind == FunctionKind::Function).then_some(func)
                            })
                            .collect();

                        if functions.is_empty() {
                            continue;
                        }

                        let interface_id = functions.iter().fold([0u8; 4], |mut acc, func| {
                            let selector = function_selector(
                                ctx.source,
                                &mut db,
                                &inline_path,
                                Some(contract.name.as_str()),
                                func,
                            );
                            for (byte, other) in acc.iter_mut().zip(selector) {
                                *byte ^= other;
                            }
                            acc
                        });

                        let expected = selector_tag_line("Interface", selector_hex(interface_id));
                        if let Some(diag) = selector_diagnostic(ctx, item, &expected, "Interface") {
                            diagnostics.push(diag);
                        }

                        for body_item in contract.body.iter() {
                            if let ItemKind::Error(error) = &body_item.kind {
                                let expected = selector_tag_line(
                                    "Error",
                                    error_selector_hex(
                                        error.name.as_str(),
                                        &error
                                            .parameters
                                            .iter()
                                            .map(|param| {
                                                db.canonical_type(
                                                    &inline_path,
                                                    Some(contract.name.as_str()),
                                                    &type_shape_from_ast(ctx.source, &param.ty),
                                                )
                                            })
                                            .collect::<Vec<_>>(),
                                    ),
                                );
                                if let Some(diag) =
                                    selector_diagnostic(ctx, body_item, &expected, "Error")
                                {
                                    diagnostics.push(diag);
                                }
                            }
                        }
                    }
                    ItemKind::Contract(contract) => {
                        for body_item in contract.body.iter() {
                            if let ItemKind::Error(error) = &body_item.kind {
                                let expected = selector_tag_line(
                                    "Error",
                                    error_selector_hex(
                                        error.name.as_str(),
                                        &error
                                            .parameters
                                            .iter()
                                            .map(|param| {
                                                db.canonical_type(
                                                    &inline_path,
                                                    Some(contract.name.as_str()),
                                                    &type_shape_from_ast(ctx.source, &param.ty),
                                                )
                                            })
                                            .collect::<Vec<_>>(),
                                    ),
                                );
                                if let Some(diag) =
                                    selector_diagnostic(ctx, body_item, &expected, "Error")
                                {
                                    diagnostics.push(diag);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            diagnostics
        })
        .unwrap_or_default()
    }
}

fn selector_diagnostic(
    ctx: &LintContext<'_>,
    item: &Item<'_>,
    expected_line: &str,
    kind: &str,
) -> Option<Diagnostic> {
    let span = solgrid_ast::item_name_range(item);
    let item_start = solgrid_ast::span_to_range(item.span).start;
    let block = find_attached_natspec(ctx.source, item_start);

    if let Some(block) = block {
        let lines = block.stripped_lines();
        if let Some((index, line)) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| selector_line_matches(line, kind))
        {
            if line.trim() == expected_line && block.style == NatSpecStyle::TripleSlash {
                return None;
            }

            let actual = extract_hex(line);
            let fix = replace_selector_line(&block, &lines, index, expected_line);
            let message = if let Some(actual) = actual {
                if line.trim() == expected_line {
                    format!("Non-canonical @dev {kind} selector format")
                } else {
                    format!(
                        "Incorrect {kind} selector: expected `{}`, found `{}`",
                        expected_selector_hex(expected_line),
                        actual
                    )
                }
            } else {
                format!("Non-canonical @dev {kind} selector format")
            };

            return Some(
                Diagnostic::new(META.id, message, META.default_severity, span).with_fix(fix),
            );
        }

        let mut lines = lines;
        lines.push(expected_line.to_string());
        return Some(
            Diagnostic::new(
                META.id,
                format!(
                    "Missing @dev {kind} selector tag (expected `{}`)",
                    expected_selector_hex(expected_line)
                ),
                META.default_severity,
                span,
            )
            .with_fix(Fix::safe(
                "Insert selector tag",
                vec![TextEdit::replace(
                    block.range.clone(),
                    render_triple_slash_block(&block.indent, &lines),
                )],
            )),
        );
    }

    let line_start = solgrid_ast::natspec::line_start(ctx.source, item_start);
    let line = &ctx.source[line_start..solgrid_ast::natspec::line_end(ctx.source, item_start)];
    let indent = line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect::<String>();

    Some(
        Diagnostic::new(
            META.id,
            format!(
                "Missing @dev {kind} selector tag (expected `{}`)",
                expected_selector_hex(expected_line)
            ),
            META.default_severity,
            span,
        )
        .with_fix(Fix::safe(
            "Insert selector tag",
            vec![TextEdit::insert(
                line_start,
                format!("{indent}/// {expected_line}\n"),
            )],
        )),
    )
}

fn replace_selector_line(
    block: &solgrid_ast::natspec::NatSpecBlock,
    lines: &[String],
    index: usize,
    expected_line: &str,
) -> Fix {
    let mut replacement = lines.to_vec();
    replacement[index] = expected_line.to_string();
    Fix::safe(
        "Rewrite selector tag",
        vec![TextEdit::replace(
            block.range.clone(),
            render_triple_slash_block(&block.indent, &replacement),
        )],
    )
}

fn selector_line_matches(line: &str, kind: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.contains(&format!("{} selector:", kind.to_ascii_lowercase()))
}

fn selector_tag_line(kind: &str, hex: String) -> String {
    format!("@dev {kind} selector: `{hex}`")
}

fn expected_selector_hex(expected_line: &str) -> &str {
    expected_line.split('`').nth(1).unwrap_or_default()
}

fn extract_hex(line: &str) -> Option<String> {
    let start = line.find("0x")?;
    let end = line[start..]
        .find(|ch: char| !ch.is_ascii_hexdigit() && ch != 'x')
        .map(|offset| start + offset)
        .unwrap_or(line.len());
    Some(line[start..end].to_ascii_lowercase())
}

fn error_selector_hex(name: &str, params: &[String]) -> String {
    selector_hex(selector_bytes(&format!("{name}({})", params.join(","))))
}

fn function_selector(
    source: &str,
    db: &mut TypeDatabase<'_>,
    file: &Path,
    current_contract: Option<&str>,
    func: &ItemFunction<'_>,
) -> [u8; 4] {
    let name = func
        .header
        .name
        .map(|name| name.as_str().to_string())
        .unwrap_or_default();
    let params = func
        .header
        .parameters
        .iter()
        .map(|param| {
            db.canonical_type(
                file,
                current_contract,
                &type_shape_from_ast(source, &param.ty),
            )
        })
        .collect::<Vec<_>>();
    selector_bytes(&format!("{name}({})", params.join(",")))
}

fn selector_bytes(signature: &str) -> [u8; 4] {
    let digest = Keccak256::digest(signature.as_bytes());
    [digest[0], digest[1], digest[2], digest[3]]
}

fn selector_hex(bytes: [u8; 4]) -> String {
    format!(
        "0x{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

impl<'a> TypeDatabase<'a> {
    fn canonical_type(
        &mut self,
        file: &Path,
        current_contract: Option<&str>,
        shape: &TypeShape,
    ) -> String {
        match shape {
            TypeShape::Elementary(value) | TypeShape::Raw(value) => value.clone(),
            TypeShape::Array(element, size) => match size {
                Some(size) => format!(
                    "{}[{size}]",
                    self.canonical_type(file, current_contract, element)
                ),
                None => format!("{}[]", self.canonical_type(file, current_contract, element)),
            },
            TypeShape::Custom(segments) => {
                let mut visited = HashSet::new();
                match self.resolve_type(file, current_contract, segments, &mut visited) {
                    Some(resolved) => match resolved.def.kind {
                        StoredTypeKind::Struct(fields) => format!(
                            "({})",
                            fields
                                .iter()
                                .map(|field| {
                                    self.canonical_type(
                                        &resolved.file,
                                        resolved.def.owner.as_deref(),
                                        field,
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                        StoredTypeKind::Enum => "uint8".to_string(),
                        StoredTypeKind::ContractLike => "address".to_string(),
                        StoredTypeKind::Udvt(inner) => self.canonical_type(
                            &resolved.file,
                            resolved.def.owner.as_deref(),
                            &inner,
                        ),
                    },
                    None => "address".to_string(),
                }
            }
        }
    }

    fn resolve_type(
        &mut self,
        file: &Path,
        current_contract: Option<&str>,
        segments: &[String],
        visited: &mut HashSet<PathBuf>,
    ) -> Option<ResolvedTypeDef> {
        let file = canonicalize_path(file);
        if !visited.insert(file.clone()) {
            return None;
        }

        let info = self.file_info(&file)?;

        if segments.len() == 1 {
            if let Some(owner) = current_contract {
                if let Some(def) = info.nested.get(&(owner.to_string(), segments[0].clone())) {
                    return Some(ResolvedTypeDef {
                        file,
                        def: def.clone(),
                    });
                }
            }

            if let Some(def) = info.top_level.get(&segments[0]) {
                return Some(ResolvedTypeDef {
                    file,
                    def: def.clone(),
                });
            }

            return self.resolve_imported_type(&file, &info.imports, &segments[0], visited);
        }

        if segments.len() == 2 {
            if let Some(def) = info.nested.get(&(segments[0].clone(), segments[1].clone())) {
                return Some(ResolvedTypeDef {
                    file,
                    def: def.clone(),
                });
            }

            return self.resolve_qualified_import(
                &file,
                &info.imports,
                &segments[0],
                &segments[1],
                visited,
            );
        }

        None
    }

    fn resolve_imported_type(
        &mut self,
        importing_file: &Path,
        imports: &[ImportInfo],
        name: &str,
        visited: &mut HashSet<PathBuf>,
    ) -> Option<ResolvedTypeDef> {
        for import in imports {
            let target_name = match &import.symbols {
                ImportedSymbols::Named(names) => names.iter().find_map(|(original, alias)| {
                    let local = alias.as_deref().unwrap_or(original.as_str());
                    (local == name).then_some(original.clone())
                }),
                ImportedSymbols::Plain(None) => Some(name.to_string()),
                ImportedSymbols::Plain(Some(_)) | ImportedSymbols::Glob(_) => None,
            };

            let Some(target_name) = target_name else {
                continue;
            };

            let Some(resolved_path) = self.resolver.resolve(&import.path, importing_file) else {
                continue;
            };
            let resolved_path = canonicalize_path(&resolved_path);
            if !visited.insert(resolved_path.clone()) {
                continue;
            }
            let info = self.file_info(&resolved_path)?;

            if let Some(def) = info.top_level.get(&target_name) {
                return Some(ResolvedTypeDef {
                    file: resolved_path,
                    def: def.clone(),
                });
            }

            if let Some(def) =
                self.resolve_imported_type(&resolved_path, &info.imports, &target_name, visited)
            {
                return Some(def);
            }
        }

        None
    }

    fn resolve_qualified_import(
        &mut self,
        importing_file: &Path,
        imports: &[ImportInfo],
        namespace: &str,
        name: &str,
        visited: &mut HashSet<PathBuf>,
    ) -> Option<ResolvedTypeDef> {
        for import in imports {
            let qualifies = match &import.symbols {
                ImportedSymbols::Plain(Some(alias)) => alias == namespace,
                ImportedSymbols::Glob(alias) => alias == namespace,
                ImportedSymbols::Named(names) => names.iter().any(|(original, alias)| {
                    alias.as_deref().unwrap_or(original.as_str()) == namespace
                }),
                ImportedSymbols::Plain(None) => false,
            };
            if !qualifies {
                continue;
            }

            let Some(resolved_path) = self.resolver.resolve(&import.path, importing_file) else {
                continue;
            };
            let resolved_path = canonicalize_path(&resolved_path);
            if !visited.insert(resolved_path.clone()) {
                continue;
            }
            let info = self.file_info(&resolved_path)?;

            if let Some(def) = info
                .nested
                .get(&(namespace.to_string(), name.to_string()))
                .or_else(|| info.top_level.get(name))
            {
                return Some(ResolvedTypeDef {
                    file: resolved_path,
                    def: def.clone(),
                });
            }

            if let Some(def) =
                self.resolve_imported_type(&resolved_path, &info.imports, name, visited)
            {
                return Some(def);
            }
        }

        None
    }

    fn file_info(&mut self, path: &Path) -> Option<FileTypeInfo> {
        let path = canonicalize_path(path);
        if let Some(info) = self.cache.get(&path) {
            return Some(info.clone());
        }

        let source = if path == self.inline_path {
            self.inline_source.to_string()
        } else {
            std::fs::read_to_string(&path).ok()?
        };

        let imports = build_symbol_table(&source, &path.to_string_lossy())?.imports;
        let mut info = FileTypeInfo {
            imports,
            top_level: HashMap::new(),
            nested: HashMap::new(),
        };

        with_parsed_ast_sequential(&source, &path.to_string_lossy(), |source_unit| {
            for item in source_unit.items.iter() {
                match &item.kind {
                    ItemKind::Struct(struct_def) => {
                        info.top_level.insert(
                            struct_def.name.as_str().to_string(),
                            StoredTypeDef {
                                owner: None,
                                kind: StoredTypeKind::Struct(
                                    struct_def
                                        .fields
                                        .iter()
                                        .map(|field| type_shape_from_ast(&source, &field.ty))
                                        .collect(),
                                ),
                            },
                        );
                    }
                    ItemKind::Enum(enum_def) => {
                        info.top_level.insert(
                            enum_def.name.as_str().to_string(),
                            StoredTypeDef {
                                owner: None,
                                kind: StoredTypeKind::Enum,
                            },
                        );
                    }
                    ItemKind::Udvt(udvt) => {
                        info.top_level.insert(
                            udvt.name.as_str().to_string(),
                            StoredTypeDef {
                                owner: None,
                                kind: StoredTypeKind::Udvt(type_shape_from_ast(&source, &udvt.ty)),
                            },
                        );
                    }
                    ItemKind::Contract(contract) => {
                        info.top_level.insert(
                            contract.name.as_str().to_string(),
                            StoredTypeDef {
                                owner: None,
                                kind: StoredTypeKind::ContractLike,
                            },
                        );

                        for body_item in contract.body.iter() {
                            match &body_item.kind {
                                ItemKind::Struct(struct_def) => {
                                    info.nested.insert(
                                        (
                                            contract.name.as_str().to_string(),
                                            struct_def.name.as_str().to_string(),
                                        ),
                                        StoredTypeDef {
                                            owner: Some(contract.name.as_str().to_string()),
                                            kind: StoredTypeKind::Struct(
                                                struct_def
                                                    .fields
                                                    .iter()
                                                    .map(|field| {
                                                        type_shape_from_ast(&source, &field.ty)
                                                    })
                                                    .collect(),
                                            ),
                                        },
                                    );
                                }
                                ItemKind::Enum(enum_def) => {
                                    info.nested.insert(
                                        (
                                            contract.name.as_str().to_string(),
                                            enum_def.name.as_str().to_string(),
                                        ),
                                        StoredTypeDef {
                                            owner: Some(contract.name.as_str().to_string()),
                                            kind: StoredTypeKind::Enum,
                                        },
                                    );
                                }
                                ItemKind::Udvt(udvt) => {
                                    info.nested.insert(
                                        (
                                            contract.name.as_str().to_string(),
                                            udvt.name.as_str().to_string(),
                                        ),
                                        StoredTypeDef {
                                            owner: Some(contract.name.as_str().to_string()),
                                            kind: StoredTypeKind::Udvt(type_shape_from_ast(
                                                &source, &udvt.ty,
                                            )),
                                        },
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    ItemKind::Import(import) => {
                        let _ = &import.items;
                    }
                    _ => {}
                }
            }
        })
        .ok()?;

        self.cache.insert(path.clone(), info.clone());
        Some(info)
    }
}

fn type_shape_from_ast(source: &str, ty: &Type<'_>) -> TypeShape {
    match &ty.kind {
        TypeKind::Elementary(elementary) => {
            TypeShape::Elementary(canonical_elementary_type(elementary))
        }
        TypeKind::Custom(path) => TypeShape::Custom(
            path.segments()
                .iter()
                .map(|segment| segment.as_str().to_string())
                .collect(),
        ),
        TypeKind::Array(array) => TypeShape::Array(
            Box::new(type_shape_from_ast(source, &array.element)),
            array
                .size
                .as_ref()
                .map(|size| solgrid_ast::span_text(source, size.span).trim().to_string()),
        ),
        _ => TypeShape::Raw(solgrid_ast::span_text(source, ty.span).trim().to_string()),
    }
}

fn canonical_elementary_type(elementary: &ElementaryType) -> String {
    match elementary {
        ElementaryType::Address(_) => "address".to_string(),
        ElementaryType::Bool => "bool".to_string(),
        ElementaryType::String => "string".to_string(),
        ElementaryType::Bytes => "bytes".to_string(),
        ElementaryType::Int(size) => format!("int{}", size.bits()),
        ElementaryType::UInt(size) => format!("uint{}", size.bits()),
        ElementaryType::FixedBytes(size) => format!("bytes{}", size.bytes()),
        ElementaryType::Fixed(size, frac) => format!("fixed{}x{}", size.bits(), frac.get()),
        ElementaryType::UFixed(size, frac) => format!("ufixed{}x{}", size.bits(), frac.get()),
    }
}

fn canonicalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
