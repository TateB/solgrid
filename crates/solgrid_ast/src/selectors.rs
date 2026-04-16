//! Shared selector and interface-ID helpers for editor tooling and lint rules.

use crate::resolve::ImportResolver;
use crate::symbols::{build_symbol_table, ImportInfo, ImportedSymbols};
use sha3::{Digest, Keccak256};
use solgrid_parser::solar_ast::{
    ElementaryType, FunctionKind, ItemFunction, ItemKind, Type, TypeKind, VariableDefinition,
};
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorInfo {
    pub signature: String,
    pub hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceIdInfo {
    pub hex: String,
    pub signatures: Vec<String>,
}

pub struct SelectorContext<'a> {
    source: &'a str,
    current_file: PathBuf,
    db: TypeDatabase<'a>,
}

impl<'a> SelectorContext<'a> {
    pub fn new(
        source: &'a str,
        current_file: &Path,
        resolver: &'a ImportResolver,
        get_source: &'a dyn Fn(&Path) -> Option<String>,
    ) -> Self {
        let current_file = canonicalize_path(current_file);
        Self {
            source,
            current_file: current_file.clone(),
            db: TypeDatabase::new(source, &current_file, resolver, get_source),
        }
    }

    pub fn function_selector_info(
        &mut self,
        current_contract: Option<&str>,
        func: &ItemFunction<'_>,
    ) -> Option<SelectorInfo> {
        if func.kind != FunctionKind::Function {
            return None;
        }

        let name = func.header.name?.as_str().to_string();
        let params = func
            .header
            .parameters
            .iter()
            .map(|param| {
                self.db.canonical_type(
                    &self.current_file,
                    current_contract,
                    &type_shape_from_ast(self.source, &param.ty),
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let signature = format!("{name}({})", params.join(","));
        Some(SelectorInfo {
            hex: selector_hex(selector_bytes(&signature)),
            signature,
        })
    }

    pub fn error_selector_info<'ast, I>(
        &mut self,
        current_contract: Option<&str>,
        error_name: &str,
        parameters: I,
    ) -> Option<SelectorInfo>
    where
        I: IntoIterator<Item = &'ast VariableDefinition<'ast>>,
    {
        let params = parameters
            .into_iter()
            .map(|param| {
                self.db.canonical_type(
                    &self.current_file,
                    current_contract,
                    &type_shape_from_ast(self.source, &param.ty),
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let signature = format!("{error_name}({})", params.join(","));
        Some(SelectorInfo {
            hex: selector_hex(selector_bytes(&signature)),
            signature,
        })
    }

    pub fn interface_id_info_for_items(
        &mut self,
        interface_name: &str,
        items: &[solgrid_parser::solar_ast::Item<'_>],
    ) -> Option<InterfaceIdInfo> {
        let mut interface_id = [0u8; 4];
        let mut signatures = Vec::new();

        for item in items {
            let ItemKind::Function(function) = &item.kind else {
                continue;
            };
            if function.kind != FunctionKind::Function {
                continue;
            }

            let info = self.function_selector_info(Some(interface_name), function)?;
            let selector = selector_bytes(&info.signature);
            for (byte, other) in interface_id.iter_mut().zip(selector) {
                *byte ^= other;
            }
            signatures.push(info.signature);
        }

        (!signatures.is_empty()).then(|| InterfaceIdInfo {
            hex: selector_hex(interface_id),
            signatures,
        })
    }
}

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
    resolver: &'a ImportResolver,
    get_source: &'a dyn Fn(&Path) -> Option<String>,
    cache: HashMap<PathBuf, FileTypeInfo>,
    inline_path: PathBuf,
    inline_source: &'a str,
}

impl<'a> TypeDatabase<'a> {
    fn new(
        inline_source: &'a str,
        inline_path: &Path,
        resolver: &'a ImportResolver,
        get_source: &'a dyn Fn(&Path) -> Option<String>,
    ) -> Self {
        Self {
            resolver,
            get_source,
            cache: HashMap::new(),
            inline_path: canonicalize_path(inline_path),
            inline_source,
        }
    }

    fn canonical_type(
        &mut self,
        file: &Path,
        current_contract: Option<&str>,
        shape: &TypeShape,
    ) -> Option<String> {
        match shape {
            TypeShape::Elementary(value) | TypeShape::Raw(value) => Some(value.clone()),
            TypeShape::Array(element, size) => match size {
                Some(size) => Some(format!(
                    "{}[{size}]",
                    self.canonical_type(file, current_contract, element)?
                )),
                None => Some(format!(
                    "{}[]",
                    self.canonical_type(file, current_contract, element)?
                )),
            },
            TypeShape::Custom(segments) => {
                let mut visited = HashSet::new();
                match self.resolve_type(file, current_contract, segments, &mut visited) {
                    Some(resolved) => Some(match resolved.def.kind {
                        StoredTypeKind::Struct(fields) => {
                            let fields = fields
                                .iter()
                                .map(|field| {
                                    self.canonical_type(
                                        &resolved.file,
                                        resolved.def.owner.as_deref(),
                                        field,
                                    )
                                })
                                .collect::<Option<Vec<_>>>()?;
                            format!("({})", fields.join(","))
                        }
                        StoredTypeKind::Enum => "uint8".to_string(),
                        StoredTypeKind::ContractLike => "address".to_string(),
                        StoredTypeKind::Udvt(inner) => self.canonical_type(
                            &resolved.file,
                            resolved.def.owner.as_deref(),
                            &inner,
                        )?,
                    }),
                    None => None,
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
            (self.get_source)(&path)?
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
                .map(|size| crate::span_text(source, size.span).trim().to_string()),
        ),
        _ => TypeShape::Raw(crate::span_text(source, ty.span).trim().to_string()),
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

fn canonicalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solgrid_parser::solar_ast::{ContractKind, ItemKind};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_selector_context_resolves_imported_structs_for_interface_ids() {
        let dir = tempdir().unwrap();
        let dep = dir.path().join("Types.sol");
        let main = dir.path().join("Main.sol");
        fs::write(
            &dep,
            r#"pragma solidity ^0.8.0;

struct Quote {
    address token;
    uint256 amount;
}
"#,
        )
        .unwrap();
        let source = r#"pragma solidity ^0.8.0;

import "./Types.sol";

interface IRouter {
    function quote(Quote calldata q) external returns (uint256);
}
"#;
        fs::write(&main, source).unwrap();

        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let get_source = |path: &Path| fs::read_to_string(path).ok();
        let info = with_parsed_ast_sequential(source, &main.to_string_lossy(), |source_unit| {
            let mut context = SelectorContext::new(source, &main, &resolver, &get_source);
            for item in source_unit.items.iter() {
                let ItemKind::Contract(contract) = &item.kind else {
                    continue;
                };
                if contract.kind != ContractKind::Interface {
                    continue;
                }

                return context.interface_id_info_for_items(contract.name.as_str(), contract.body);
            }

            None
        })
        .unwrap()
        .unwrap();

        assert_eq!(
            info.signatures,
            vec!["quote((address,uint256))".to_string()]
        );
        assert!(info.hex.starts_with("0x"));
    }
}
