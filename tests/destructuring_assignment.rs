use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}: {source}"
        );
    }
}

#[test]
fn assignment_preserves_rhs_identity_and_writes_nested_members() {
    check(
        "let a=0,obj={},rest;let input=[20,{value:22},40,42];let result=([a,{value:obj.answer},...rest]=input);a+obj.answer===42&&rest[1]===42&&result===input;",
    );
    check(
        "let obj={},rest;let sym=Symbol();let input={value:42,other:20,[sym]:22};let result=({value:obj.answer,...rest}=input);obj.answer===42&&rest.other+rest[sym]===42&&result===input;",
    );
}
#[test]
fn target_reference_precedes_iterator_step_and_default() {
    check(
        "let order='',obj={};let iterable={[Symbol.iterator](){order+='I';return {next(){order+='N';return {value:undefined,done:false};},return(){order+='C';return {};}};}};function target(){order+='T';return obj;}[target().value=(order+='D',42)]=iterable;obj.value===42&&order==='ITNDC';",
    );
}
#[test]
fn object_key_target_get_default_put_order() {
    check(
        "let order='',obj={set answer(v){order+='P';if(v!==42)throw 1;}};function target(){order+='T';return obj;}let input={get value(){order+='G';return undefined;}};({[(order+='K','value')]:target().answer=(order+='D',42)}=input);order==='KTGDP';",
    );
}
#[test]
fn abrupt_put_preserves_partial_writes_and_closes_iterator() {
    check(
        "let a=0,closed=0,marker={},caught=false;let obj={set value(v){throw marker;}};let iterable={[Symbol.iterator](){return {next(){return {value:42,done:false};},return(){closed++;throw 99;}};}};try{[a,obj.value]=iterable;}catch(e){caught=e===marker;}a===42&&closed===1&&caught;",
    );
}
#[test]
fn for_of_assignment_targets_and_nested_defaults() {
    check(
        "let obj={},a=0;for([a,obj.value] of [[20,22],[40,2]]){}let b=0;({missing:[b]=[42]}={});a+obj.value===42&&b===42;",
    );
}
