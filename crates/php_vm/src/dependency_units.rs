//! Metadata-only dependency-unit planner for future module compilation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use php_ir::constants::IrConstant;
use php_ir::ids::{ClassId, ConstId, FileId, FunctionId};
use php_ir::instruction::{CallableKind, InstructionKind, IrCallArg};
use php_ir::module::{ClassEntry, IrUnit};
use php_ir::source_map::IrSpan;
use php_ir::{Operand, instruction::IncludeKind};

/// Stable dependency-unit identifier inside one planned graph.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyUnitId(u32);

impl DependencyUnitId {
    /// Creates a unit ID from a zero-based index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the raw integer used in reports.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    fn as_report_key(self) -> String {
        format!("u{}", self.0)
    }
}

/// Planned immutable-unit family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyUnitKind {
    /// Source file unit.
    File,
    /// User function body.
    Function,
    /// Class/interface/enum metadata.
    Class,
    /// Class method implementation edge.
    Method,
    /// Class property metadata.
    Property,
    /// Global/class constant metadata.
    Constant,
    /// Literal constant or constant array used by executable code.
    Literal,
    /// Include expression site.
    IncludeExpression,
    /// Runtime lookup that may consult autoload state.
    Lookup,
    /// Autoload resolver/map fingerprint.
    AutoloadResolver,
    /// Configuration fingerprint component.
    Configuration,
}

impl DependencyUnitKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Function => "function",
            Self::Class => "class",
            Self::Method => "method",
            Self::Property => "property",
            Self::Constant => "constant",
            Self::Literal => "literal",
            Self::IncludeExpression => "include_expression",
            Self::Lookup => "lookup",
            Self::AutoloadResolver => "autoload_resolver",
            Self::Configuration => "configuration",
        }
    }
}

/// Source span rendered without depending on frontend source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DependencySpan {
    /// Source file index.
    pub file: u32,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset.
    pub end: u32,
}

impl From<IrSpan> for DependencySpan {
    fn from(span: IrSpan) -> Self {
        Self {
            file: span.file.raw(),
            start: span.start,
            end: span.end,
        }
    }
}

/// Best-effort source identity for invalidation reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileFingerprint {
    /// Stable path from the IR file table.
    pub path: String,
    /// Canonical path when filesystem metadata is available.
    pub canonical_path: Option<String>,
    /// Deterministic content hash when the file can be read.
    pub content_hash: Option<String>,
    /// File length from metadata.
    pub len: Option<u64>,
    /// Last modification timestamp in Unix nanoseconds.
    pub modified_unix_nanos: Option<u128>,
    /// Device ID on Unix platforms.
    pub dev: Option<u64>,
    /// Inode number on Unix platforms.
    pub inode: Option<u64>,
    /// Whether this represents a missing path observation.
    pub missing: bool,
}

/// One planner unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyUnit {
    /// Unit ID.
    pub id: DependencyUnitId,
    /// Unit family.
    pub kind: DependencyUnitKind,
    /// Stable human-readable name.
    pub name: String,
    /// Owning source file, if known.
    pub file: Option<u32>,
    /// Source span, if known.
    pub span: Option<DependencySpan>,
    /// File fingerprint for file units.
    pub fingerprint: Option<FileFingerprint>,
    /// Deterministic supplemental metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Dependency edge family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyEdgeKind {
    /// Source file owns a declaration.
    Declares,
    /// Class owns a member.
    OwnsMember,
    /// Function or declaration uses a literal.
    UsesLiteral,
    /// Function contains an include expression.
    ContainsInclude,
    /// Include expression resolves to a target file.
    ResolvesInclude,
    /// Include expression observed no current target.
    NegativeIncludeLookup,
    /// Lookup consults an autoload resolver.
    UsesAutoloadResolver,
    /// Unit depends on a file fingerprint.
    DependsOnFileFingerprint,
    /// Unit depends on a configuration component.
    DependsOnConfig,
}

impl DependencyEdgeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declares => "declares",
            Self::OwnsMember => "owns_member",
            Self::UsesLiteral => "uses_literal",
            Self::ContainsInclude => "contains_include",
            Self::ResolvesInclude => "resolves_include",
            Self::NegativeIncludeLookup => "negative_include_lookup",
            Self::UsesAutoloadResolver => "uses_autoload_resolver",
            Self::DependsOnFileFingerprint => "depends_on_file_fingerprint",
            Self::DependsOnConfig => "depends_on_config",
        }
    }
}

/// Invalidation reason attached to dependency edges.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InvalidationReason {
    /// Source bytes changed.
    SourceContentChanged,
    /// Metadata such as mtime, length, inode, or permissions changed.
    FileMetadataChanged,
    /// Symlink resolution can change the target identity.
    SymlinkTargetChanged,
    /// Include path ordering or entries changed.
    IncludePathChanged,
    /// Case-sensitive and case-insensitive filesystems differ.
    CaseSensitivityChanged,
    /// Generated file lifecycle changed.
    GeneratedFileChanged,
    /// Future PHAR/archive identity changed.
    PharArchiveChanged,
    /// Autoload map or resolver version changed.
    AutoloadMapChanged,
    /// Runtime or compile configuration changed.
    ConfigurationChanged,
    /// Negative lookup observations cannot be trusted forever.
    NegativeLookupExpired,
}

impl InvalidationReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceContentChanged => "source_content_changed",
            Self::FileMetadataChanged => "file_metadata_changed",
            Self::SymlinkTargetChanged => "symlink_target_changed",
            Self::IncludePathChanged => "include_path_changed",
            Self::CaseSensitivityChanged => "case_sensitivity_changed",
            Self::GeneratedFileChanged => "generated_file_changed",
            Self::PharArchiveChanged => "phar_archive_changed",
            Self::AutoloadMapChanged => "autoload_map_changed",
            Self::ConfigurationChanged => "configuration_changed",
            Self::NegativeLookupExpired => "negative_lookup_expired",
        }
    }
}

/// One directed dependency edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyEdge {
    /// Source unit.
    pub from: DependencyUnitId,
    /// Target unit when known.
    pub to: Option<DependencyUnitId>,
    /// Edge family.
    pub kind: DependencyEdgeKind,
    /// Stable explanation.
    pub detail: String,
    /// Invalidation reasons that make the edge stale.
    pub invalidation_reasons: Vec<InvalidationReason>,
    /// Whether this edge can be used without revalidation.
    pub trusted: bool,
}

/// Planned dependency graph.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DependencyGraph {
    /// Units in deterministic insertion order.
    pub units: Vec<DependencyUnit>,
    /// Directed dependency edges.
    pub edges: Vec<DependencyEdge>,
}

impl DependencyGraph {
    fn push_unit(&mut self, mut unit: DependencyUnit) -> DependencyUnitId {
        let id = DependencyUnitId::new(self.units.len() as u32);
        unit.id = id;
        self.units.push(unit);
        id
    }

    fn push_edge(&mut self, edge: DependencyEdge) {
        self.edges.push(edge);
    }

    /// Returns the first unit matching a unit kind/name pair.
    #[must_use]
    pub fn unit_by_kind_name(
        &self,
        kind: DependencyUnitKind,
        name: &str,
    ) -> Option<DependencyUnitId> {
        self.units
            .iter()
            .find(|unit| unit.kind == kind && unit.name == name)
            .map(|unit| unit.id)
    }
}

/// Runtime-observed include resolution for report-only enrichment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedIncludeTarget {
    /// Stable include expression label.
    pub expression: String,
    /// Resolved target paths observed for that expression.
    pub targets: Vec<String>,
    /// Include-path fingerprint or version label.
    pub include_path_fingerprint: String,
}

/// Runtime-observed lookup/autoload resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedLookup {
    /// Lookup family, such as `class`, `function`, or `constant`.
    pub kind: String,
    /// Lookup name.
    pub name: String,
    /// Resolved target when one was found.
    pub target: Option<String>,
    /// Autoload resolver or map fingerprint.
    pub resolver_fingerprint: String,
}

/// Additional planner inputs that are only available from runtime observation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DependencyPlannerInputs {
    /// Observed include target sets.
    pub observed_includes: Vec<ObservedIncludeTarget>,
    /// Observed lookup/autoload behavior.
    pub observed_lookups: Vec<ObservedLookup>,
    /// Sorted configuration components that affect unit reuse.
    pub configuration: BTreeMap<String, String>,
}

/// Planner report with counters and deterministic renderers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DependencyUnitReport {
    /// Planned graph.
    pub graph: DependencyGraph,
    /// Report counters.
    pub counters: BTreeMap<String, u64>,
    /// Invalidations grouped by reason.
    pub invalidation_by_reason: BTreeMap<String, u64>,
}

impl DependencyUnitReport {
    fn from_graph(graph: DependencyGraph) -> Self {
        let mut counters = BTreeMap::new();
        counters.insert("dependency_units".to_owned(), graph.units.len() as u64);
        counters.insert("dependency_edges".to_owned(), graph.edges.len() as u64);
        for unit in &graph.units {
            *counters
                .entry(format!("dependency_units_{}", unit.kind.as_str()))
                .or_default() += 1;
        }
        for edge in &graph.edges {
            *counters
                .entry(format!("dependency_edges_{}", edge.kind.as_str()))
                .or_default() += 1;
        }

        let mut invalidation_by_reason = BTreeMap::new();
        for edge in &graph.edges {
            for reason in &edge.invalidation_reasons {
                *invalidation_by_reason
                    .entry(reason.as_str().to_owned())
                    .or_default() += 1;
            }
        }

        Self {
            graph,
            counters,
            invalidation_by_reason,
        }
    }

    /// Deterministic digest suitable for optional cache-fingerprint extras.
    #[must_use]
    pub fn fingerprint_component(&self) -> String {
        stable_hash_hex(self.to_json().as_bytes())
    }

    /// Renders a stable Markdown report.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Dependency Units\n\n");
        out.push_str("## Counters\n\n");
        for (key, value) in &self.counters {
            out.push_str(&format!("- {key}: {value}\n"));
        }
        out.push_str("\n## Invalidation Reasons\n\n");
        for (key, value) in &self.invalidation_by_reason {
            out.push_str(&format!("- {key}: {value}\n"));
        }
        out.push_str("\n## Units\n\n");
        for unit in &self.graph.units {
            out.push_str(&format!(
                "- {} {} `{}`",
                unit.id.as_report_key(),
                unit.kind.as_str(),
                unit.name
            ));
            if let Some(file) = unit.file {
                out.push_str(&format!(" file={file}"));
            }
            if let Some(fingerprint) = &unit.fingerprint {
                if let Some(hash) = &fingerprint.content_hash {
                    out.push_str(&format!(" hash={hash}"));
                }
                if fingerprint.missing {
                    out.push_str(" missing=true");
                }
            }
            out.push('\n');
        }
        out.push_str("\n## Edges\n\n");
        for edge in &self.graph.edges {
            let target = edge
                .to
                .map_or_else(|| "unknown".to_owned(), DependencyUnitId::as_report_key);
            let reasons = edge
                .invalidation_reasons
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(
                "- {} -> {} {} trusted={} detail=`{}` reasons=[{}]\n",
                edge.from.as_report_key(),
                target,
                edge.kind.as_str(),
                edge.trusted,
                edge.detail,
                reasons
            ));
        }
        out
    }

    /// Renders deterministic JSON without adding a serde dependency to `php_vm`.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        push_json_map_u64(&mut out, "  ", "counters", &self.counters, true);
        push_json_map_u64(
            &mut out,
            "  ",
            "invalidation_by_reason",
            &self.invalidation_by_reason,
            true,
        );
        out.push_str("  \"units\": [\n");
        for (index, unit) in self.graph.units.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"id\": {},\n", unit.id.raw()));
            out.push_str(&format!(
                "      \"kind\": \"{}\",\n",
                json_escape(unit.kind.as_str())
            ));
            out.push_str(&format!("      \"name\": \"{}\"", json_escape(&unit.name)));
            if let Some(file) = unit.file {
                out.push_str(&format!(",\n      \"file\": {file}"));
            }
            if let Some(span) = unit.span {
                out.push_str(&format!(
                    ",\n      \"span\": {{\"file\": {}, \"start\": {}, \"end\": {}}}",
                    span.file, span.start, span.end
                ));
            }
            if let Some(fingerprint) = &unit.fingerprint {
                out.push_str(",\n      \"fingerprint\": ");
                push_file_fingerprint_json(&mut out, fingerprint);
            }
            if !unit.metadata.is_empty() {
                out.push_str(",\n");
                push_json_map_string(&mut out, "      ", "metadata", &unit.metadata, false);
            }
            out.push_str("\n    }");
            if index + 1 != self.graph.units.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ],\n");
        out.push_str("  \"edges\": [\n");
        for (index, edge) in self.graph.edges.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"from\": {},\n", edge.from.raw()));
            match edge.to {
                Some(to) => out.push_str(&format!("      \"to\": {},\n", to.raw())),
                None => out.push_str("      \"to\": null,\n"),
            }
            out.push_str(&format!(
                "      \"kind\": \"{}\",\n",
                json_escape(edge.kind.as_str())
            ));
            out.push_str(&format!(
                "      \"detail\": \"{}\",\n",
                json_escape(&edge.detail)
            ));
            out.push_str(&format!("      \"trusted\": {},\n", edge.trusted));
            out.push_str("      \"invalidation_reasons\": [");
            for (reason_index, reason) in edge.invalidation_reasons.iter().enumerate() {
                if reason_index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format!("\"{}\"", reason.as_str()));
            }
            out.push_str("]\n    }");
            if index + 1 != self.graph.edges.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ]\n}\n");
        out
    }
}

/// Build a metadata-only dependency-unit report from an IR unit.
#[must_use]
pub fn plan_dependency_units(unit: &IrUnit) -> DependencyUnitReport {
    plan_dependency_units_with_inputs(unit, &DependencyPlannerInputs::default())
}

/// Build a metadata-only dependency-unit report with runtime observations.
#[must_use]
pub fn plan_dependency_units_with_inputs(
    unit: &IrUnit,
    inputs: &DependencyPlannerInputs,
) -> DependencyUnitReport {
    let mut planner = Planner::new(unit);
    planner.add_files();
    planner.add_autoload_resolver("static", "static-autoload-map:none");
    planner.add_configuration(inputs);
    planner.add_functions();
    planner.add_classes();
    planner.add_global_constants();
    planner.add_function_body_edges();
    planner.add_observed_includes(inputs);
    planner.add_observed_lookups(inputs);
    DependencyUnitReport::from_graph(planner.graph)
}

struct Planner<'a> {
    unit: &'a IrUnit,
    graph: DependencyGraph,
    files: BTreeMap<FileId, DependencyUnitId>,
    functions: BTreeMap<FunctionId, DependencyUnitId>,
    classes: BTreeMap<ClassId, DependencyUnitId>,
    constants: BTreeMap<ConstId, DependencyUnitId>,
    literals: BTreeMap<ConstId, DependencyUnitId>,
    autoload_resolver: Option<DependencyUnitId>,
}

impl<'a> Planner<'a> {
    fn new(unit: &'a IrUnit) -> Self {
        Self {
            unit,
            graph: DependencyGraph::default(),
            files: BTreeMap::new(),
            functions: BTreeMap::new(),
            classes: BTreeMap::new(),
            constants: BTreeMap::new(),
            literals: BTreeMap::new(),
            autoload_resolver: None,
        }
    }

    fn add_files(&mut self) {
        for file in &self.unit.files {
            let fingerprint = file_fingerprint(&file.path);
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::File,
                name: file.path.clone(),
                file: Some(file.id.raw()),
                span: None,
                fingerprint: Some(fingerprint),
                metadata: BTreeMap::new(),
            });
            self.files.insert(file.id, id);
        }
    }

    fn add_autoload_resolver(&mut self, name: &str, fingerprint: &str) {
        let mut metadata = BTreeMap::new();
        metadata.insert("fingerprint".to_owned(), fingerprint.to_owned());
        let id = self.graph.push_unit(DependencyUnit {
            id: DependencyUnitId::new(0),
            kind: DependencyUnitKind::AutoloadResolver,
            name: name.to_owned(),
            file: None,
            span: None,
            fingerprint: None,
            metadata,
        });
        self.autoload_resolver = Some(id);
    }

    fn add_configuration(&mut self, inputs: &DependencyPlannerInputs) {
        for (name, value) in &inputs.configuration {
            let mut metadata = BTreeMap::new();
            metadata.insert("value".to_owned(), value.clone());
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::Configuration,
                name: name.clone(),
                file: None,
                span: None,
                fingerprint: None,
                metadata,
            });
            let file_ids = self.files.values().copied().collect::<Vec<_>>();
            for file_id in file_ids {
                self.edge(
                    file_id,
                    Some(id),
                    DependencyEdgeKind::DependsOnConfig,
                    format!("configuration {name}"),
                    &[InvalidationReason::ConfigurationChanged],
                    true,
                );
            }
        }
    }

    fn add_functions(&mut self) {
        for (index, function) in self.unit.functions.iter().enumerate() {
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::Function,
                name: function.name.clone(),
                file: Some(function.span.file.raw()),
                span: Some(function.span.into()),
                fingerprint: None,
                metadata: BTreeMap::new(),
            });
            let function_id = FunctionId::new(index as u32);
            self.functions.insert(function_id, id);
            if let Some(file_id) = self.files.get(&function.span.file).copied() {
                self.edge(
                    file_id,
                    Some(id),
                    DependencyEdgeKind::Declares,
                    format!("function {}", function.name),
                    &[
                        InvalidationReason::SourceContentChanged,
                        InvalidationReason::FileMetadataChanged,
                    ],
                    true,
                );
            }
        }
    }

    fn add_classes(&mut self) {
        for class in &self.unit.classes {
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::Class,
                name: class.name.clone(),
                file: Some(class.span.file.raw()),
                span: Some(class.span.into()),
                fingerprint: None,
                metadata: class_metadata(class),
            });
            self.classes.insert(class.id, id);
            if let Some(file_id) = self.files.get(&class.span.file).copied() {
                self.edge(
                    file_id,
                    Some(id),
                    DependencyEdgeKind::Declares,
                    format!("class {}", class.name),
                    &[
                        InvalidationReason::SourceContentChanged,
                        InvalidationReason::FileMetadataChanged,
                    ],
                    true,
                );
            }
            self.add_class_members(class, id);
            self.add_class_lookup_edges(class, id);
        }
    }

    fn add_class_members(&mut self, class: &ClassEntry, class_id: DependencyUnitId) {
        for method in &class.methods {
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::Method,
                name: format!("{}::{}", class.name, method.name),
                file: self
                    .unit
                    .functions
                    .get(method.function.index())
                    .map(|function| function.span.file.raw()),
                span: self
                    .unit
                    .functions
                    .get(method.function.index())
                    .map(|function| function.span.into()),
                fingerprint: None,
                metadata: BTreeMap::from([(
                    "function".to_owned(),
                    method.function.raw().to_string(),
                )]),
            });
            self.edge(
                class_id,
                Some(id),
                DependencyEdgeKind::OwnsMember,
                format!("method {}", method.name),
                &[InvalidationReason::SourceContentChanged],
                true,
            );
            if let Some(function_id) = self.functions.get(&method.function).copied() {
                self.edge(
                    id,
                    Some(function_id),
                    DependencyEdgeKind::Declares,
                    format!("method body {}", method.name),
                    &[InvalidationReason::SourceContentChanged],
                    true,
                );
            }
        }
        for property in &class.properties {
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::Property,
                name: format!("{}::${}", class.name, property.name),
                file: Some(class.span.file.raw()),
                span: Some(class.span.into()),
                fingerprint: None,
                metadata: BTreeMap::new(),
            });
            self.edge(
                class_id,
                Some(id),
                DependencyEdgeKind::OwnsMember,
                format!("property {}", property.name),
                &[InvalidationReason::SourceContentChanged],
                true,
            );
            if let Some(default) = property.default {
                self.add_literal_edge(id, default, "property_default");
            }
        }
    }

    fn add_class_lookup_edges(&mut self, class: &ClassEntry, owner: DependencyUnitId) {
        if let Some(parent) = &class.parent {
            self.add_lookup_edge(owner, "class", parent);
        }
        for interface in &class.interfaces {
            self.add_lookup_edge(owner, "class", interface);
        }
        for attribute in &class.attributes {
            if let Some(name) = attribute
                .resolved_name
                .as_ref()
                .or(attribute.fallback_name.as_ref())
            {
                self.add_lookup_edge(owner, "class", name);
            }
        }
    }

    fn add_global_constants(&mut self) {
        for entry in &self.unit.constant_table {
            let id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::Constant,
                name: entry.name.clone(),
                file: Some(entry.span.file.raw()),
                span: Some(entry.span.into()),
                fingerprint: None,
                metadata: BTreeMap::new(),
            });
            self.constants.insert(entry.value, id);
            if let Some(file_id) = self.files.get(&entry.span.file).copied() {
                self.edge(
                    file_id,
                    Some(id),
                    DependencyEdgeKind::Declares,
                    format!("constant {}", entry.name),
                    &[
                        InvalidationReason::SourceContentChanged,
                        InvalidationReason::FileMetadataChanged,
                    ],
                    true,
                );
            }
            self.add_literal_edge(id, entry.value, "constant_value");
        }
    }

    fn add_function_body_edges(&mut self) {
        for (function_index, function) in self.unit.functions.iter().enumerate() {
            let function_id = FunctionId::new(function_index as u32);
            let Some(owner) = self.functions.get(&function_id).copied() else {
                continue;
            };
            for param in &function.params {
                if let Some(default) = &param.default {
                    let const_id = const_id_for_value(&self.unit.constants, default);
                    if let Some(const_id) = const_id {
                        self.add_literal_edge(owner, const_id, "parameter_default");
                    }
                }
            }
            for block in &function.blocks {
                for instruction in &block.instructions {
                    self.inspect_instruction(owner, instruction.kind.clone(), instruction.span);
                }
            }
        }
    }

    fn inspect_instruction(
        &mut self,
        owner: DependencyUnitId,
        kind: InstructionKind,
        span: IrSpan,
    ) {
        match kind {
            InstructionKind::LoadConst { constant, .. } => {
                self.add_literal_edge(owner, constant, "load_const");
            }
            InstructionKind::CallFunction { name, args, .. }
            | InstructionKind::BindReferenceFromCall { name, args, .. } => {
                self.add_lookup_edge(owner, "function", &name);
                self.add_call_arg_literals(owner, &args);
            }
            InstructionKind::CallMethod { method, args, .. } => {
                self.add_lookup_edge(owner, "method", &method);
                self.add_call_arg_literals(owner, &args);
            }
            InstructionKind::BindReferenceFromMethodCall { method, args, .. } => {
                self.add_lookup_edge(owner, "method", &method);
                self.add_call_arg_literals(owner, &args);
            }
            InstructionKind::CallStaticMethod {
                class_name,
                method,
                args,
                ..
            } => {
                self.add_lookup_edge(owner, "class", &class_name);
                self.add_lookup_edge(owner, "method", &format!("{class_name}::{method}"));
                self.add_call_arg_literals(owner, &args);
            }
            InstructionKind::NewObject {
                class_name, args, ..
            } => {
                self.add_lookup_edge(owner, "class", &class_name);
                self.add_call_arg_literals(owner, &args);
            }
            InstructionKind::FetchConst { name, .. } => {
                self.add_lookup_edge(owner, "constant", &name);
            }
            InstructionKind::RegisterConstant { name, .. } => {
                self.add_lookup_edge(owner, "constant", &name);
            }
            InstructionKind::FetchClassConstant {
                class_name,
                constant,
                ..
            } => {
                self.add_lookup_edge(owner, "class", &class_name);
                self.add_lookup_edge(
                    owner,
                    "class_constant",
                    &format!("{class_name}::{constant}"),
                );
            }
            InstructionKind::InstanceOf { class_name, .. }
            | InstructionKind::FetchStaticProperty { class_name, .. }
            | InstructionKind::IssetStaticProperty { class_name, .. }
            | InstructionKind::EmptyStaticProperty { class_name, .. }
            | InstructionKind::IssetStaticPropertyDim { class_name, .. }
            | InstructionKind::EmptyStaticPropertyDim { class_name, .. }
            | InstructionKind::UnsetStaticPropertyDim { class_name, .. }
            | InstructionKind::BindReferenceStaticProperty { class_name, .. }
            | InstructionKind::BindReferenceFromStaticPropertyDim { class_name, .. }
            | InstructionKind::AssignStaticProperty { class_name, .. } => {
                self.add_lookup_edge(owner, "class", &class_name);
            }
            InstructionKind::FetchDynamicStaticProperty { .. }
            | InstructionKind::AssignDynamicStaticProperty { .. } => {}
            InstructionKind::ResolveCallable { callable, .. } => match callable {
                CallableKind::FunctionName { name } => {
                    self.add_lookup_edge(owner, "function", &name)
                }
                CallableKind::MethodPlaceholder { target }
                | CallableKind::UnresolvedDynamic { target } => {
                    self.add_lookup_edge(owner, "callable", &target);
                }
            },
            InstructionKind::Include { kind, path, .. } => {
                self.add_include_edge(owner, span, kind, path);
            }
            _ => {}
        }
    }

    fn add_call_arg_literals(&mut self, owner: DependencyUnitId, args: &[IrCallArg]) {
        for arg in args {
            self.inspect_operand_literal(owner, arg.value, "call_arg");
        }
    }

    fn add_include_edge(
        &mut self,
        owner: DependencyUnitId,
        span: IrSpan,
        kind: IncludeKind,
        path: Operand,
    ) {
        let mut metadata = BTreeMap::new();
        metadata.insert("kind".to_owned(), format!("{kind:?}").to_ascii_lowercase());
        let path_label = static_string_operand(self.unit, path)
            .unwrap_or_else(|| format!("dynamic_include@{}..{}", span.start, span.end));
        let include_id = self.graph.push_unit(DependencyUnit {
            id: DependencyUnitId::new(0),
            kind: DependencyUnitKind::IncludeExpression,
            name: path_label.clone(),
            file: Some(span.file.raw()),
            span: Some(span.into()),
            fingerprint: None,
            metadata,
        });
        self.edge(
            owner,
            Some(include_id),
            DependencyEdgeKind::ContainsInclude,
            format!("{kind:?}"),
            &[
                InvalidationReason::IncludePathChanged,
                InvalidationReason::SymlinkTargetChanged,
            ],
            true,
        );
        if path_label != format!("dynamic_include@{}..{}", span.start, span.end) {
            let target = self.file_unit_for_path(&path_label);
            self.edge(
                include_id,
                Some(target),
                DependencyEdgeKind::ResolvesInclude,
                path_label,
                &[
                    InvalidationReason::SourceContentChanged,
                    InvalidationReason::FileMetadataChanged,
                    InvalidationReason::IncludePathChanged,
                    InvalidationReason::CaseSensitivityChanged,
                ],
                true,
            );
        }
    }

    fn add_observed_includes(&mut self, inputs: &DependencyPlannerInputs) {
        for observed in &inputs.observed_includes {
            let include_id = self.graph.push_unit(DependencyUnit {
                id: DependencyUnitId::new(0),
                kind: DependencyUnitKind::IncludeExpression,
                name: observed.expression.clone(),
                file: None,
                span: None,
                fingerprint: None,
                metadata: BTreeMap::from([(
                    "include_path_fingerprint".to_owned(),
                    observed.include_path_fingerprint.clone(),
                )]),
            });
            if observed.targets.is_empty() {
                self.edge(
                    include_id,
                    None,
                    DependencyEdgeKind::NegativeIncludeLookup,
                    observed.expression.clone(),
                    &[
                        InvalidationReason::IncludePathChanged,
                        InvalidationReason::NegativeLookupExpired,
                    ],
                    false,
                );
            }
            for target in &observed.targets {
                let file_id = self.file_unit_for_path(target);
                self.edge(
                    include_id,
                    Some(file_id),
                    DependencyEdgeKind::ResolvesInclude,
                    target.clone(),
                    &[
                        InvalidationReason::SourceContentChanged,
                        InvalidationReason::FileMetadataChanged,
                        InvalidationReason::IncludePathChanged,
                    ],
                    true,
                );
            }
        }
    }

    fn add_observed_lookups(&mut self, inputs: &DependencyPlannerInputs) {
        for observed in &inputs.observed_lookups {
            let lookup_id = self.add_lookup_unit(&observed.kind, &observed.name);
            let resolver = self.autoload_resolver_with_fingerprint(&observed.resolver_fingerprint);
            self.edge(
                lookup_id,
                Some(resolver),
                DependencyEdgeKind::UsesAutoloadResolver,
                observed
                    .target
                    .as_ref()
                    .map_or_else(|| "negative_lookup".to_owned(), |target| target.clone()),
                if observed.target.is_some() {
                    &[InvalidationReason::AutoloadMapChanged]
                } else {
                    &[
                        InvalidationReason::AutoloadMapChanged,
                        InvalidationReason::NegativeLookupExpired,
                    ]
                },
                observed.target.is_some(),
            );
        }
    }

    fn add_lookup_edge(&mut self, owner: DependencyUnitId, kind: &str, name: &str) {
        let lookup = self.add_lookup_unit(kind, name);
        if let Some(resolver) = self.autoload_resolver {
            self.edge(
                owner,
                Some(lookup),
                DependencyEdgeKind::UsesAutoloadResolver,
                format!("{kind}:{name}"),
                &[InvalidationReason::AutoloadMapChanged],
                true,
            );
            self.edge(
                lookup,
                Some(resolver),
                DependencyEdgeKind::UsesAutoloadResolver,
                "static autoload resolver".to_owned(),
                &[InvalidationReason::AutoloadMapChanged],
                true,
            );
        }
    }

    fn add_lookup_unit(&mut self, kind: &str, name: &str) -> DependencyUnitId {
        let unit_name = format!("{kind}:{name}");
        if let Some(id) = self
            .graph
            .unit_by_kind_name(DependencyUnitKind::Lookup, &unit_name)
        {
            return id;
        }
        self.graph.push_unit(DependencyUnit {
            id: DependencyUnitId::new(0),
            kind: DependencyUnitKind::Lookup,
            name: unit_name,
            file: None,
            span: None,
            fingerprint: None,
            metadata: BTreeMap::from([("lookup_kind".to_owned(), kind.to_owned())]),
        })
    }

    fn add_literal_edge(&mut self, owner: DependencyUnitId, constant: ConstId, detail: &str) {
        let literal = self.literal_unit(constant);
        self.edge(
            owner,
            Some(literal),
            DependencyEdgeKind::UsesLiteral,
            detail.to_owned(),
            &[InvalidationReason::SourceContentChanged],
            true,
        );
        self.add_nested_literals(owner, constant);
    }

    fn add_nested_literals(&mut self, owner: DependencyUnitId, constant: ConstId) {
        let Some(IrConstant::Array(entries)) = self.unit.constants.get(constant.index()) else {
            return;
        };
        for entry in entries {
            for nested in entry.key.iter().chain(std::iter::once(&entry.value)) {
                if let Some(nested_id) = const_id_for_value(&self.unit.constants, nested)
                    && nested_id != constant
                {
                    self.add_literal_edge(owner, nested_id, "nested_array_literal");
                }
            }
        }
    }

    fn inspect_operand_literal(&mut self, owner: DependencyUnitId, operand: Operand, detail: &str) {
        if let Operand::Constant(constant) = operand {
            self.add_literal_edge(owner, constant, detail);
        }
    }

    fn literal_unit(&mut self, constant: ConstId) -> DependencyUnitId {
        if let Some(id) = self.literals.get(&constant).copied() {
            return id;
        }
        let name = self
            .unit
            .constants
            .get(constant.index())
            .map_or_else(|| format!("c{}", constant.raw()), literal_label);
        let id = self.graph.push_unit(DependencyUnit {
            id: DependencyUnitId::new(0),
            kind: DependencyUnitKind::Literal,
            name,
            file: None,
            span: None,
            fingerprint: None,
            metadata: BTreeMap::from([("const".to_owned(), constant.raw().to_string())]),
        });
        self.literals.insert(constant, id);
        id
    }

    fn file_unit_for_path(&mut self, path: &str) -> DependencyUnitId {
        if let Some(id) = self.graph.unit_by_kind_name(DependencyUnitKind::File, path) {
            return id;
        }
        let id = self.graph.push_unit(DependencyUnit {
            id: DependencyUnitId::new(0),
            kind: DependencyUnitKind::File,
            name: path.to_owned(),
            file: None,
            span: None,
            fingerprint: Some(file_fingerprint(path)),
            metadata: BTreeMap::new(),
        });
        self.edge(
            id,
            Some(id),
            DependencyEdgeKind::DependsOnFileFingerprint,
            path.to_owned(),
            &[
                InvalidationReason::SourceContentChanged,
                InvalidationReason::FileMetadataChanged,
                InvalidationReason::SymlinkTargetChanged,
                InvalidationReason::GeneratedFileChanged,
                InvalidationReason::PharArchiveChanged,
            ],
            true,
        );
        id
    }

    fn autoload_resolver_with_fingerprint(&mut self, fingerprint: &str) -> DependencyUnitId {
        let name = format!("observed:{fingerprint}");
        if let Some(id) = self
            .graph
            .unit_by_kind_name(DependencyUnitKind::AutoloadResolver, &name)
        {
            return id;
        }
        let mut metadata = BTreeMap::new();
        metadata.insert("fingerprint".to_owned(), fingerprint.to_owned());
        self.graph.push_unit(DependencyUnit {
            id: DependencyUnitId::new(0),
            kind: DependencyUnitKind::AutoloadResolver,
            name,
            file: None,
            span: None,
            fingerprint: None,
            metadata,
        })
    }

    fn edge(
        &mut self,
        from: DependencyUnitId,
        to: Option<DependencyUnitId>,
        kind: DependencyEdgeKind,
        detail: String,
        invalidation_reasons: &[InvalidationReason],
        trusted: bool,
    ) {
        self.graph.push_edge(DependencyEdge {
            from,
            to,
            kind,
            detail,
            invalidation_reasons: invalidation_reasons.to_vec(),
            trusted,
        });
    }
}

fn class_metadata(class: &ClassEntry) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    if let Some(parent) = &class.parent {
        metadata.insert("parent".to_owned(), parent.clone());
    }
    if !class.interfaces.is_empty() {
        metadata.insert("interfaces".to_owned(), class.interfaces.join(","));
    }
    metadata.insert("methods".to_owned(), class.methods.len().to_string());
    metadata.insert("properties".to_owned(), class.properties.len().to_string());
    metadata
}

fn static_string_operand(unit: &IrUnit, operand: Operand) -> Option<String> {
    let Operand::Constant(id) = operand else {
        return None;
    };
    match unit.constants.get(id.index())? {
        IrConstant::String(value) => Some(value.clone()),
        IrConstant::StringBytes(value) => Some(String::from_utf8_lossy(value).into_owned()),
        _ => None,
    }
}

fn const_id_for_value(constants: &[IrConstant], value: &IrConstant) -> Option<ConstId> {
    constants
        .iter()
        .position(|constant| constant == value)
        .map(|index| ConstId::new(index as u32))
}

fn literal_label(constant: &IrConstant) -> String {
    match constant {
        IrConstant::Null => "null".to_owned(),
        IrConstant::Bool(value) => format!("bool:{value}"),
        IrConstant::Int(value) => format!("int:{value}"),
        IrConstant::Float(value) => format!("float:{value}"),
        IrConstant::String(value) => format!("string:{}", truncate(value, 40)),
        IrConstant::StringBytes(value) => format!("string_bytes:{}b", value.len()),
        IrConstant::NamedConstant(name) => format!("const:{}", truncate(name, 40)),
        IrConstant::ClassConstant {
            class_name,
            constant_name,
        } => format!(
            "class_const:{}::{}",
            truncate(class_name, 30),
            truncate(constant_name, 30)
        ),
        IrConstant::Array(entries) => format!("array:{}entries", entries.len()),
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    let mut out = value.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn file_fingerprint(path: &str) -> FileFingerprint {
    let raw_path = PathBuf::from(path);
    let canonical = fs::canonicalize(&raw_path).ok();
    let metadata_path = canonical.as_deref().unwrap_or_else(|| Path::new(path));
    let metadata = fs::metadata(metadata_path).ok();
    let content_hash = fs::read(metadata_path)
        .ok()
        .map(|bytes| stable_hash_hex(&bytes));
    let modified_unix_nanos = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let (dev, inode) = unix_file_identity(metadata.as_ref());
    FileFingerprint {
        path: path.to_owned(),
        canonical_path: canonical.map(|path| path.to_string_lossy().into_owned()),
        content_hash,
        len: metadata.as_ref().map(fs::Metadata::len),
        modified_unix_nanos,
        dev,
        inode,
        missing: metadata.is_none(),
    }
}

#[cfg(unix)]
fn unix_file_identity(metadata: Option<&fs::Metadata>) -> (Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;

    metadata.map_or((None, None), |metadata| {
        (Some(metadata.dev()), Some(metadata.ino()))
    })
}

#[cfg(not(unix))]
fn unix_file_identity(_metadata: Option<&fs::Metadata>) -> (Option<u64>, Option<u64>) {
    (None, None)
}

fn stable_hash_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn push_json_map_u64(
    out: &mut String,
    indent: &str,
    name: &str,
    map: &BTreeMap<String, u64>,
    comma: bool,
) {
    out.push_str(&format!("{indent}\"{name}\": {{"));
    if map.is_empty() {
        out.push('}');
        if comma {
            out.push(',');
        }
        out.push('\n');
        return;
    }
    out.push('\n');
    for (index, (key, value)) in map.iter().enumerate() {
        out.push_str(&format!(
            "{indent}  \"{}\": {}{}",
            json_escape(key),
            value,
            if index + 1 == map.len() { "" } else { "," }
        ));
        out.push('\n');
    }
    out.push_str(&format!("{indent}}}"));
    if comma {
        out.push(',');
    }
    out.push('\n');
}

fn push_json_map_string(
    out: &mut String,
    indent: &str,
    name: &str,
    map: &BTreeMap<String, String>,
    comma: bool,
) {
    out.push_str(&format!("{indent}\"{name}\": {{"));
    if map.is_empty() {
        out.push('}');
        if comma {
            out.push(',');
        }
        out.push('\n');
        return;
    }
    out.push('\n');
    for (index, (key, value)) in map.iter().enumerate() {
        out.push_str(&format!(
            "{indent}  \"{}\": \"{}\"{}",
            json_escape(key),
            json_escape(value),
            if index + 1 == map.len() { "" } else { "," }
        ));
        out.push('\n');
    }
    out.push_str(&format!("{indent}}}"));
    if comma {
        out.push(',');
    }
    out.push('\n');
}

fn push_file_fingerprint_json(out: &mut String, fingerprint: &FileFingerprint) {
    out.push('{');
    out.push_str(&format!("\"path\": \"{}\"", json_escape(&fingerprint.path)));
    if let Some(value) = &fingerprint.canonical_path {
        out.push_str(&format!(", \"canonical_path\": \"{}\"", json_escape(value)));
    }
    if let Some(value) = &fingerprint.content_hash {
        out.push_str(&format!(", \"content_hash\": \"{}\"", json_escape(value)));
    }
    if let Some(value) = fingerprint.len {
        out.push_str(&format!(", \"len\": {value}"));
    }
    if let Some(value) = fingerprint.modified_unix_nanos {
        out.push_str(&format!(", \"modified_unix_nanos\": {value}"));
    }
    if let Some(value) = fingerprint.dev {
        out.push_str(&format!(", \"dev\": {value}"));
    }
    if let Some(value) = fingerprint.inode {
        out.push_str(&format!(", \"inode\": {value}"));
    }
    out.push_str(&format!(", \"missing\": {}", fingerprint.missing));
    out.push('}');
}

fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        DependencyEdgeKind, DependencyPlannerInputs, DependencyUnitKind, InvalidationReason,
        ObservedIncludeTarget, ObservedLookup, plan_dependency_units_with_inputs,
    };
    use php_ir::ids::{ClassId, UnitId};
    use php_ir::instruction::{IncludeKind, InstructionKind};
    use php_ir::module::{
        ClassEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ClassPropertyEntry,
        ClassPropertyFlags, ClassPropertyHooks,
    };
    use php_ir::{FunctionFlags, IrBuilder, IrConstant, IrSpan, Operand};
    use std::fs;

    #[test]
    fn dependency_units_include_file_functions_and_classes() {
        let unit = sample_unit();
        let report = plan_dependency_units_with_inputs(&unit, &DependencyPlannerInputs::default());

        assert!(
            report
                .graph
                .units
                .iter()
                .any(|unit| { unit.kind == DependencyUnitKind::Function && unit.name == "main" })
        );
        assert!(
            report
                .graph
                .units
                .iter()
                .any(|unit| unit.kind == DependencyUnitKind::Class && unit.name == "App\\Thing")
        );
        assert!(report.graph.edges.iter().any(|edge| {
            edge.kind == DependencyEdgeKind::Declares
                && edge.detail == "function main"
                && edge
                    .invalidation_reasons
                    .contains(&InvalidationReason::SourceContentChanged)
        }));
    }

    #[test]
    fn static_include_edge_records_target_fingerprint() {
        let unit = sample_unit();
        let report = plan_dependency_units_with_inputs(&unit, &DependencyPlannerInputs::default());

        assert!(report.graph.edges.iter().any(|edge| {
            edge.kind == DependencyEdgeKind::ResolvesInclude
                && edge.detail == "tests/fixtures/performance/bytecode_cache/functions.php"
                && edge
                    .invalidation_reasons
                    .contains(&InvalidationReason::IncludePathChanged)
        }));
    }

    #[test]
    fn observed_autoload_like_lookup_edge_uses_resolver_version() {
        let unit = sample_unit();
        let inputs = DependencyPlannerInputs {
            observed_lookups: vec![ObservedLookup {
                kind: "class".to_owned(),
                name: "App\\Thing".to_owned(),
                target: Some("tests/fixtures/App/Thing.php".to_owned()),
                resolver_fingerprint: "autoload-v1".to_owned(),
            }],
            ..DependencyPlannerInputs::default()
        };
        let report = plan_dependency_units_with_inputs(&unit, &inputs);
        assert!(report.graph.units.iter().any(|unit| {
            unit.kind == DependencyUnitKind::AutoloadResolver && unit.name == "observed:autoload-v1"
        }));
        assert!(
            report.graph.edges.iter().any(|edge| {
                edge.kind == DependencyEdgeKind::UsesAutoloadResolver && edge.trusted
            })
        );
    }

    #[test]
    fn changed_file_fingerprint_changes_report_component() {
        let dir =
            std::env::temp_dir().join(format!("phrust-dependency-units-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("dep.php");
        fs::write(&path, "<?php function a() {}\n").expect("write first");
        let first = unit_with_file(path.to_string_lossy().as_ref());
        let first_report =
            plan_dependency_units_with_inputs(&first, &DependencyPlannerInputs::default());
        fs::write(&path, "<?php function b() {}\n").expect("write second");
        let second = unit_with_file(path.to_string_lossy().as_ref());
        let second_report =
            plan_dependency_units_with_inputs(&second, &DependencyPlannerInputs::default());

        assert_ne!(
            first_report.fingerprint_component(),
            second_report.fingerprint_component()
        );
        assert!(
            second_report
                .invalidation_by_reason
                .contains_key(InvalidationReason::SourceContentChanged.as_str())
        );
    }

    #[test]
    fn missing_include_negative_lookup_is_not_trusted_forever() {
        let unit = sample_unit();
        let inputs = DependencyPlannerInputs {
            observed_includes: vec![ObservedIncludeTarget {
                expression: "missing.php".to_owned(),
                targets: Vec::new(),
                include_path_fingerprint: "include-path-v1".to_owned(),
            }],
            ..DependencyPlannerInputs::default()
        };
        let report = plan_dependency_units_with_inputs(&unit, &inputs);

        assert!(report.graph.edges.iter().any(|edge| {
            edge.kind == DependencyEdgeKind::NegativeIncludeLookup
                && !edge.trusted
                && edge
                    .invalidation_reasons
                    .contains(&InvalidationReason::NegativeLookupExpired)
        }));
    }

    fn sample_unit() -> php_ir::IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(1));
        let file = builder.add_file("tests/fixtures/performance/bytecode_cache/simple.php");
        let span = IrSpan::new(file, 0, 10);
        let main = builder.start_function("main", FunctionFlags::default(), span);
        builder.register_function_name("main", main);
        let block = builder.append_block(main);
        let include_path = builder.add_constant(IrConstant::String(
            "tests/fixtures/performance/bytecode_cache/functions.php".to_owned(),
        ));
        let reg = builder.alloc_register(main);
        builder.emit(
            main,
            block,
            InstructionKind::Include {
                dst: reg,
                kind: IncludeKind::RequireOnce,
                path: Operand::Constant(include_path),
            },
            span,
        );
        builder.terminate_return(main, block, Some(Operand::Register(reg)), span);
        let method = builder.start_function(
            "App\\Thing::run",
            FunctionFlags {
                is_method: true,
                ..FunctionFlags::default()
            },
            span,
        );
        let class = ClassEntry {
            id: ClassId::new(0),
            name: "App\\Thing".to_owned(),
            display_name: "App\\Thing".to_owned(),
            parent: Some("App\\Base".to_owned()),
            parent_display_name: Some("App\\Base".to_owned()),
            interfaces: vec!["App\\Contract".to_owned()],
            methods: vec![ClassMethodEntry {
                name: "run".to_owned(),
                origin_class: "App\\Thing".to_owned(),
                function: method,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            }],
            properties: vec![ClassPropertyEntry {
                name: "value".to_owned(),
                default: Some(include_path),
                default_class_constant: None,
                default_named_constant: None,
                default_expr: None,
                type_: None,
                flags: ClassPropertyFlags::default(),
                hooks: ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor: None,
            flags: ClassFlags::default(),
            span,
        };
        builder.push_class(class);
        builder.finish()
    }

    fn unit_with_file(path: &str) -> php_ir::IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(2));
        let file = builder.add_file(path);
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("main", FunctionFlags::default(), span);
        builder.register_function_name("main", function);
        builder.finish()
    }
}
