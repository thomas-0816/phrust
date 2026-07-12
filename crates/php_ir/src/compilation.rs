//! Typed multi-file compilation-session inputs.

use std::collections::{HashMap, HashSet};

use php_semantics::FrontendResult;
use php_semantics::hir::{ClassLikeKind, ClassLikeMemberId, DeclareValue, NameKind};

use crate::module::normalize_class_name;

/// Stable file identity inside one compilation session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilationFileId(u32);

impl CompilationFileId {
    /// Returns the stable zero-based session index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Immutable source owned by a compilation session.
#[derive(Clone, Debug)]
pub struct CompilationSource {
    id: CompilationFileId,
    path: String,
    source: String,
    frontend: FrontendResult,
    strict_types: bool,
    namespaces: Vec<String>,
}

impl CompilationSource {
    fn new(id: CompilationFileId, path: String, source: String) -> Self {
        let frontend = php_semantics::analyze_source(&source);
        let (strict_types, namespaces) = frontend
            .database()
            .module(frontend.module().module_id())
            .map(|module| {
                let strict_types = module
                    .file_directives()
                    .strict_types()
                    .is_some_and(|directive| matches!(directive.value(), DeclareValue::Int(1)));
                let namespaces = module
                    .namespaces()
                    .values()
                    .map(|namespace| {
                        namespace
                            .name()
                            .map_or_else(String::new, |name| name.text().to_owned())
                    })
                    .collect();
                (strict_types, namespaces)
            })
            .unwrap_or_default();
        Self {
            id,
            path,
            source,
            frontend,
            strict_types,
            namespaces,
        }
    }

    /// Stable session file ID.
    #[must_use]
    pub const fn id(&self) -> CompilationFileId {
        self.id
    }

    /// Original display path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Exact immutable source bytes interpreted as UTF-8.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Per-file semantic frontend result.
    #[must_use]
    pub const fn frontend(&self) -> &FrontendResult {
        &self.frontend
    }

    /// File-level `declare(strict_types=1)` mode.
    #[must_use]
    pub const fn strict_types(&self) -> bool {
        self.strict_types
    }

    /// Lexical namespace contexts in source order; the global namespace is empty.
    #[must_use]
    pub fn namespaces(&self) -> &[String] {
        &self.namespaces
    }
}

/// Typed unresolved declaration request presented to an external resolver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedTraitRequest {
    /// File containing the trait use.
    pub requesting_file: CompilationFileId,
    /// Normalized requested trait name.
    pub normalized_name: String,
    /// Resolved declaration name preserving PHP-visible source casing.
    pub resolved_name: String,
    /// PHP-visible spelling at the use site.
    pub display_name: String,
    /// Normalized owner class-like name.
    pub owner_name: String,
}

/// One explicit declaration dependency edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationDependency {
    /// File containing the unresolved declaration use.
    pub requester: CompilationFileId,
    /// File supplying the declaration.
    pub dependency: CompilationFileId,
    /// Normalized declaration name that established the edge.
    pub declaration: String,
}

/// Deterministic dependency cycle represented by its declaration edges.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationCycle {
    /// Edges in traversal order, including the edge that closes the cycle.
    pub edges: Vec<CompilationDependency>,
}

/// Multi-file compiler input with stable file IDs and an explicit graph.
#[derive(Clone, Debug)]
pub struct CompilationSession {
    files: Vec<CompilationSource>,
    paths: HashMap<String, CompilationFileId>,
    dependencies: Vec<CompilationDependency>,
    entry: CompilationFileId,
    /// Normalized declaration names that explicit resolver metadata requires
    /// the runtime autoload protocol to activate, keyed by file.
    autoload_declarations: HashMap<CompilationFileId, String>,
}

impl CompilationSession {
    /// Creates a session whose first source is the executable entry file.
    #[must_use]
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        let path = path.into();
        let entry = CompilationFileId(0);
        let file = CompilationSource::new(entry, path.clone(), source.into());
        Self {
            files: vec![file],
            paths: HashMap::from([(path, entry)]),
            dependencies: Vec::new(),
            entry,
            autoload_declarations: HashMap::new(),
        }
    }

    /// Adds or reuses an immutable dependency source and records its edge.
    pub fn add_dependency(
        &mut self,
        requester: CompilationFileId,
        declaration: impl Into<String>,
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> CompilationFileId {
        let path = path.into();
        let dependency = if let Some(id) = self.paths.get(&path).copied() {
            id
        } else {
            let id = CompilationFileId(self.files.len() as u32);
            self.files
                .push(CompilationSource::new(id, path.clone(), source.into()));
            self.paths.insert(path, id);
            id
        };
        let edge = CompilationDependency {
            requester,
            dependency,
            declaration: normalize_class_name(&declaration.into()),
        };
        if !self.dependencies.contains(&edge) {
            self.dependencies.push(edge);
        }
        dependency
    }

    /// Adds an explicitly resolved autoload dependency and records the
    /// normalized declaration used to activate it through the runtime autoload
    /// protocol.
    pub fn add_autoload_dependency(
        &mut self,
        requester: CompilationFileId,
        declaration: impl Into<String>,
        display_name: impl Into<String>,
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> CompilationFileId {
        let dependency = self.add_dependency(requester, declaration, path, source);
        self.autoload_declarations
            .entry(dependency)
            .or_insert_with(|| display_name.into());
        dependency
    }

    /// Returns the normalized declaration used to activate `file` through the
    /// runtime autoload protocol.
    #[must_use]
    pub fn autoload_declaration(&self, file: CompilationFileId) -> Option<&str> {
        self.autoload_declarations.get(&file).map(String::as_str)
    }

    /// Returns all files in stable insertion order.
    #[must_use]
    pub fn files(&self) -> &[CompilationSource] {
        &self.files
    }

    /// Returns the entry file ID.
    #[must_use]
    pub const fn entry(&self) -> CompilationFileId {
        self.entry
    }

    /// Returns explicit dependency edges in discovery order.
    #[must_use]
    pub fn dependencies(&self) -> &[CompilationDependency] {
        &self.dependencies
    }

    /// Returns the first dependency cycle in deterministic discovery order.
    #[must_use]
    pub fn dependency_cycle(&self) -> Option<CompilationCycle> {
        fn visit(
            session: &CompilationSession,
            file: CompilationFileId,
            active_files: &mut Vec<CompilationFileId>,
            active_edges: &mut Vec<CompilationDependency>,
            emitted: &mut HashSet<CompilationFileId>,
        ) -> Option<CompilationCycle> {
            active_files.push(file);
            for edge in session
                .dependencies
                .iter()
                .filter(|edge| edge.requester == file)
            {
                if let Some(cycle_start) = active_files
                    .iter()
                    .position(|active| *active == edge.dependency)
                {
                    let mut edges = active_edges[cycle_start..].to_vec();
                    edges.push(edge.clone());
                    return Some(CompilationCycle { edges });
                }
                if emitted.contains(&edge.dependency) {
                    continue;
                }
                active_edges.push(edge.clone());
                if let Some(cycle) = visit(
                    session,
                    edge.dependency,
                    active_files,
                    active_edges,
                    emitted,
                ) {
                    return Some(cycle);
                }
                active_edges.pop();
            }
            active_files.pop();
            emitted.insert(file);
            None
        }

        visit(
            self,
            self.entry,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut HashSet::new(),
        )
    }

    /// Returns normalized traits declared by one source file.
    #[must_use]
    pub fn declared_trait_names(&self, file_id: CompilationFileId) -> Vec<String> {
        let Some(file) = self.files.get(file_id.index()) else {
            return Vec::new();
        };
        let frontend = file.frontend();
        let Some(module) = frontend.database().module(frontend.module().module_id()) else {
            return Vec::new();
        };
        module
            .class_likes()
            .values()
            .filter(|class_like| class_like.kind() == ClassLikeKind::Trait)
            .filter_map(|class_like| {
                class_like
                    .fqn()
                    .map(|name| name.canonical(NameKind::ClassLike))
                    .or_else(|| class_like.name().map(normalize_class_name))
            })
            .map(|name| normalize_class_name(&name))
            .collect()
    }

    /// Returns typed trait requests not declared in the requesting file.
    #[must_use]
    pub fn unresolved_trait_requests(
        &self,
        file_id: CompilationFileId,
    ) -> Vec<UnresolvedTraitRequest> {
        let Some(file) = self.files.get(file_id.index()) else {
            return Vec::new();
        };
        let frontend = file.frontend();
        let Some(module) = frontend.database().module(frontend.module().module_id()) else {
            return Vec::new();
        };
        let local_traits = self
            .declared_trait_names(file_id)
            .into_iter()
            .collect::<HashSet<_>>();
        let mut requests = Vec::new();
        for class_like in module.class_likes().values() {
            let owner_name = class_like
                .fqn()
                .map(|name| name.canonical(NameKind::ClassLike))
                .or_else(|| class_like.name().map(normalize_class_name))
                .unwrap_or_default();
            for member in class_like.members() {
                let Some(ClassLikeMemberId::TraitUse(trait_use_id)) = member.id() else {
                    continue;
                };
                let Some(trait_use) = module.trait_uses().get(trait_use_id) else {
                    continue;
                };
                for trait_name in trait_use.traits() {
                    let resolved_name = trait_name
                        .resolved_display()
                        .or_else(|| trait_name.resolved())
                        .or_else(|| trait_name.fallback())
                        .unwrap_or_else(|| trait_name.source())
                        .trim_start_matches('\\')
                        .to_owned();
                    let normalized_name = normalize_class_name(&resolved_name);
                    if local_traits.contains(&normalized_name)
                        || requests.iter().any(|request: &UnresolvedTraitRequest| {
                            request.normalized_name == normalized_name
                        })
                    {
                        continue;
                    }
                    requests.push(UnresolvedTraitRequest {
                        requesting_file: file_id,
                        normalized_name,
                        resolved_name,
                        display_name: trait_name.source().to_owned(),
                        owner_name: normalize_class_name(&owner_name),
                    });
                }
            }
        }
        requests
    }

    pub(crate) fn lowering_order(&self) -> Vec<CompilationFileId> {
        fn visit(
            session: &CompilationSession,
            file: CompilationFileId,
            active: &mut HashSet<CompilationFileId>,
            emitted: &mut HashSet<CompilationFileId>,
            order: &mut Vec<CompilationFileId>,
        ) {
            if emitted.contains(&file) || !active.insert(file) {
                return;
            }
            for edge in session
                .dependencies
                .iter()
                .filter(|edge| edge.requester == file)
            {
                visit(session, edge.dependency, active, emitted, order);
            }
            active.remove(&file);
            if emitted.insert(file) {
                order.push(file);
            }
        }

        let mut order = Vec::new();
        visit(
            self,
            self.entry,
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut order,
        );
        order
    }
}

#[cfg(test)]
mod tests {
    use super::CompilationSession;
    use crate::{LoweringOptions, lower_compilation_session};

    #[test]
    fn lowers_cross_file_trait_without_rewriting_sources() {
        let mut session = CompilationSession::new(
            "/app/src/Registry.php",
            "<?php\ndeclare(strict_types=1);\nnamespace App;\nuse Lib\\Transport;\nclass Registry { use Transport { send as protected dispatch; } }\n",
        );
        let request = session
            .unresolved_trait_requests(session.entry())
            .pop()
            .expect("typed trait request");
        session.add_dependency(
            request.requesting_file,
            request.normalized_name,
            "/app/lib/Transport.php",
            "<?php\ndeclare(strict_types=0);\nnamespace Lib;\ntrait Transport { private $client = null; public function send(): string { return __FILE__; } }\n",
        );

        let result = lower_compilation_session(&session, LoweringOptions::default());
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert!(result.verification.is_ok(), "{:?}", result.verification);
        assert_eq!(result.unit.files[0].path, "/app/src/Registry.php");
        assert_eq!(result.unit.files[1].path, "/app/lib/Transport.php");
        assert_eq!(result.unit.file_strict_types, vec![true, false]);
        assert_eq!(session.files()[0].namespaces(), ["", "App"]);
        assert_eq!(session.files()[1].namespaces(), ["", "Lib"]);
        assert!(session.files()[0].strict_types());
        assert!(!session.files()[1].strict_types());

        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "app\\registry")
            .expect("root class");
        let alias = class
            .methods
            .iter()
            .find(|method| method.name == "dispatch")
            .expect("trait alias");
        assert!(alias.flags.is_protected);
        assert!(
            class
                .properties
                .iter()
                .any(|property| property.name == "client")
        );
        assert_eq!(
            result.unit.functions[alias.function.index()]
                .span
                .file
                .index(),
            1,
            "trait method keeps the dependency file origin"
        );
    }

    #[test]
    fn dependency_cycles_are_typed_and_deterministic() {
        let mut session =
            CompilationSession::new("/app/A.php", "<?php namespace App; trait A { use B; }");
        let a = session.entry();
        let b = session.add_dependency(
            a,
            "app\\b",
            "/app/B.php",
            "<?php namespace App; trait B { use A; }",
        );
        session.add_dependency(b, "app\\a", "/app/A.php", "ignored");

        let first = session.dependency_cycle().expect("dependency cycle");
        let second = session.dependency_cycle().expect("dependency cycle");
        assert_eq!(first, second);
        assert_eq!(first.edges.len(), 2);
        assert_eq!(first.edges[0].requester, a);
        assert_eq!(first.edges[0].dependency, b);
        assert_eq!(first.edges[0].declaration, "app\\b");
        assert_eq!(first.edges[1].requester, b);
        assert_eq!(first.edges[1].dependency, a);
        assert_eq!(first.edges[1].declaration, "app\\a");
    }

    #[test]
    fn diagnostics_on_identical_lines_keep_their_original_files() {
        let mut session = CompilationSession::new(
            "/app/Root.php",
            "<?php namespace App; class Root { use Dep; use RootMissing; }",
        );
        session.add_dependency(
            session.entry(),
            "app\\dep",
            "/app/Dep.php",
            "<?php namespace App; trait Dep { use DepMissing; }",
        );

        let result = lower_compilation_session(&session, LoweringOptions::default());
        let mut missing = result
            .diagnostics
            .iter()
            .filter_map(|diagnostic| {
                diagnostic
                    .missing_trait()
                    .map(|payload| (payload.normalized_name.as_str(), diagnostic.span.file))
            })
            .collect::<Vec<_>>();
        missing.sort_by_key(|(name, _)| *name);

        assert_eq!(missing.len(), 2, "{:?}", result.diagnostics);
        assert_eq!(missing[0].0, "app\\depmissing");
        assert_eq!(missing[0].1.index(), 1);
        assert_eq!(missing[1].0, "app\\rootmissing");
        assert_eq!(missing[1].1.index(), 0);
    }

    #[test]
    fn linked_traits_apply_alias_and_precedence_adaptations() {
        let mut session = CompilationSession::new(
            "/app/Registry.php",
            "<?php namespace Demo;
                use Demo\\Traits\\PRIMARYTRAIT as First;
                use Demo\\Traits\\SecondaryTrait;
                class Registry {
                    use First, SecondaryTrait {
                        First::send insteadof SecondaryTrait;
                        SecondaryTrait::send as backup;
                    }
                }
            ",
        );
        session.add_dependency(
            session.entry(),
            "demo\\traits\\primarytrait",
            "/app/PrimaryTrait.php",
            "<?php namespace Demo\\Traits; trait PrimaryTrait { public function send(): string { return 'primary'; } }",
        );
        session.add_dependency(
            session.entry(),
            "demo\\traits\\secondarytrait",
            "/app/SecondaryTrait.php",
            "<?php namespace Demo\\Traits; trait SecondaryTrait { public function send(): string { return 'secondary'; } }",
        );

        let result = lower_compilation_session(&session, LoweringOptions::default());
        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "demo\\registry")
            .expect("registry class");
        let methods = class
            .methods
            .iter()
            .map(|method| method.name.as_str())
            .collect::<Vec<_>>();
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(methods, vec!["send", "backup"]);
    }
}
