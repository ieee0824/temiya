use std::{ffi::CStr, ffi::c_void, ptr};

use temiya::ffi::*;

#[test]
fn version_compile_evaluate_and_cleanup() {
    assert_eq!(temiya_abi_version(), TEMIYA_ABI_VERSION);
    let mut engine = ptr::null_mut();
    let mut realm = ptr::null_mut();
    let mut script = ptr::null_mut();
    let mut value = ptr::null_mut();
    let mut error = ptr::null_mut();
    let source = c"40 + 2;";
    // SAFETY: Every output slot is writable and handles remain live until cleanup.
    unsafe {
        assert_eq!(temiya_engine_new(&mut engine, &mut error), TemiyaStatus::Ok);
        assert_eq!(
            temiya_realm_new(engine, &mut realm, &mut error),
            TemiyaStatus::Ok
        );
        assert_eq!(
            temiya_script_compile(engine, realm, source.as_ptr(), &mut script, &mut error),
            TemiyaStatus::Ok
        );
        assert_eq!(
            temiya_script_evaluate(engine, script, &mut value, &mut error),
            TemiyaStatus::Ok
        );
        let mut number = 0.0;
        assert_eq!(temiya_value_to_number(value, &mut number), TemiyaStatus::Ok);
        assert_eq!(number, 42.0);
        temiya_error_free(error);
        temiya_value_free(value);
        temiya_script_free(script);
        temiya_realm_free(realm);
        temiya_engine_free(engine);
    }
}

#[test]
fn invalid_utf8_and_compile_errors_are_owned() {
    let mut engine = ptr::null_mut();
    let mut realm = ptr::null_mut();
    let mut script = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: Pointers and cleanup follow the API ownership contract.
    unsafe {
        assert_eq!(temiya_engine_new(&mut engine, &mut error), TemiyaStatus::Ok);
        assert_eq!(
            temiya_realm_new(engine, &mut realm, &mut error),
            TemiyaStatus::Ok
        );
        let invalid = [0xff_u8, 0];
        assert_eq!(
            temiya_script_compile(
                engine,
                realm,
                invalid.as_ptr().cast(),
                &mut script,
                &mut error
            ),
            TemiyaStatus::InvalidArgument
        );
        assert!(!error.is_null());
        assert!(
            !CStr::from_ptr(temiya_error_message(error))
                .to_bytes()
                .is_empty()
        );
        temiya_error_free(error);
        error = ptr::null_mut();
        assert_eq!(
            temiya_script_compile(engine, realm, c"let = ;".as_ptr(), &mut script, &mut error),
            TemiyaStatus::Exception
        );
        assert!(
            CStr::from_ptr(temiya_error_message(error))
                .to_string_lossy()
                .contains("compile failed")
        );
        temiya_error_free(error);
        temiya_realm_free(realm);
        temiya_engine_free(engine);
    }
}

#[test]
fn null_arguments_are_rejected_without_dereference() {
    // SAFETY: Null pointers are explicitly accepted for validation/free paths.
    unsafe {
        assert_eq!(
            temiya_engine_new(ptr::null_mut(), ptr::null_mut()),
            TemiyaStatus::InvalidArgument
        );
        assert_eq!(temiya_engine_free(ptr::null_mut()), TemiyaStatus::Ok);
        assert_eq!(temiya_error_free(ptr::null_mut()), TemiyaStatus::Ok);
    }
}

#[test]
fn double_free_and_stale_handles_are_rejected() {
    let mut engine = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: The registry validates each pointer before dereferencing it.
    unsafe {
        assert_eq!(temiya_engine_new(&mut engine, &mut error), TemiyaStatus::Ok);
        assert_eq!(temiya_engine_free(engine), TemiyaStatus::Ok);
        assert_eq!(temiya_engine_free(engine), TemiyaStatus::InvalidArgument);
        let mut realm = ptr::null_mut();
        assert_eq!(
            temiya_realm_new(engine, &mut realm, &mut error),
            TemiyaStatus::InvalidArgument
        );
    }
}

struct Reentry {
    engine: *mut TemiyaEngine,
    realm: *mut TemiyaRealm,
}

unsafe extern "C" fn reentrant_callback(
    user_data: *mut c_void,
    input: f64,
    output: *mut f64,
) -> TemiyaStatus {
    // SAFETY: The test keeps this context and all handles live for the call.
    let context = unsafe { &*(user_data.cast::<Reentry>()) };
    let mut script = ptr::null_mut();
    let mut value = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: All C ABI pointer contracts are satisfied by the fixture.
    unsafe {
        let status = temiya_script_compile(
            context.engine,
            context.realm,
            c"1 + 1;".as_ptr(),
            &mut script,
            &mut error,
        );
        if status != TemiyaStatus::Ok {
            return status;
        }
        let status = temiya_script_evaluate(context.engine, script, &mut value, &mut error);
        let mut nested = 0.0;
        if status == TemiyaStatus::Ok {
            let _ = temiya_value_to_number(value, &mut nested);
            *output = input + nested;
        }
        let _ = temiya_error_free(error);
        let _ = temiya_value_free(value);
        let _ = temiya_script_free(script);
        status
    }
}

#[test]
fn c_callback_can_reenter_compile_and_evaluate() {
    let mut engine = ptr::null_mut();
    let mut realm = ptr::null_mut();
    let mut script = ptr::null_mut();
    let mut value = ptr::null_mut();
    let mut error = ptr::null_mut();
    // SAFETY: Every handle and callback context remains live through cleanup.
    unsafe {
        assert_eq!(temiya_engine_new(&mut engine, &mut error), TemiyaStatus::Ok);
        assert_eq!(
            temiya_realm_new(engine, &mut realm, &mut error),
            TemiyaStatus::Ok
        );
        let mut context = Reentry { engine, realm };
        assert_eq!(
            temiya_realm_define_number_function(
                engine,
                realm,
                c"host".as_ptr(),
                Some(reentrant_callback),
                (&raw mut context).cast(),
                &mut error,
            ),
            TemiyaStatus::Ok
        );
        assert_eq!(
            temiya_script_compile(
                engine,
                realm,
                c"host(40);".as_ptr(),
                &mut script,
                &mut error
            ),
            TemiyaStatus::Ok
        );
        assert_eq!(
            temiya_script_evaluate(engine, script, &mut value, &mut error),
            TemiyaStatus::Ok
        );
        let mut number = 0.0;
        assert_eq!(temiya_value_to_number(value, &mut number), TemiyaStatus::Ok);
        assert_eq!(number, 42.0);
        let _ = temiya_value_free(value);
        let _ = temiya_script_free(script);
        let _ = temiya_realm_free(realm);
        let _ = temiya_engine_free(engine);
    }
}
