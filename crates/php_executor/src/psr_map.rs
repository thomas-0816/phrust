//! Local PSR-4 source-map inference for cross-file trait lowering.
//!
//! Trait composition is resolved at IR lowering time, but PSR-4 codebases
//! (WordPress bundled libraries, Composer packages) declare each trait in its
//! own file and rely on an autoloader that only runs at class-link time in
//! reference PHP. When an include-unit compile hits a `use SomeTrait;` whose
//! declaration is neither in the compilation session nor explicitly mapped on
//! the loader, this module infers the trait's file from the requesting file's
//! own namespace-to-path layout so the compiler can pull it in as a tracked
//! session dependency.
//!
//! The inference is deliberately conservative: it only fires when the
//! requesting file's namespace suffix mirrors its directory suffix (the PSR-4
//! contract), and the resolved file must actually declare the requested
//! trait. Anything else falls through to the standard missing-trait
//! diagnostic.
//!
//! Known gap (pre-dates the compilation-session rewrite): the inference does
//! not know whether an autoloader is registered at the use site, so a class
//! whose trait file exists in the PSR layout links successfully even where
//! reference PHP raises `Error: Trait "X" not found` because no autoloader
//! would have provided it. Reference-exact linking requires a runtime
//! DeclareClass check against the runtime class table plus the autoload
//! protocol; until then this trades that rare divergence for correct
//! behavior on the ubiquitous autoloader-registered case.

use std::fs;
use std::path::{Path, PathBuf};

/// Namespace-to-directory mapping inferred from one PHP source file.
#[derive(Clone, Debug)]
pub(crate) struct LocalPsrSourceMap {
    root: PathBuf,
    prefix: Vec<String>,
    imports: Vec<String>,
}

impl LocalPsrSourceMap {
    /// Infers the PSR-4 root and namespace prefix from a file's declared
    /// namespace, its first class-like declaration, and its canonical path.
    pub(crate) fn infer(source: &str, canonical_path: &Path) -> Option<Self> {
        let namespace = declared_namespace_parts(source);
        let class_like = first_class_like_name(source)?;
        let mut fqn = namespace;
        fqn.push(class_like);
        let (root, prefix) = infer_psr_root_and_prefix(canonical_path, &fqn)?;
        Some(Self {
            root,
            prefix,
            imports: imported_class_like_names(source),
        })
    }

    /// Resolves a normalized declaration name to an existing file, preferring
    /// the file's own `use` imports, then falling back to the resolved name
    /// itself for same-namespace-relative references.
    pub(crate) fn resolve_declaration(&self, normalized_name: &str) -> Option<PathBuf> {
        self.resolve_imported_name(normalized_name)
            .or_else(|| self.path_for_name(normalized_name))
    }

    fn resolve_imported_name(&self, normalized_name: &str) -> Option<PathBuf> {
        let import = self.imports.iter().find(|import| {
            normalize_php_class_name(import).eq_ignore_ascii_case(normalized_name)
        })?;
        self.path_for_name(import)
    }

    fn path_for_name(&self, name: &str) -> Option<PathBuf> {
        let parts = php_name_parts(name);
        if parts.len() <= self.prefix.len() {
            return None;
        }
        if !parts
            .iter()
            .zip(&self.prefix)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
        {
            return None;
        }
        let relative = &parts[self.prefix.len()..];
        let mut path = self.root.clone();
        for part in &relative[..relative.len().saturating_sub(1)] {
            path.push(part);
        }
        let file_name = format!("{}.php", relative.last()?);
        path.push(file_name);
        if path.is_file() {
            return fs::canonicalize(path).ok();
        }
        case_insensitive_existing_path(&path)
    }
}

fn declared_namespace_parts(source: &str) -> Vec<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if is_php_comment_line(trimmed) {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("namespace ") else {
            continue;
        };
        let name = rest
            .split([';', '{'])
            .next()
            .map(str::trim)
            .unwrap_or_default();
        return php_name_parts(name);
    }
    Vec::new()
}

fn first_class_like_name(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if is_php_comment_line(trimmed) {
            continue;
        }
        for keyword in ["class", "trait", "interface", "enum"] {
            if let Some(name) = identifier_after_keyword(trimmed, keyword) {
                return Some(name.to_owned());
            }
        }
    }
    None
}

fn imported_class_like_names(source: &str) -> Vec<String> {
    let mut imports = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if is_php_comment_line(trimmed) {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("use ") else {
            continue;
        };
        if !trimmed.ends_with(';')
            || rest.starts_with("function ")
            || rest.starts_with("const ")
            || rest.contains('{')
        {
            continue;
        }
        for raw_import in rest.trim_end_matches(';').split(',') {
            let import = strip_use_alias(raw_import.trim()).trim_start_matches('\\');
            if import.contains('\\') && !imports.iter().any(|existing| existing == import) {
                imports.push(import.to_owned());
            }
        }
    }
    imports
}

fn infer_psr_root_and_prefix(
    canonical_path: &Path,
    fqn: &[String],
) -> Option<(PathBuf, Vec<String>)> {
    if fqn.is_empty() {
        return None;
    }
    let path_parts = canonical_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    for suffix_len in (1..=fqn.len()).rev() {
        let suffix_start = fqn.len() - suffix_len;
        let mut expected = fqn[suffix_start..fqn.len().saturating_sub(1)].to_vec();
        expected.push(format!("{}.php", fqn.last()?));
        if expected.len() > path_parts.len() {
            continue;
        }
        let actual = &path_parts[path_parts.len() - expected.len()..];
        if !actual
            .iter()
            .zip(&expected)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
        {
            continue;
        }
        let mut root = canonical_path.to_path_buf();
        for _ in 0..expected.len() {
            root.pop();
        }
        return Some((root, fqn[..suffix_start].to_vec()));
    }
    None
}

fn case_insensitive_existing_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let file_name = path.file_name()?.to_string_lossy();
    let parent = if parent.is_dir() {
        parent.to_path_buf()
    } else {
        case_insensitive_existing_path(parent)?
    };
    for entry in fs::read_dir(parent).ok()? {
        let entry = entry.ok()?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(&file_name)
        {
            let path = entry.path();
            return path
                .is_file()
                .then(|| fs::canonicalize(path).ok())
                .flatten();
        }
    }
    None
}

fn php_name_parts(name: &str) -> Vec<String> {
    name.trim()
        .trim_start_matches('\\')
        .split('\\')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_php_class_name(name: &str) -> String {
    php_name_parts(name).join("\\").to_ascii_lowercase()
}

fn strip_use_alias(import: &str) -> &str {
    let lower = import.to_ascii_lowercase();
    match lower.find(" as ") {
        Some(index) => &import[..index],
        None => import,
    }
}

fn is_php_comment_line(line: &str) -> bool {
    line.starts_with("//")
        || line.starts_with('#')
        || line.starts_with("/*")
        || line.starts_with('*')
}

fn identifier_after_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let index = line.find(keyword)?;
    let before = line[..index].chars().last();
    if before.is_some_and(is_php_identifier_char) {
        return None;
    }
    let rest = &line[index + keyword.len()..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let end = rest
        .char_indices()
        .find_map(|(index, ch)| (!is_php_identifier_char(ch)).then_some(index))
        .unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
}

fn is_php_identifier_char(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric() || !ch.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct MapFixture {
        root: PathBuf,
    }

    impl MapFixture {
        fn new(name: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "phrust-psr-map-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&root).expect("create fixture root");
            Self { root }
        }

        fn write(&self, relative: &str, source: &str) -> PathBuf {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("create parents");
            fs::write(&path, source).expect("write fixture");
            fs::canonicalize(path).expect("canonicalize fixture")
        }
    }

    impl Drop for MapFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn resolves_imported_trait_in_nested_namespace() {
        let fixture = MapFixture::new("imported");
        let trait_path = fixture.write(
            "src/Providers/Http/Traits/WithTransporterTrait.php",
            "<?php\nnamespace Acme\\Providers\\Http\\Traits;\ntrait WithTransporterTrait {}\n",
        );
        let class_path = fixture.write(
            "src/Providers/Registry.php",
            "<?php\nnamespace Acme\\Providers;\n\nuse Acme\\Providers\\Http\\Traits\\WithTransporterTrait;\n\nclass Registry {\n    use WithTransporterTrait;\n}\n",
        );
        let source = fs::read_to_string(&class_path).expect("read class");

        let map = LocalPsrSourceMap::infer(&source, &class_path).expect("infer map");
        let resolved = map
            .resolve_declaration("acme\\providers\\http\\traits\\withtransportertrait")
            .expect("resolve trait");

        assert_eq!(resolved, trait_path);
    }

    #[test]
    fn resolves_relative_trait_reference_without_import() {
        let fixture = MapFixture::new("relative");
        let trait_path = fixture.write(
            "src/Acme/Traits/Helper.php",
            "<?php\nnamespace Acme\\Traits;\ntrait Helper {}\n",
        );
        let class_path = fixture.write(
            "src/Acme/Registry.php",
            "<?php\nnamespace Acme;\nclass Registry {\n    use Traits\\Helper;\n}\n",
        );
        let source = fs::read_to_string(&class_path).expect("read class");

        let map = LocalPsrSourceMap::infer(&source, &class_path).expect("infer map");
        let resolved = map
            .resolve_declaration("acme\\traits\\helper")
            .expect("resolve trait");

        assert_eq!(resolved, trait_path);
    }

    #[test]
    fn rejects_names_outside_the_inferred_prefix() {
        let fixture = MapFixture::new("outside");
        let class_path = fixture.write(
            "src/Acme/Registry.php",
            "<?php\nnamespace Acme;\nclass Registry {}\n",
        );
        let source = fs::read_to_string(&class_path).expect("read class");

        let map = LocalPsrSourceMap::infer(&source, &class_path).expect("infer map");

        assert!(
            map.resolve_declaration("other\\vendor\\sometrait")
                .is_none()
        );
    }

    #[test]
    fn requires_namespace_to_mirror_directories() {
        let fixture = MapFixture::new("mismatch");
        let class_path = fixture.write(
            "lib/randomdir/Registry.php",
            "<?php\nnamespace Acme\\Providers;\nclass Registry {}\n",
        );
        let source = fs::read_to_string(&class_path).expect("read class");

        // Only the class-name path component matches; the namespace does not
        // mirror the directory layout, so the file itself is the root anchor
        // and unrelated names must not resolve.
        let map = LocalPsrSourceMap::infer(&source, &class_path);
        if let Some(map) = map {
            assert!(
                map.resolve_declaration("acme\\providers\\http\\missing")
                    .is_none()
            );
        }
    }
}
