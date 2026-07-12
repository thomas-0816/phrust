//! Composer declaration metadata for multi-file include compilation.

use php_semantics::hir::{ExprId, HirExprKind, HirStmtKind};
use php_vm::api::{
    CompilationDependencyRequest, CompilationDependencyResolver, ResolvedCompilationDependency,
    VmError,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RESOLVER_FINGERPRINT: &str = "composer-hir-metadata-v1";

/// Executor-owned resolver for Composer's generated classmap and PSR-4 maps.
#[derive(Debug, Default)]
pub(crate) struct ComposerCompilationResolver;

impl CompilationDependencyResolver for ComposerCompilationResolver {
    fn fingerprint(&self) -> String {
        RESOLVER_FINGERPRINT.to_owned()
    }

    fn resolve(
        &self,
        request: CompilationDependencyRequest<'_>,
    ) -> Result<Option<ResolvedCompilationDependency>, VmError> {
        for ancestor in request
            .requesting_path
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
        {
            let composer_dir = ancestor.join("vendor").join("composer");
            if !composer_dir.is_dir() {
                continue;
            }
            let maps = ComposerMaps::load(&composer_dir)?;
            if let Some(path) = maps.resolve(request.declaration) {
                return Ok(Some(ResolvedCompilationDependency {
                    path,
                    metadata_paths: maps.metadata_paths,
                    activate_through_autoload: true,
                }));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct ComposerMaps {
    classmap: HashMap<String, PathBuf>,
    psr4: Vec<(String, Vec<PathBuf>)>,
    metadata_paths: Vec<PathBuf>,
}

impl ComposerMaps {
    fn load(composer_dir: &Path) -> Result<Self, VmError> {
        let mut maps = Self::default();
        let classmap_path = composer_dir.join("autoload_classmap.php");
        if classmap_path.is_file() {
            let value = evaluate_metadata_file(&classmap_path)?;
            maps.classmap = string_map(value, &classmap_path)?
                .into_iter()
                .map(|(name, paths)| {
                    let path = paths.into_iter().next().ok_or_else(|| {
                        metadata_error(
                            &classmap_path,
                            format!("classmap declaration `{name}` has no path"),
                        )
                    })?;
                    Ok((normalize_declaration(&name), path))
                })
                .collect::<Result<_, VmError>>()?;
            maps.metadata_paths.push(classmap_path);
        }

        let psr4_path = composer_dir.join("autoload_psr4.php");
        if psr4_path.is_file() {
            let value = evaluate_metadata_file(&psr4_path)?;
            maps.psr4 = string_map(value, &psr4_path)?;
            maps.psr4
                .sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
            maps.metadata_paths.push(psr4_path);
        }
        Ok(maps)
    }

    fn resolve(&self, declaration: &str) -> Option<PathBuf> {
        if let Some(path) = self.classmap.get(&normalize_declaration(declaration))
            && path.is_file()
            && let Ok(path) = std::fs::canonicalize(path)
        {
            return Some(path);
        }
        for (prefix, roots) in &self.psr4 {
            let Some(relative) = strip_prefix_case_sensitive(declaration, prefix) else {
                continue;
            };
            let relative = format!("{}.php", relative.replace('\\', "/"));
            for root in roots {
                let candidate = root.join(&relative);
                if candidate.is_file()
                    && let Ok(candidate) = std::fs::canonicalize(candidate)
                {
                    return Some(candidate);
                }
            }
        }
        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MetadataValue {
    String(String),
    Array(Vec<(Option<MetadataValue>, MetadataValue)>),
}

fn evaluate_metadata_file(path: &Path) -> Result<MetadataValue, VmError> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        metadata_error(path, format!("failed to read Composer metadata: {error}"))
    })?;
    let frontend = php_semantics::analyze_source(&source);
    if frontend.has_errors() {
        return Err(metadata_error(
            path,
            "Composer metadata failed frontend analysis",
        ));
    }
    let module = frontend
        .database()
        .module(frontend.module().module_id())
        .ok_or_else(|| metadata_error(path, "Composer metadata has no HIR module"))?;
    let mut variables = HashMap::new();
    for statement in module.statements().values() {
        match statement.kind() {
            HirStmtKind::Expr { expr: Some(expr) } => {
                evaluate_assignment(module, *expr, path, &mut variables)?;
            }
            HirStmtKind::Return { expr: Some(expr) } => {
                return evaluate_expr(module, *expr, path, &variables);
            }
            _ => {}
        }
    }
    Err(metadata_error(
        path,
        "Composer metadata does not return an array",
    ))
}

fn evaluate_assignment(
    module: &php_semantics::hir::HirModule,
    expr: ExprId,
    path: &Path,
    variables: &mut HashMap<String, MetadataValue>,
) -> Result<(), VmError> {
    let Some(expression) = module.expressions().get(expr) else {
        return Ok(());
    };
    let HirExprKind::Assign {
        operator,
        left: Some(left),
        right: Some(right),
    } = expression.kind()
    else {
        return Ok(());
    };
    if operator != "=" {
        return Ok(());
    }
    let Some(left) = module.expressions().get(*left) else {
        return Ok(());
    };
    let HirExprKind::Variable { name, .. } = left.kind() else {
        return Ok(());
    };
    let value = evaluate_expr(module, *right, path, variables)?;
    variables.insert(name.clone(), value);
    Ok(())
}

fn evaluate_expr(
    module: &php_semantics::hir::HirModule,
    expr: ExprId,
    path: &Path,
    variables: &HashMap<String, MetadataValue>,
) -> Result<MetadataValue, VmError> {
    let expression = module
        .expressions()
        .get(expr)
        .ok_or_else(|| metadata_error(path, "Composer metadata expression is missing"))?;
    match expression.kind() {
        HirExprKind::Literal { text } if text == "__DIR__" => path
            .parent()
            .map(|directory| MetadataValue::String(directory.to_string_lossy().into_owned()))
            .ok_or_else(|| metadata_error(path, "Composer metadata path has no parent")),
        HirExprKind::Literal { text } => parse_literal(text)
            .map(MetadataValue::String)
            .ok_or_else(|| metadata_error(path, format!("unsupported literal `{text}`"))),
        HirExprKind::Name { resolution } if resolution.source() == "__DIR__" => path
            .parent()
            .map(|directory| MetadataValue::String(directory.to_string_lossy().into_owned()))
            .ok_or_else(|| metadata_error(path, "Composer metadata path has no parent")),
        HirExprKind::Variable { name, .. } => variables.get(name).cloned().ok_or_else(|| {
            metadata_error(path, format!("unknown Composer metadata variable `{name}`"))
        }),
        HirExprKind::Binary {
            operator,
            left: Some(left),
            right: Some(right),
        } if operator == "." => {
            let left = expect_string(evaluate_expr(module, *left, path, variables)?, path)?;
            let right = expect_string(evaluate_expr(module, *right, path, variables)?, path)?;
            Ok(MetadataValue::String(left + &right))
        }
        HirExprKind::Array { elements } => {
            let mut values = Vec::with_capacity(elements.len());
            for element in elements {
                let element = module.expressions().get(*element).ok_or_else(|| {
                    metadata_error(path, "Composer metadata array element is missing")
                })?;
                let HirExprKind::ArrayPair { key, value, .. } = element.kind() else {
                    return Err(metadata_error(
                        path,
                        "Composer metadata array contains a non-pair element",
                    ));
                };
                let key = key
                    .map(|key| evaluate_expr(module, key, path, variables))
                    .transpose()?;
                let value = value
                    .map(|value| evaluate_expr(module, value, path, variables))
                    .transpose()?
                    .ok_or_else(|| metadata_error(path, "Composer metadata array value missing"))?;
                values.push((key, value));
            }
            Ok(MetadataValue::Array(values))
        }
        HirExprKind::Call { callee, args } if args.len() <= 2 => {
            let Some(callee) = callee.and_then(|callee| module.expressions().get(callee)) else {
                return Err(metadata_error(
                    path,
                    "Composer metadata call target missing",
                ));
            };
            let HirExprKind::Name { resolution } = callee.kind() else {
                return Err(metadata_error(path, "unsupported Composer metadata call"));
            };
            if !resolution.source().eq_ignore_ascii_case("dirname") || args.is_empty() {
                return Err(metadata_error(path, "unsupported Composer metadata call"));
            }
            let mut value = PathBuf::from(expect_string(
                evaluate_expr(module, args[0].value, path, variables)?,
                path,
            )?);
            let levels = if let Some(levels) = args.get(1) {
                expect_string(evaluate_expr(module, levels.value, path, variables)?, path)?
                    .parse::<usize>()
                    .map_err(|_| metadata_error(path, "invalid dirname level"))?
            } else {
                1
            };
            for _ in 0..levels {
                value = value
                    .parent()
                    .ok_or_else(|| metadata_error(path, "dirname escaped filesystem root"))?
                    .to_path_buf();
            }
            Ok(MetadataValue::String(value.to_string_lossy().into_owned()))
        }
        _ => Err(metadata_error(
            path,
            "unsupported Composer metadata expression",
        )),
    }
}

fn string_map(value: MetadataValue, path: &Path) -> Result<Vec<(String, Vec<PathBuf>)>, VmError> {
    let MetadataValue::Array(entries) = value else {
        return Err(metadata_error(
            path,
            "Composer metadata must return an array",
        ));
    };
    entries
        .into_iter()
        .map(|(key, value)| {
            let key = key
                .map(|key| expect_string(key, path))
                .transpose()?
                .ok_or_else(|| metadata_error(path, "Composer metadata key is missing"))?;
            let paths = match value {
                MetadataValue::String(value) => vec![PathBuf::from(value)],
                MetadataValue::Array(values) => values
                    .into_iter()
                    .map(|(_, value)| expect_string(value, path).map(PathBuf::from))
                    .collect::<Result<Vec<_>, _>>()?,
            };
            Ok((key, paths))
        })
        .collect()
}

fn expect_string(value: MetadataValue, path: &Path) -> Result<String, VmError> {
    let MetadataValue::String(value) = value else {
        return Err(metadata_error(path, "expected string metadata value"));
    };
    Ok(value)
}

fn parse_literal(text: &str) -> Option<String> {
    let text = text.trim();
    let quote = text.as_bytes().first().copied()?;
    if !matches!(quote, b'\'' | b'"') || text.as_bytes().last().copied()? != quote {
        return text
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| text.to_owned());
    }
    let mut output = String::new();
    let mut chars = text[1..text.len() - 1].chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = chars.next()?;
        match (quote, escaped) {
            (b'\'', '\\' | '\'') => output.push(escaped),
            (b'\'', _) => {
                output.push('\\');
                output.push(escaped);
            }
            (b'"', 'n') => output.push('\n'),
            (b'"', 'r') => output.push('\r'),
            (b'"', 't') => output.push('\t'),
            (b'"', '\\' | '"') => output.push(escaped),
            (b'"', _) => {
                output.push('\\');
                output.push(escaped);
            }
            _ => return None,
        }
    }
    Some(output)
}

fn strip_prefix_case_sensitive<'a>(declaration: &'a str, prefix: &str) -> Option<&'a str> {
    declaration.strip_prefix(prefix)
}

fn normalize_declaration(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

fn metadata_error(path: &Path, message: impl Into<String>) -> VmError {
    VmError::fatal(
        "E_PHP_EXECUTOR_COMPOSER_METADATA",
        "include_compile",
        message,
    )
    .with_context("path", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "phrust-composer-metadata-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create fixture");
        root
    }

    #[test]
    fn evaluates_standard_composer_variables_and_arrays() {
        let root = fixture("evaluate");
        let composer = root.join("vendor/composer");
        fs::create_dir_all(&composer).expect("composer dir");
        let map = composer.join("autoload_psr4.php");
        fs::write(
            &map,
            "<?php\n$vendorDir = dirname(__DIR__);\n$baseDir = dirname($vendorDir);\nreturn array('Acme\\\\' => array($baseDir . '/src'));\n",
        )
        .expect("write map");

        let value = evaluate_metadata_file(&map).expect("evaluate map");
        assert_eq!(
            string_map(value, &map).expect("string map"),
            vec![("Acme\\".to_owned(), vec![root.join("src")])]
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn resolves_classmap_before_longest_psr4_prefix() {
        let root = fixture("resolve");
        let composer = root.join("vendor/composer");
        fs::create_dir_all(&composer).expect("composer dir");
        fs::create_dir_all(root.join("src/Deep")).expect("source dir");
        fs::create_dir_all(root.join("classmap")).expect("classmap dir");
        fs::write(
            root.join("src/Deep/TraitName.php"),
            "<?php trait PsrTraitName {}",
        )
        .expect("psr trait file");
        fs::write(
            root.join("src/Deep/OtherTrait.php"),
            "<?php trait OtherTrait {}",
        )
        .expect("longest-prefix trait file");
        fs::write(
            root.join("classmap/TraitName.php"),
            "<?php trait TraitName {}",
        )
        .expect("classmap trait file");
        fs::write(
            composer.join("autoload_psr4.php"),
            "<?php return ['Acme\\\\' => [__DIR__ . '/../../src'], 'Acme\\\\Deep\\\\' => [__DIR__ . '/../../src/Deep']];",
        )
        .expect("psr map");
        fs::write(
            composer.join("autoload_classmap.php"),
            "<?php return ['Acme\\\\Deep\\\\TraitName' => __DIR__ . '/../../classmap/TraitName.php'];",
        )
        .expect("classmap");

        let maps = ComposerMaps::load(&composer).expect("load maps");
        assert_eq!(
            maps.resolve("Acme\\Deep\\TraitName"),
            Some(
                fs::canonicalize(root.join("classmap/TraitName.php"))
                    .expect("canonical trait path")
            )
        );
        assert_eq!(
            maps.resolve("Acme\\Deep\\OtherTrait"),
            Some(
                fs::canonicalize(root.join("src/Deep/OtherTrait.php"))
                    .expect("canonical longest-prefix trait path")
            )
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
