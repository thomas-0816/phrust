//! Explicit autoload declaration metadata for multi-file include compilation.

use php_semantics::hir::{ExprId, HirExprKind, HirStmtKind, StmtId};
use php_vm::api::{
    CompilationDependencyRequest, CompilationDependencyResolver, ResolvedCompilationDependency,
    VmError,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const RESOLVER_FINGERPRINT: &str = "autoload-hir-metadata-v2";

/// Executor-owned resolver for explicit Composer and static PSR-4 metadata.
#[derive(Debug, Default)]
pub(crate) struct AutoloadCompilationResolver;

impl CompilationDependencyResolver for AutoloadCompilationResolver {
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
        for ancestor in request
            .requesting_path
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
        {
            let metadata_path = ancestor.join("autoload.php");
            if !metadata_path.is_file() {
                continue;
            }
            let Some(maps) = StaticPsr4Maps::load(&metadata_path)? else {
                continue;
            };
            if let Some(path) = maps.resolve(request.declaration) {
                return Ok(Some(ResolvedCompilationDependency {
                    path,
                    metadata_paths: vec![metadata_path],
                    activate_through_autoload: true,
                }));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct StaticPsr4Maps {
    psr4: Vec<(String, PathBuf)>,
}

impl StaticPsr4Maps {
    fn load(path: &Path) -> Result<Option<Self>, VmError> {
        let source = std::fs::read_to_string(path).map_err(|error| {
            metadata_error(path, format!("failed to read autoload metadata: {error}"))
        })?;
        let frontend = php_semantics::analyze_source(&source);
        if frontend.has_errors() {
            return Ok(None);
        }
        let module = frontend
            .database()
            .module(frontend.module().module_id())
            .ok_or_else(|| metadata_error(path, "autoload metadata has no HIR module"))?;
        if !has_static_autoload_registration(module) {
            return Ok(None);
        }

        let variables = collect_static_variables(module, path);
        let mut psr4 = Vec::new();
        for statement in module.statements().values() {
            let HirStmtKind::If {
                condition: Some(condition),
                body,
                ..
            } = statement.kind()
            else {
                continue;
            };
            let Some(prefix) = autoload_prefix(module, *condition, path, &variables) else {
                continue;
            };
            if let Some(root) = autoload_root(module, body, path, &variables) {
                psr4.push((prefix, root));
            }
        }
        psr4.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        Ok((!psr4.is_empty()).then_some(Self { psr4 }))
    }

    fn resolve(&self, declaration: &str) -> Option<PathBuf> {
        for (prefix, root) in &self.psr4 {
            let Some(relative) = strip_prefix_case_sensitive(declaration, prefix) else {
                continue;
            };
            let candidate = root.join(format!("{}.php", relative.replace('\\', "/")));
            if candidate.is_file()
                && let Ok(candidate) = std::fs::canonicalize(candidate)
            {
                return Some(candidate);
            }
        }
        None
    }
}

fn has_static_autoload_registration(module: &php_semantics::hir::HirModule) -> bool {
    module.expressions().values().any(|expression| {
        let HirExprKind::Call {
            callee: Some(callee),
            args,
        } = expression.kind()
        else {
            return false;
        };
        is_named_call(module, *callee, "spl_autoload_register")
            && args.first().is_some_and(|argument| {
                module
                    .expressions()
                    .get(argument.value)
                    .is_some_and(|argument| matches!(argument.kind(), HirExprKind::Closure { .. }))
            })
    })
}

fn collect_static_variables(
    module: &php_semantics::hir::HirModule,
    path: &Path,
) -> HashMap<String, MetadataValue> {
    let mut variables = HashMap::new();
    for _ in 0..module.expressions().len() {
        let previous_len = variables.len();
        for expression in module.expressions().values() {
            let HirExprKind::Assign {
                operator,
                left: Some(left),
                right: Some(right),
            } = expression.kind()
            else {
                continue;
            };
            if operator != "=" {
                continue;
            }
            let Some(HirExprKind::Variable { name, .. }) =
                module.expressions().get(*left).map(|left| left.kind())
            else {
                continue;
            };
            if let Ok(value) = evaluate_expr(module, *right, path, &variables) {
                variables.insert(name.clone(), value);
            }
        }
        if variables.len() == previous_len {
            break;
        }
    }
    variables
}

fn autoload_prefix(
    module: &php_semantics::hir::HirModule,
    condition: ExprId,
    path: &Path,
    variables: &HashMap<String, MetadataValue>,
) -> Option<String> {
    let condition_expr = module.expressions().get(condition)?;
    let call = match condition_expr.kind() {
        HirExprKind::Binary {
            operator,
            left: Some(left),
            right: Some(right),
        } if matches!(operator.as_str(), "==" | "===") => {
            if is_zero_literal(module, *left) {
                *right
            } else if is_zero_literal(module, *right) {
                *left
            } else {
                return None;
            }
        }
        HirExprKind::Call { .. } => condition,
        _ => return None,
    };
    let HirExprKind::Call {
        callee: Some(callee),
        args,
    } = module.expressions().get(call)?.kind()
    else {
        return None;
    };
    let prefix = if is_named_call(module, *callee, "strncmp") && args.len() >= 2
        || is_named_call(module, *callee, "str_starts_with") && args.len() == 2
    {
        args[1].value
    } else {
        return None;
    };
    match evaluate_expr(module, prefix, path, variables).ok()? {
        MetadataValue::String(prefix) if prefix.ends_with('\\') => Some(prefix),
        _ => None,
    }
}

fn is_zero_literal(module: &php_semantics::hir::HirModule, expr: ExprId) -> bool {
    matches!(
        module.expressions().get(expr).map(|expression| expression.kind()),
        Some(HirExprKind::Literal { text }) if text == "0"
    )
}

fn is_named_call(module: &php_semantics::hir::HirModule, callee: ExprId, name: &str) -> bool {
    matches!(
        module.expressions().get(callee).map(|expression| expression.kind()),
        Some(HirExprKind::Name { resolution }) if resolution.source().eq_ignore_ascii_case(name)
    )
}

#[derive(Debug)]
struct PathTemplate {
    before: String,
    has_relative_class: bool,
    after: String,
}

fn autoload_root(
    module: &php_semantics::hir::HirModule,
    body: &[StmtId],
    path: &Path,
    variables: &HashMap<String, MetadataValue>,
) -> Option<PathBuf> {
    let mut expressions = Vec::new();
    collect_statement_expressions(module, body, &mut expressions);
    let included_variables = expressions
        .iter()
        .filter_map(|expr| module.expressions().get(*expr))
        .filter_map(|expression| match expression.kind() {
            HirExprKind::Include {
                expr: Some(expr), ..
            } => variable_name(module, *expr),
            _ => None,
        })
        .collect::<Vec<_>>();
    for expr in expressions {
        let HirExprKind::Assign {
            operator,
            left: Some(left),
            right: Some(right),
        } = module.expressions().get(expr)?.kind()
        else {
            continue;
        };
        if operator != "=" {
            continue;
        }
        let Some(variable) = variable_name(module, *left) else {
            continue;
        };
        if !included_variables.contains(&variable) {
            continue;
        }
        let template = path_template(module, *right, path, variables)?;
        if template.has_relative_class && template.after == ".php" {
            return Some(PathBuf::from(template.before));
        }
    }
    None
}

fn collect_statement_expressions(
    module: &php_semantics::hir::HirModule,
    statements: &[StmtId],
    output: &mut Vec<ExprId>,
) {
    for statement in statements {
        let Some(statement) = module.statements().get(*statement) else {
            continue;
        };
        match statement.kind() {
            HirStmtKind::Expr { expr: Some(expr) } => output.push(*expr),
            HirStmtKind::Block { statements } => {
                collect_statement_expressions(module, statements, output);
            }
            HirStmtKind::If {
                body,
                elseifs,
                else_body,
                ..
            } => {
                collect_statement_expressions(module, body, output);
                for branch in elseifs {
                    collect_statement_expressions(module, &branch.body, output);
                }
                collect_statement_expressions(module, else_body, output);
            }
            _ => {}
        }
    }
}

fn variable_name(module: &php_semantics::hir::HirModule, expr: ExprId) -> Option<&String> {
    match module.expressions().get(expr)?.kind() {
        HirExprKind::Variable { name, .. } => Some(name),
        _ => None,
    }
}

fn path_template(
    module: &php_semantics::hir::HirModule,
    expr: ExprId,
    path: &Path,
    variables: &HashMap<String, MetadataValue>,
) -> Option<PathTemplate> {
    let expression = module.expressions().get(expr)?;
    match expression.kind() {
        HirExprKind::Binary {
            operator,
            left: Some(left),
            right: Some(right),
        } if operator == "." => {
            let left = path_template(module, *left, path, variables)?;
            let right = path_template(module, *right, path, variables)?;
            if left.has_relative_class && right.has_relative_class {
                return None;
            }
            if left.has_relative_class {
                return Some(PathTemplate {
                    before: left.before,
                    has_relative_class: true,
                    after: left.after + &right.before + &right.after,
                });
            }
            if right.has_relative_class {
                return Some(PathTemplate {
                    before: left.before + &left.after + &right.before,
                    has_relative_class: true,
                    after: right.after,
                });
            }
            Some(PathTemplate {
                before: left.before + &left.after + &right.before + &right.after,
                has_relative_class: false,
                after: String::new(),
            })
        }
        HirExprKind::Call {
            callee: Some(callee),
            args,
        } if is_named_call(module, *callee, "str_replace") && args.len() == 3 => {
            let search = expect_string(
                evaluate_expr(module, args[0].value, path, variables).ok()?,
                path,
            )
            .ok()?;
            let replacement = expect_string(
                evaluate_expr(module, args[1].value, path, variables).ok()?,
                path,
            )
            .ok()?;
            (search == "\\" && replacement == "/").then_some(PathTemplate {
                before: String::new(),
                has_relative_class: true,
                after: String::new(),
            })
        }
        _ => match evaluate_expr(module, expr, path, variables).ok()? {
            MetadataValue::String(value) => Some(PathTemplate {
                before: value,
                has_relative_class: false,
                after: String::new(),
            }),
            MetadataValue::Array(_) => None,
        },
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
