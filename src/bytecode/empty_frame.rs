//! Conservative proof that no observable binding requires a function environment.
use super::{Bytecode, Constant, Instruction, ScopeKind};
use crate::ast::ExprKind;

pub(super) fn proven(code: &Bytecode) -> bool {
    let Some(Instruction::EnterScope(index)) = code.instructions.first() else {
        return false;
    };
    if !matches!(&code.constants[*index as usize], Constant::Scope(scope) if scope.kind == ScopeKind::Function)
    {
        return false;
    }
    for constant in &code.constants {
        match constant {
            Constant::Scope(scope) if !scope.bindings.is_empty() || !scope.functions.is_empty() => {
                return false;
            }
            Constant::Class { .. } | Constant::Function(_) => return false,
            Constant::ObjectLiteral { definition, .. } => {
                let mut observable = false;
                definition.visit(&mut |expression| {
                    observable |= match &expression.kind {
                        ExprKind::This
                        | ExprKind::Super
                        | ExprKind::MetaProperty { .. }
                        | ExprKind::Arrow(_)
                        | ExprKind::Class(_)
                        | ExprKind::ImportCall { .. } => true,
                        ExprKind::Identifier(name) => name == "arguments" || name == "eval",
                        ExprKind::Function(function) | ExprKind::Method { function, .. } => {
                            function.asynchronous || function.generator
                        }
                        ExprKind::Literal(_)
                        | ExprKind::Object(_)
                        | ExprKind::Array { .. }
                        | ExprKind::Parenthesized(_)
                        | ExprKind::Unary { .. }
                        | ExprKind::Binary { .. }
                        | ExprKind::Assignment { .. }
                        | ExprKind::Sequence(_)
                        | ExprKind::Member { .. }
                        | ExprKind::Call { .. }
                        | ExprKind::New { .. }
                        | ExprKind::Spread(_)
                        | ExprKind::Update { .. }
                        | ExprKind::Conditional { .. }
                        | ExprKind::Template { .. }
                        | ExprKind::PrivateIdentifier(_) => false,
                        // New metadata forms require an explicit proof update.
                        _ => true,
                    };
                });
                if observable {
                    return false;
                }
            }
            Constant::Scope(_)
            | Constant::Undefined
            | Constant::Null
            | Constant::Boolean(_)
            | Constant::Number(_)
            | Constant::String(_)
            | Constant::BigInt(_) => {}
        }
    }
    for instruction in &code.instructions {
        match instruction {
            Instruction::LoadNewTarget
            | Instruction::LoadSuperBinding
            | Instruction::ResolveSuperProperty { .. }
            | Instruction::CreateFunction(_)
            | Instruction::CreateClass(_) => return false,
            Instruction::LoadName(index)
            | Instruction::GetName { name: index, .. }
            | Instruction::ResolveName { name: index, .. } => {
                if matches!(&code.constants[*index as usize], Constant::String(name)
                    if name == &super::JsString::from("arguments") || name == &super::JsString::from("eval"))
                {
                    return false;
                }
            }
            Instruction::LoadThis
            | Instruction::CreateObject
            | Instruction::CoercePropertyKey
            | Instruction::DefineObjectProperty { .. }
            | Instruction::DefineNamedObjectProperty { .. }
            | Instruction::SpreadObject
            | Instruction::CreateArray
            | Instruction::AppendArray
            | Instruction::ElideArray
            | Instruction::SpreadArray
            | Instruction::FinishArray
            | Instruction::EnterScope(_)
            | Instruction::ExitScope
            | Instruction::LoadReference
            | Instruction::DiscardReference
            | Instruction::StoreReference
            | Instruction::GetProperty { .. }
            | Instruction::GetNamedProperty { .. }
            | Instruction::ResolveProperty { .. }
            | Instruction::ResolveNamedProperty { .. }
            | Instruction::UpdateReference { .. }
            | Instruction::TypeOfReference
            | Instruction::UnaryNumeric(_)
            | Instruction::TypeOf
            | Instruction::DeleteReference
            | Instruction::BeginCall
            | Instruction::BeginReferenceCall
            | Instruction::LoadReferenceCallTarget
            | Instruction::AsCallTarget
            | Instruction::BeginMethodCall
            | Instruction::BeginConstruct
            | Instruction::AppendArgument
            | Instruction::SpreadArguments
            | Instruction::Invoke
            | Instruction::CreateObjectLiteral(_)
            | Instruction::JumpIfNullish(_)
            | Instruction::LoadConstant(_)
            | Instruction::LoadLocal(_)
            | Instruction::StoreLocal(_)
            | Instruction::Duplicate
            | Instruction::Pop
            | Instruction::Binary(_)
            | Instruction::Add
            | Instruction::Subtract
            | Instruction::Multiply
            | Instruction::Divide
            | Instruction::StrictEqual
            | Instruction::LogicalNot
            | Instruction::Jump(_)
            | Instruction::JumpIfFalse(_)
            | Instruction::Return
            | Instruction::Complete
            | Instruction::Throw => {}
            // Binding operations and future instructions are not implicitly safe.
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::{ast::StmtKind, bytecode::BytecodeCompiler, parser::parse_script};
    fn eligible(source: &str) -> bool {
        let program = parse_script(source).unwrap();
        let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
            panic!("function");
        };
        BytecodeCompiler::compile_function_template(function)
            .unwrap()
            .body
            .empty_function_frame
    }
    #[test]
    fn accepts_captured_bindings_and_fresh_method_definitions() {
        assert!(eligible("function f(){return this;}"));
        assert!(eligible(
            "function f(){if(state!==0)throw 42;state=1;reads++;return current>n;}"
        ));
        assert!(eligible(
            "function f(){return {next(){return 0;},return(){throw 99;}};}"
        ));
    }
    #[test]
    fn rejects_activation_observers_in_instructions_and_nested_metadata() {
        for source in [
            "function f(){return new.target;}",
            "function f(){return arguments;}",
            "function f(){let local=1;return local;}",
            "function f(x){return x;}",
            "function f(){return {m(){return ()=>this;}};}",
            "function f(){return {m(){return arguments;}};}",
            "function f(){return {m(){return eval('1');}};}",
            "function f(){return function(){return 1;};}",
        ] {
            assert!(!eligible(source), "{source}");
        }
    }
}
