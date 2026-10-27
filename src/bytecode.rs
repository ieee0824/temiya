//! Portable stack bytecode and its mandatory pre-execution verifier.

use std::{collections::VecDeque, fmt, rc::Rc};

use crate::runtime::JsString;

mod compiler;
mod vm;
pub use compiler::{BytecodeCompiler, CompileError};
pub use vm::VirtualMachine;

/// Environment category instantiated by a scope entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeKind {
    /// The supplied Realm's persistent global environment.
    Global,
    /// A caller-prepared function environment.
    Function,
    /// A fresh declarative block environment.
    Block,
}

/// Binding initialization and assignment policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKind {
    /// Hoisted, mutable and initially undefined.
    Var,
    /// Mutable, initially in the temporal dead zone.
    Let,
    /// Immutable, initially in the temporal dead zone.
    Const,
}

/// One immutable binding entry; its position is the scope's slot operand.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeBinding {
    /// Decoded binding name.
    pub name: JsString,
    /// Initialization and mutability policy.
    pub kind: BindingKind,
}

/// One hoisted function and the active scope entry containing its binding.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeFunction {
    /// Declared binding name.
    pub name: JsString,
    /// Scope entry PC owning the binding (may differ from the capture scope).
    pub target: u32,
    /// Verified function code and metadata.
    pub template: Rc<FunctionTemplate>,
}

/// Immutable scope metadata, containing no runtime environment or values.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeDescriptor {
    /// Concrete environment category.
    pub kind: ScopeKind,
    /// Assignment strictness of this code.
    pub strict: bool,
    /// Ordered bindings addressed by slot operands.
    pub bindings: Vec<ScopeBinding>,
    /// Selected hoisted functions in source order (last duplicate wins).
    pub functions: Vec<ScopeFunction>,
}

/// Immutable executable function metadata; contains no runtime environment.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionTemplate {
    /// Explicit function name, if any.
    pub name: Option<JsString>,
    /// Whether an explicit expression name creates a private self binding.
    pub self_binding: bool,
    /// Whether ordinary call instantiation creates an arguments object.
    pub arguments_object: bool,
    /// Simple identifier parameters, in source order.
    pub parameters: Vec<JsString>,
    /// Whether this function uses lexical this/arguments/newTarget.
    pub arrow: bool,
    /// Whether the function is strict code.
    pub strict: bool,
    /// Independently verified executable function body.
    pub body: VerifiedBytecode,
}

/// Pointer-free literal stored in a bytecode constant pool.
#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    /// ECMAScript `undefined`.
    Undefined,
    /// ECMAScript `null`.
    Null,
    /// Boolean literal.
    Boolean(bool),
    /// IEEE-754 binary64 number.
    Number(f64),
    /// Exact UTF-16 string, including lone surrogates.
    String(JsString),
    /// Arbitrary-precision integer literal.
    BigInt(num_bigint::BigInt),
    /// Scope metadata, usable only by `EnterScope`, never `LoadConstant`.
    Scope(ScopeDescriptor),
    /// Immutable nested code, usable only by CreateFunction.
    Function(Rc<FunctionTemplate>),
}

/// Supported numeric unary operations; conversion semantics are shared with AST.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryNumericOperator {
    /// Unary plus (ToNumber; rejects BigInt).
    Plus,
    /// Number/BigInt negation.
    Minus,
    /// Number ToInt32 / BigInt bitwise complement.
    BitNot,
}

impl UnaryNumericOperator {
    pub(crate) fn spelling(self) -> &'static str {
        match self {
            Self::Plus => "+",
            Self::Minus => "-",
            Self::BitNot => "~",
        }
    }
}

/// Non-short-circuit binary operations shared with the AST runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOperator {
    /// JavaScript `+`.
    Add,
    /// JavaScript `-`.
    Subtract,
    /// JavaScript `*`.
    Multiply,
    /// JavaScript `/`.
    Divide,
    /// JavaScript `%`.
    Remainder,
    /// JavaScript `**`.
    Power,
    /// JavaScript `&`.
    BitAnd,
    /// JavaScript `|`.
    BitOr,
    /// JavaScript `^`.
    BitXor,
    /// JavaScript `<<`.
    LeftShift,
    /// JavaScript `>>`.
    RightShift,
    /// JavaScript `>>>`.
    UnsignedRightShift,
    /// JavaScript `===`.
    StrictEqual,
    /// JavaScript `!==`.
    StrictNotEqual,
    /// JavaScript `==`.
    Equal,
    /// JavaScript `!=`.
    NotEqual,
    /// JavaScript `<`.
    Less,
    /// JavaScript `>`.
    Greater,
    /// JavaScript `<=`.
    LessEqual,
    /// JavaScript `>=`.
    GreaterEqual,
    /// JavaScript `in`.
    In,
    /// JavaScript `instanceof`.
    InstanceOf,
}
impl BinaryOperator {
    pub(crate) fn from_spelling(operator: &str) -> Option<Self> {
        Some(match operator {
            "+" => Self::Add,
            "-" => Self::Subtract,
            "*" => Self::Multiply,
            "/" => Self::Divide,
            "%" => Self::Remainder,
            "**" => Self::Power,
            "&" => Self::BitAnd,
            "|" => Self::BitOr,
            "^" => Self::BitXor,
            "<<" => Self::LeftShift,
            ">>" => Self::RightShift,
            ">>>" => Self::UnsignedRightShift,
            "===" => Self::StrictEqual,
            "!==" => Self::StrictNotEqual,
            "==" => Self::Equal,
            "!=" => Self::NotEqual,
            "<" => Self::Less,
            ">" => Self::Greater,
            "<=" => Self::LessEqual,
            ">=" => Self::GreaterEqual,
            "in" => Self::In,
            "instanceof" => Self::InstanceOf,
            _ => return None,
        })
    }
    pub(crate) fn spelling(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Remainder => "%",
            Self::Power => "**",
            Self::BitAnd => "&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::LeftShift => "<<",
            Self::RightShift => ">>",
            Self::UnsignedRightShift => ">>>",
            Self::StrictEqual => "===",
            Self::StrictNotEqual => "!==",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::Greater => ">",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
            Self::In => "in",
            Self::InstanceOf => "instanceof",
        }
    }
}

/// Portable instruction with fixed-width integer operands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    /// Instantiate metadata entry `index` and identify this scope by its entry PC.
    EnterScope(u32),
    /// Leave the innermost active scope, preserving the operand stack.
    ExitScope,
    /// Copy the current let-only block's bindings into a fresh sibling environment.
    /// The scope entry identity and operand stack are preserved.
    CloneLoopBindings,
    /// Push a value from an active scope entry PC and binding slot.
    LoadBinding { scope: u32, slot: u32 },
    /// Initialize an active lexical binding from the stack.
    InitializeBinding { scope: u32, slot: u32 },
    /// Assign an active binding from the stack, checking TDZ and mutability.
    StoreBinding { scope: u32, slot: u32 },
    /// Capture a statically resolved active binding as a Reference.
    ResolveBinding { scope: u32, slot: u32, strict: bool },
    /// Resolve a string constant in the current environment chain and push its value.
    LoadName(u32),
    /// Resolve a name before its assignment RHS and retain its Reference.
    ResolveName { name: u32, strict: bool },
    /// Read the innermost Reference without consuming it.
    LoadReference,
    /// Discard a retained Reference after logical assignment short-circuits.
    DiscardReference,
    /// Pop an assigned value and the innermost retained Reference, then PutValue.
    StoreReference,
    /// Pop base and key values; push GetValue with the original receiver.
    GetProperty { strict: bool },
    /// Pop base and key values; retain their property Reference before the RHS.
    ResolveProperty { strict: bool },
    /// Consume a retained Reference, update its Number/BigInt, push prefix/postfix result.
    UpdateReference { increment: bool, prefix: bool },
    /// Consume a retained Reference and implement typeof, including unresolvable names.
    TypeOfReference,
    /// Apply Realm-aware numeric coercion and replace the unary operand.
    UnaryNumeric(UnaryNumericOperator),
    /// Replace a value with its ECMAScript typeof string.
    TypeOf,
    /// Consume a retained Reference and push the deletion result.
    DeleteReference,
    /// Capture a value callee before evaluating its arguments.
    BeginCall,
    /// Consume a Reference, capturing its value and receiver before arguments.
    BeginReferenceCall,
    /// Consume a Reference and push receiver then callee, evaluating GetValue once.
    LoadReferenceCallTarget,
    /// Convert a value callee to an undefined-receiver call target pair.
    AsCallTarget,
    /// Capture a receiver/callee pair before arguments.
    BeginMethodCall,
    /// Capture a constructor value and its newTarget before arguments.
    BeginConstruct,
    /// Append one evaluated value to the innermost pending invocation.
    AppendArgument,
    /// Consume an iterable through the Realm iterator protocol and append arguments.
    SpreadArguments,
    /// Invoke the innermost captured callee and push its result.
    Invoke,
    /// Create a bytecode-backed closure capturing the current environment.
    CreateFunction(u32),
    /// Push the nearest non-arrow function newTarget, or undefined.
    LoadNewTarget,
    /// Push the current lexical this binding.
    LoadThis,
    /// Pop a condition and jump if null or undefined.
    JumpIfNullish(u32),
    /// Push constant-pool entry `index` (`+1`).
    LoadConstant(u32),
    /// Push local slot `index` (`+1`).
    LoadLocal(u32),
    /// Pop and write local slot `index` (`-1`).
    StoreLocal(u32),
    /// Duplicate the stack top (`+1`, requires one value).
    Duplicate,
    /// Discard the stack top (`-1`).
    Pop,
    /// Pop two operands and apply the shared binary runtime operation.
    Binary(BinaryOperator),
    /// Pop two operands and push their numeric sum (`-1`).
    Add,
    /// Pop two operands and push their numeric difference (`-1`).
    Subtract,
    /// Pop two operands and push their numeric product (`-1`).
    Multiply,
    /// Pop two operands and push their numeric quotient (`-1`).
    Divide,
    /// Pop two operands and push strict equality (`-1`).
    StrictEqual,
    /// Replace the stack top with its logical negation (`0`).
    LogicalNot,
    /// Continue at absolute instruction `target` (`0`).
    Jump(u32),
    /// Pop a condition and jump when false (`-1`).
    JumpIfFalse(u32),
    /// Pop and produce a function-return completion (`-1`, terminal).
    Return,
    /// Pop and produce a normal program completion (`-1`, terminal).
    Complete,
    /// Pop and produce a throw completion (`-1`, terminal).
    Throw,
}

impl Instruction {
    fn required_stack(self) -> usize {
        match self {
            Self::LoadReference | Self::DiscardReference | Self::LoadReferenceCallTarget => 0,
            Self::AsCallTarget => 1,
            Self::BeginMethodCall => 2,
            Self::CreateFunction(_)
            | Self::LoadNewTarget
            | Self::BeginReferenceCall
            | Self::Invoke
            | Self::LoadThis => 0,
            Self::BeginCall
            | Self::BeginConstruct
            | Self::AppendArgument
            | Self::SpreadArguments => 1,
            Self::GetProperty { .. } | Self::ResolveProperty { .. } => 2,
            Self::DeleteReference | Self::UpdateReference { .. } | Self::TypeOfReference => 0,
            Self::TypeOf | Self::UnaryNumeric(_) => 1,
            Self::JumpIfNullish(_) => 1,
            Self::EnterScope(_)
            | Self::ExitScope
            | Self::CloneLoopBindings
            | Self::ResolveBinding { .. }
            | Self::ResolveName { .. }
            | Self::LoadBinding { .. }
            | Self::LoadName(_) => 0,
            Self::InitializeBinding { .. } | Self::StoreBinding { .. } | Self::StoreReference => 1,
            Self::LoadConstant(_) | Self::LoadLocal(_) | Self::Jump(_) => 0,
            Self::StoreLocal(_)
            | Self::Duplicate
            | Self::Pop
            | Self::JumpIfFalse(_)
            | Self::LogicalNot
            | Self::Return
            | Self::Complete
            | Self::Throw => 1,
            Self::Binary(_)
            | Self::Add
            | Self::Subtract
            | Self::Multiply
            | Self::Divide
            | Self::StrictEqual => 2,
        }
    }

    fn stack_delta(self) -> isize {
        match self {
            Self::LoadReference => 1,
            Self::DiscardReference => 0,
            Self::LoadReferenceCallTarget => 2,
            Self::AsCallTarget => 1,
            Self::BeginMethodCall => -2,
            Self::BeginReferenceCall => 0,
            Self::BeginCall
            | Self::BeginConstruct
            | Self::AppendArgument
            | Self::SpreadArguments => -1,
            Self::CreateFunction(_)
            | Self::LoadNewTarget
            | Self::Invoke
            | Self::LoadThis
            | Self::DeleteReference
            | Self::UpdateReference { .. }
            | Self::TypeOfReference => 1,
            Self::GetProperty { .. } | Self::JumpIfNullish(_) => -1,
            Self::ResolveProperty { .. } => -2,
            Self::EnterScope(_)
            | Self::ExitScope
            | Self::CloneLoopBindings
            | Self::ResolveBinding { .. }
            | Self::ResolveName { .. } => 0,
            Self::LoadBinding { .. } | Self::LoadName(_) => 1,
            Self::InitializeBinding { .. } | Self::StoreBinding { .. } | Self::StoreReference => -1,
            Self::LoadConstant(_) | Self::LoadLocal(_) | Self::Duplicate => 1,
            Self::StoreLocal(_)
            | Self::Pop
            | Self::JumpIfFalse(_)
            | Self::Return
            | Self::Complete
            | Self::Throw => -1,
            Self::Binary(_)
            | Self::Add
            | Self::Subtract
            | Self::Multiply
            | Self::Divide
            | Self::StrictEqual => -1,
            Self::Jump(_) | Self::LogicalNot | Self::TypeOf | Self::UnaryNumeric(_) => 0,
        }
    }
}

/// Untrusted portable bytecode plus its constant pool and local-slot count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bytecode {
    /// Pointer-free literals referenced by `LoadConstant`.
    pub constants: Vec<Constant>,
    /// Instructions addressed by zero-based absolute indices.
    pub instructions: Vec<Instruction>,
    /// Number of local slots available to load and store instructions.
    pub local_count: u32,
}

impl Bytecode {
    /// Validates operands, stack discipline, joins, reachability, and exits.
    pub fn verify(self) -> Result<VerifiedBytecode, VerifyError> {
        verify(self)
    }
}

/// Bytecode proven safe for VM dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedBytecode {
    bytecode: Bytecode,
    max_stack: usize,
}

impl VerifiedBytecode {
    /// Returns the verified bytecode body.
    #[must_use]
    pub fn bytecode(&self) -> &Bytecode {
        &self.bytecode
    }

    /// Returns the maximum operand-stack depth required by reachable code.
    #[must_use]
    pub fn max_stack(&self) -> usize {
        self.max_stack
    }

    /// Consumes the proof wrapper and returns the bytecode body.
    #[must_use]
    pub fn into_bytecode(self) -> Bytecode {
        self.bytecode
    }
}

/// Stable verifier failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyErrorKind {
    /// No instruction exists at the entry point.
    Empty,
    /// A constant-pool operand is outside the pool.
    ConstantOutOfBounds,
    /// A local-slot operand is outside the declared frame.
    LocalOutOfBounds,
    /// A branch target is not an instruction boundary.
    JumpOutOfBounds,
    /// An instruction would pop more values than are available.
    StackUnderflow,
    /// Two control-flow edges reach an instruction at different depths.
    StackDepthMismatch,
    /// Execution can fall beyond the final instruction.
    MissingReturn,
    /// A return does not consume the only remaining stack value.
    InvalidReturnDepth,
    /// An instruction is not reachable from entry.
    UnreachableInstruction,
    /// Scope metadata has an invalid kind, duplicate name or binding policy.
    InvalidScopeDescriptor,
    /// A metadata constant was used as a value, or a name operand is not a String.
    InvalidConstantKind,
    /// Scopes or retained References mismatch, underflow, or nest incorrectly.
    ScopeMismatch,
    /// A binding references an inactive scope, invalid slot, or var initialization.
    BindingOutOfBounds,
}

/// Verifier diagnostic carrying an instruction offset when available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyError {
    kind: VerifyErrorKind,
    offset: Option<usize>,
}

impl VerifyError {
    /// Returns the stable failure category.
    #[must_use]
    pub fn kind(&self) -> VerifyErrorKind {
        self.kind
    }

    /// Returns the offending instruction index, if one exists.
    #[must_use]
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }

    fn at(kind: VerifyErrorKind, offset: usize) -> Self {
        Self {
            kind,
            offset: Some(offset),
        }
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(
                formatter,
                "bytecode verification failed at {offset}: {:?}",
                self.kind
            ),
            None => write!(formatter, "bytecode verification failed: {:?}", self.kind),
        }
    }
}

impl std::error::Error for VerifyError {}

#[derive(Clone, Debug, PartialEq)]
struct FlowState {
    depth: usize,
    scopes: Vec<(u32, u32)>,
    references: Vec<u32>,
    calls: Vec<(u32, usize)>,
}

fn verify(bytecode: Bytecode) -> Result<VerifiedBytecode, VerifyError> {
    let length = bytecode.instructions.len();
    if length == 0 {
        return Err(VerifyError {
            kind: VerifyErrorKind::Empty,
            offset: None,
        });
    }
    for constant in &bytecode.constants {
        if let Constant::Scope(plan) = constant {
            let mut names = std::collections::HashSet::new();
            if plan.bindings.iter().any(|binding| {
                !names.insert(binding.name.code_units())
                    || (plan.kind == ScopeKind::Block && binding.kind == BindingKind::Var)
            }) {
                return Err(VerifyError {
                    kind: VerifyErrorKind::InvalidScopeDescriptor,
                    offset: None,
                });
            }
        }
    }
    let mut states: Vec<Option<FlowState>> = vec![None; length];
    let mut work = VecDeque::from([(
        0_usize,
        FlowState {
            depth: 0,
            scopes: vec![],
            references: vec![],
            calls: vec![],
        },
    )]);
    let mut max_stack = 0;
    while let Some((offset, mut state)) = work.pop_front() {
        if let Some(previous) = &states[offset] {
            if previous.depth != state.depth {
                return Err(VerifyError::at(VerifyErrorKind::StackDepthMismatch, offset));
            }
            if previous.scopes != state.scopes
                || previous.references != state.references
                || previous.calls != state.calls
            {
                return Err(VerifyError::at(VerifyErrorKind::ScopeMismatch, offset));
            }
            continue;
        }
        states[offset] = Some(state.clone());
        let instruction = bytecode.instructions[offset];
        let error = |kind| VerifyError::at(kind, offset);
        let constant = |index: u32| {
            bytecode
                .constants
                .get(index as usize)
                .ok_or_else(|| error(VerifyErrorKind::ConstantOutOfBounds))
        };
        match instruction {
            Instruction::EnterScope(index) => {
                let Constant::Scope(plan) = constant(index)? else {
                    return Err(error(VerifyErrorKind::InvalidConstantKind));
                };
                if (plan.kind == ScopeKind::Block) == state.scopes.is_empty()
                    || state
                        .scopes
                        .iter()
                        .any(|(entry, _)| *entry as usize == offset)
                {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
                let entry =
                    u32::try_from(offset).map_err(|_| error(VerifyErrorKind::ScopeMismatch))?;
                state.scopes.push((entry, index));
                let mut function_names = std::collections::HashSet::new();
                for declaration in &plan.functions {
                    validate_function_template(&declaration.template).map_err(error)?;
                    if declaration.template.arrow
                        || declaration.template.self_binding
                        || declaration.template.name.as_ref() != Some(&declaration.name)
                        || !function_names.insert(declaration.name.code_units())
                    {
                        return Err(error(VerifyErrorKind::InvalidScopeDescriptor));
                    }
                    let (_, target) = state
                        .scopes
                        .iter()
                        .find(|(entry, _)| *entry == declaration.target)
                        .ok_or_else(|| error(VerifyErrorKind::BindingOutOfBounds))?;
                    let Constant::Scope(target) = &bytecode.constants[*target as usize] else {
                        unreachable!()
                    };
                    if declaration.target != entry
                        && (plan.kind != ScopeKind::Block || target.kind != ScopeKind::Function)
                    {
                        return Err(error(VerifyErrorKind::ScopeMismatch));
                    }
                    let expected = if target.kind == ScopeKind::Block {
                        BindingKind::Let
                    } else {
                        BindingKind::Var
                    };
                    if !target
                        .bindings
                        .iter()
                        .any(|binding| binding.name == declaration.name && binding.kind == expected)
                    {
                        return Err(error(VerifyErrorKind::BindingOutOfBounds));
                    }
                }
            }
            Instruction::ExitScope => {
                state
                    .scopes
                    .pop()
                    .ok_or_else(|| error(VerifyErrorKind::ScopeMismatch))?;
            }
            Instruction::CloneLoopBindings => {
                let (_, index) = state
                    .scopes
                    .last()
                    .ok_or_else(|| error(VerifyErrorKind::ScopeMismatch))?;
                let Constant::Scope(plan) = &bytecode.constants[*index as usize] else {
                    unreachable!()
                };
                if plan.kind != ScopeKind::Block
                    || !plan.functions.is_empty()
                    || plan.bindings.iter().any(|b| b.kind != BindingKind::Let)
                {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
            }
            Instruction::LoadBinding { scope, slot }
            | Instruction::InitializeBinding { scope, slot }
            | Instruction::StoreBinding { scope, slot }
            | Instruction::ResolveBinding { scope, slot, .. } => {
                let (_, index) = state
                    .scopes
                    .iter()
                    .find(|(entry, _)| *entry == scope)
                    .ok_or_else(|| error(VerifyErrorKind::BindingOutOfBounds))?;
                let Constant::Scope(plan) = &bytecode.constants[*index as usize] else {
                    unreachable!()
                };
                if matches!(instruction, Instruction::ResolveBinding { .. })
                    && plan.kind == ScopeKind::Global
                {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
                let binding = plan
                    .bindings
                    .get(slot as usize)
                    .ok_or_else(|| error(VerifyErrorKind::BindingOutOfBounds))?;
                if matches!(instruction, Instruction::InitializeBinding { .. })
                    && binding.kind == BindingKind::Var
                {
                    return Err(error(VerifyErrorKind::BindingOutOfBounds));
                }
                if matches!(instruction, Instruction::ResolveBinding { .. }) {
                    state.references.push(
                        u32::try_from(offset).map_err(|_| error(VerifyErrorKind::ScopeMismatch))?,
                    );
                }
            }
            Instruction::LoadName(index) | Instruction::ResolveName { name: index, .. } => {
                if !matches!(constant(index)?, Constant::String(_)) {
                    return Err(error(VerifyErrorKind::InvalidConstantKind));
                }
                if state.scopes.is_empty() {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
                if matches!(instruction, Instruction::ResolveName { .. }) {
                    state.references.push(
                        u32::try_from(offset).map_err(|_| error(VerifyErrorKind::ScopeMismatch))?,
                    );
                }
            }
            Instruction::CreateFunction(index) => {
                let Constant::Function(template) = constant(index)? else {
                    return Err(error(VerifyErrorKind::InvalidConstantKind));
                };
                if state.scopes.is_empty() {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
                validate_function_template(template).map_err(error)?;
            }
            Instruction::GetProperty { .. }
            | Instruction::ResolveProperty { .. }
            | Instruction::LoadThis
            | Instruction::LoadNewTarget => {
                if state.scopes.is_empty() {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
                if matches!(instruction, Instruction::ResolveProperty { .. }) {
                    state.references.push(
                        u32::try_from(offset).map_err(|_| error(VerifyErrorKind::ScopeMismatch))?,
                    );
                }
            }
            Instruction::LoadReferenceCallTarget => {
                state
                    .references
                    .pop()
                    .ok_or_else(|| error(VerifyErrorKind::ScopeMismatch))?;
            }
            Instruction::BeginCall
            | Instruction::BeginMethodCall
            | Instruction::BeginConstruct
            | Instruction::BeginReferenceCall => {
                if matches!(instruction, Instruction::BeginReferenceCall) {
                    state
                        .references
                        .pop()
                        .ok_or_else(|| error(VerifyErrorKind::ScopeMismatch))?;
                }
                let baseline = state
                    .depth
                    .checked_sub(instruction.required_stack())
                    .ok_or_else(|| error(VerifyErrorKind::StackUnderflow))?;
                state.calls.push((
                    u32::try_from(offset).map_err(|_| error(VerifyErrorKind::ScopeMismatch))?,
                    baseline,
                ));
            }
            Instruction::AppendArgument | Instruction::SpreadArguments | Instruction::Invoke => {
                if matches!(instruction, Instruction::SpreadArguments) && state.scopes.is_empty() {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
                let (_, baseline) = state
                    .calls
                    .last()
                    .ok_or_else(|| error(VerifyErrorKind::ScopeMismatch))?;
                let expected = baseline
                    + usize::from(matches!(
                        instruction,
                        Instruction::AppendArgument | Instruction::SpreadArguments
                    ));
                if state.depth != expected {
                    return Err(error(VerifyErrorKind::StackDepthMismatch));
                }
                if matches!(instruction, Instruction::Invoke) {
                    state.calls.pop();
                }
            }
            Instruction::LoadReference => {
                if state.references.is_empty() {
                    return Err(error(VerifyErrorKind::ScopeMismatch));
                }
            }
            Instruction::DiscardReference
            | Instruction::StoreReference
            | Instruction::DeleteReference
            | Instruction::UpdateReference { .. }
            | Instruction::TypeOfReference => {
                state
                    .references
                    .pop()
                    .ok_or_else(|| error(VerifyErrorKind::ScopeMismatch))?;
            }
            Instruction::LoadConstant(index) => {
                if matches!(constant(index)?, Constant::Scope(_) | Constant::Function(_)) {
                    return Err(error(VerifyErrorKind::InvalidConstantKind));
                }
            }
            Instruction::LoadLocal(index) | Instruction::StoreLocal(index)
                if index >= bytecode.local_count =>
            {
                return Err(error(VerifyErrorKind::LocalOutOfBounds));
            }
            Instruction::Jump(target)
            | Instruction::JumpIfFalse(target)
            | Instruction::JumpIfNullish(target)
                if target as usize >= length =>
            {
                return Err(error(VerifyErrorKind::JumpOutOfBounds));
            }
            Instruction::Complete if !state.scopes.is_empty() || !state.references.is_empty() => {
                return Err(error(VerifyErrorKind::ScopeMismatch));
            }
            _ => {}
        }
        if matches!(
            instruction,
            Instruction::Return | Instruction::Complete | Instruction::Throw
        ) && !state.calls.is_empty()
        {
            return Err(error(VerifyErrorKind::ScopeMismatch));
        }
        if state.depth < instruction.required_stack() {
            return Err(error(VerifyErrorKind::StackUnderflow));
        }
        if matches!(
            instruction,
            Instruction::Return | Instruction::Complete | Instruction::Throw
        ) && state.depth != 1
        {
            return Err(error(VerifyErrorKind::InvalidReturnDepth));
        }
        max_stack = max_stack.max(state.depth);
        state.depth = state
            .depth
            .checked_add_signed(instruction.stack_delta())
            .expect("stack requirement prevents underflow");
        if state
            .calls
            .iter()
            .any(|(_, baseline)| state.depth < *baseline)
        {
            return Err(error(VerifyErrorKind::StackUnderflow));
        }
        max_stack = max_stack.max(state.depth);
        match instruction {
            Instruction::Return | Instruction::Complete | Instruction::Throw => {}
            Instruction::Jump(target) => work.push_back((target as usize, state)),
            Instruction::JumpIfFalse(target) | Instruction::JumpIfNullish(target) => {
                work.push_back((target as usize, state.clone()));
                enqueue_fallthrough(offset, length, state, &mut work)?;
            }
            _ => enqueue_fallthrough(offset, length, state, &mut work)?,
        }
    }
    if let Some(offset) = states.iter().position(Option::is_none) {
        return Err(VerifyError::at(
            VerifyErrorKind::UnreachableInstruction,
            offset,
        ));
    }
    Ok(VerifiedBytecode {
        bytecode,
        max_stack,
    })
}

fn enqueue_fallthrough(
    offset: usize,
    length: usize,
    state: FlowState,
    work: &mut VecDeque<(usize, FlowState)>,
) -> Result<(), VerifyError> {
    let next = offset + 1;
    if next == length {
        Err(VerifyError::at(VerifyErrorKind::MissingReturn, offset))
    } else {
        work.push_back((next, state));
        Ok(())
    }
}

fn validate_function_template(template: &FunctionTemplate) -> Result<(), VerifyErrorKind> {
    let body = template.body.bytecode();
    let Some(Instruction::EnterScope(index)) = body.instructions.first() else {
        return Err(VerifyErrorKind::InvalidScopeDescriptor);
    };
    if !matches!(body.constants.get(*index as usize), Some(Constant::Scope(plan)) if plan.kind == ScopeKind::Function && plan.strict == template.strict)
    {
        return Err(VerifyErrorKind::InvalidScopeDescriptor);
    }
    if template.arrow && (template.self_binding || template.arguments_object) {
        return Err(VerifyErrorKind::InvalidScopeDescriptor);
    }
    if template.strict || template.arrow {
        let mut names = std::collections::HashSet::new();
        if template
            .parameters
            .iter()
            .any(|name| !names.insert(name.code_units()))
        {
            return Err(VerifyErrorKind::InvalidScopeDescriptor);
        }
    }
    Ok(())
}
