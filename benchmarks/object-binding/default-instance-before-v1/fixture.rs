use temiya::{parser::parse_script,runtime::{AllocationContext,IntrinsicId,Object,Realm,StatementEvaluator,Value,WellKnownSymbol}};
#[test]
fn default_has_instance_errors_retain_the_builtin_defining_realm() {
 let context=AllocationContext::new();let realm=Realm::ordinary_in(&context);
 StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script("function C(){}C.prototype=0;").unwrap()).unwrap();
 let constructor=realm.global_object().get(&"C".into()).unwrap();
 let Value::Object(method)=realm.intrinsic(IntrinsicId::FunctionPrototype).get(&realm.well_known_symbol(WellKnownSymbol::HasInstance).into()).unwrap() else{panic!("method")};
 let error=method.call(constructor,&[Value::Object(Object::new_in(&context))]).unwrap_err();
 let Some(Value::Object(exception))=error.thrown_value() else {panic!("default @@hasInstance must materialize TypeError in its defining Realm")};
 assert_eq!(exception.try_prototype().unwrap(),Some(realm.intrinsic(IntrinsicId::TypeErrorPrototype)));
}
