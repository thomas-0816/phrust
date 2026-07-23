//! IR functions and local/register metadata.

use crate::block::BasicBlock;
use crate::constants::IrConstant;
use crate::ids::LocalId;

/// Returns whether a local name belongs to an IR-only compiler slot.
///
/// PHP variable names cannot contain `:`, so the lowering-owned namespace
/// cannot collide with a PHP-visible local. These slots are frame-local even
/// when they are emitted in a top-level function.
#[must_use]
pub fn is_compiler_generated_local_name(name: &str) -> bool {
    name.starts_with("__phrust:")
}
use crate::module::AttributeEntry;
use crate::source_map::IrSpan;
use serde::{Deserialize, Serialize};

/// Minimal runtime type family enforced by the runtime-type VM.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrReturnType {
    /// `int`
    Int,
    /// `float`
    Float,
    /// `string`
    String,
    /// `array`
    Array,
    /// `callable`
    Callable,
    /// `iterable`
    Iterable,
    /// `object`
    Object,
    /// `bool`
    Bool,
    /// `null`
    Null,
    /// `void`
    Void,
    /// `mixed`
    Mixed,
    /// `never`
    Never,
    /// Literal `false` type.
    False,
    /// Literal `true` type.
    True,
    /// Class-like return type. Runtime object checking is a known gap until
    /// object storage exists.
    Class {
        /// Normalized lookup name.
        name: String,
        /// Source-spelled type name for PHP-visible diagnostics/reflection.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
    },
    /// Nullable simple type from `?T` or normalized `T|null`.
    Nullable { inner: Box<IrReturnType> },
    /// Union type in source order.
    Union { members: Vec<IrReturnType> },
    /// Intersection type in source order.
    Intersection { members: Vec<IrReturnType> },
    /// Disjunctive-normal-form type in source order.
    Dnf { members: Vec<IrReturnType> },
}

/// Function parameter metadata.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IrParam {
    /// Parameter name without `$`.
    pub name: String,
    /// Local slot assigned to the parameter.
    pub local: LocalId,
    /// True when callers must pass this positional argument.
    pub required: bool,
    /// Constant-pool default value for omitted optional arguments.
    pub default: Option<IrConstant>,
    /// Optional Semantic frontend lowered runtime type enforced by the VM MVP.
    pub type_: Option<IrReturnType>,
    /// True when the callee aliases the caller argument into this parameter.
    pub by_ref: bool,
    /// True when this parameter collects remaining positional arguments.
    pub variadic: bool,
    /// Attribute metadata attached to this parameter declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Closure capture metadata stored on a synthesized closure function.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IrCapture {
    /// Captured variable name without `$`.
    pub name: String,
    /// Local slot initialized from the closure value before parameters.
    pub local: LocalId,
    /// True when the closure capture aliases the source local's reference cell.
    pub by_ref: bool,
}

/// Function shape flags.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FunctionFlags {
    /// True for the synthesized top-level script function.
    pub is_top_level: bool,
    /// True for closures.
    pub is_closure: bool,
    /// True for methods.
    pub is_method: bool,
    /// True for static closures.
    pub is_static: bool,
    /// True when the function body contains `yield` or `yield from`.
    pub is_generator: bool,
}

/// IR function body.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IrFunction {
    /// Function name or synthesized top-level name.
    pub name: String,
    /// Parameters in declaration order.
    pub params: Vec<IrParam>,
    /// Local slot names without the leading `$`, indexed by `LocalId`.
    pub locals: Vec<String>,
    /// Number of local slots.
    pub local_count: u32,
    /// Number of registers.
    pub register_count: u32,
    /// Basic blocks.
    pub blocks: Vec<BasicBlock>,
    /// Source span for the function declaration/body.
    pub span: IrSpan,
    /// Function flags.
    pub flags: FunctionFlags,
    /// Optional declared return type enforced by the VM MVP.
    pub return_type: Option<IrReturnType>,
    /// True when the function declaration uses `function &name()`.
    pub returns_by_ref: bool,
    /// Closure capture locals in deterministic declaration/discovery order.
    pub captures: Vec<IrCapture>,
    /// Attribute metadata attached to this function-like declaration.
    pub attributes: Vec<AttributeEntry>,
}

impl IrFunction {
    /// Creates a function shell.
    #[must_use]
    pub fn new(name: impl Into<String>, flags: FunctionFlags, span: IrSpan) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            locals: Vec::new(),
            local_count: 0,
            register_count: 0,
            blocks: Vec::new(),
            span,
            flags,
            return_type: None,
            returns_by_ref: false,
            captures: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// Returns the native entry slot for a Closure's bindable `$this`.
    ///
    /// Captures are allocated before other Closure locals, so `$this` is not
    /// necessarily local zero. A captured outer `$this` is already carried by
    /// the capture list and must not also become an implicit entry operand.
    #[must_use]
    pub fn implicit_closure_this_local(&self) -> Option<LocalId> {
        if !self.flags.is_closure || self.flags.is_static {
            return None;
        }
        let local = self
            .locals
            .iter()
            .position(|name| name == "this")
            .and_then(|index| u32::try_from(index).ok())
            .map(LocalId::new)?;
        (!self.captures.iter().any(|capture| capture.local == local)).then_some(local)
    }
}
