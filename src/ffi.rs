//! Versioned C ABI backed by opaque, explicitly owned handles.

use std::{
    collections::HashSet,
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::{Mutex, OnceLock},
};

use crate::embedding::{CompiledScript, Engine, Exception, HostError, RealmHandle, ValueHandle};
use crate::runtime::Value;

/// ABI version implemented by this library and `include/temiya.h`.
pub const TEMIYA_ABI_VERSION: u32 = 1;

/// Stable status codes returned by fallible C ABI calls.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemiyaStatus {
    /// Operation succeeded.
    Ok = 0,
    /// A required pointer, UTF-8 string, or output slot was invalid.
    InvalidArgument = 1,
    /// JavaScript compilation or evaluation failed.
    Exception = 2,
    /// A Rust panic was caught at the ABI boundary.
    Panic = 3,
}

/// Opaque engine handle owned by C.
pub struct TemiyaEngine(Engine);
/// Opaque Realm handle owned by C.
pub struct TemiyaRealm(RealmHandle);
/// Opaque compiled Script handle owned by C.
pub struct TemiyaScript(CompiledScript);
/// Opaque JavaScript value handle owned by C.
pub struct TemiyaValue(ValueHandle);
/// Opaque error handle owned by C.
pub struct TemiyaError(CString);

/// C callback used by a numeric host function.
pub type TemiyaNumberCallback = unsafe extern "C" fn(*mut c_void, f64, *mut f64) -> TemiyaStatus;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum HandleKind {
    Engine,
    Realm,
    Script,
    Value,
    Error,
}

fn handles() -> &'static Mutex<HashSet<(HandleKind, usize)>> {
    static HANDLES: OnceLock<Mutex<HashSet<(HandleKind, usize)>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn register<T>(kind: HandleKind, value: T) -> *mut T {
    let pointer = Box::into_raw(Box::new(value));
    handles()
        .lock()
        .expect("handle registry mutex poisoned")
        .insert((kind, pointer as usize));
    pointer
}

fn is_live<T>(kind: HandleKind, pointer: *const T) -> bool {
    !pointer.is_null()
        && handles()
            .lock()
            .expect("handle registry mutex poisoned")
            .contains(&(kind, pointer as usize))
}

unsafe fn release<T>(kind: HandleKind, pointer: *mut T) -> TemiyaStatus {
    if pointer.is_null() {
        return TemiyaStatus::Ok;
    }
    if !handles()
        .lock()
        .expect("handle registry mutex poisoned")
        .remove(&(kind, pointer as usize))
    {
        return TemiyaStatus::InvalidArgument;
    }
    // SAFETY: The registry proves this allocation is live and uniquely released.
    unsafe { drop(Box::from_raw(pointer)) };
    TemiyaStatus::Ok
}

fn message(text: impl ToString) -> CString {
    CString::new(text.to_string().replace('\0', "\\0")).expect("NUL bytes were replaced")
}

unsafe fn set_error(slot: *mut *mut TemiyaError, text: impl ToString) {
    if !slot.is_null() {
        // SAFETY: The caller promises that a non-null output slot is writable.
        unsafe { *slot = register(HandleKind::Error, TemiyaError(message(text))) };
    }
}

fn boundary(
    error: *mut *mut TemiyaError,
    operation: impl FnOnce() -> Result<(), (TemiyaStatus, String)>,
) -> TemiyaStatus {
    if !error.is_null() {
        // SAFETY: Fallible ABI entry points require a writable error output slot.
        unsafe { *error = ptr::null_mut() };
    }
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => TemiyaStatus::Ok,
        Ok(Err((status, text))) => {
            // SAFETY: `error` was validated by the entry point.
            unsafe { set_error(error, text) };
            status
        }
        Err(_) => {
            // SAFETY: `error` was validated by the entry point.
            unsafe { set_error(error, "panic caught at Temiya C ABI boundary") };
            TemiyaStatus::Panic
        }
    }
}

/// Returns the ABI version without allocating.
#[unsafe(no_mangle)]
pub extern "C" fn temiya_abi_version() -> u32 {
    TEMIYA_ABI_VERSION
}

/// Allocates an engine with default options.
///
/// # Safety
/// `out` must point to writable storage for one handle. `error` may be null or
/// point to writable storage. Successful handles must be freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_engine_new(
    out: *mut *mut TemiyaEngine,
    error: *mut *mut TemiyaError,
) -> TemiyaStatus {
    if out.is_null() {
        return TemiyaStatus::InvalidArgument;
    }
    // SAFETY: `out` is non-null and writable by contract.
    unsafe { *out = ptr::null_mut() };
    boundary(error, || {
        // SAFETY: Validated above.
        unsafe { *out = register(HandleKind::Engine, TemiyaEngine(Engine::default())) };
        Ok(())
    })
}

/// Frees an engine handle; null is accepted.
///
/// # Safety
/// `engine` must be null or a live handle returned by `temiya_engine_new`, and
/// must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_engine_free(engine: *mut TemiyaEngine) -> TemiyaStatus {
    // SAFETY: Registry validation precedes deallocation.
    unsafe { release(HandleKind::Engine, engine) }
}

/// Creates a Realm owned by `engine`.
///
/// # Safety
/// All input handles must be live, and `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_realm_new(
    engine: *mut TemiyaEngine,
    out: *mut *mut TemiyaRealm,
    error: *mut *mut TemiyaError,
) -> TemiyaStatus {
    if !is_live(HandleKind::Engine, engine) || out.is_null() {
        return TemiyaStatus::InvalidArgument;
    }
    // SAFETY: Validated pointers are live/writable by contract.
    unsafe { *out = ptr::null_mut() };
    boundary(error, || {
        // SAFETY: Validated above.
        let realm = unsafe { (*engine).0.create_realm() };
        // SAFETY: Validated above.
        unsafe { *out = register(HandleKind::Realm, TemiyaRealm(realm)) };
        Ok(())
    })
}

macro_rules! free_handle {
    ($name:ident, $ty:ty, $kind:expr) => {
        #[doc = "Frees an opaque handle; null is accepted.\n\n# Safety\nThe pointer must be null or a live handle of the matching type, and must not be reused."]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(handle: *mut $ty) -> TemiyaStatus {
            // SAFETY: Registry validation precedes deallocation.
            unsafe { release($kind, handle) }
        }
    };
}

free_handle!(temiya_realm_free, TemiyaRealm, HandleKind::Realm);
free_handle!(temiya_script_free, TemiyaScript, HandleKind::Script);
free_handle!(temiya_value_free, TemiyaValue, HandleKind::Value);
free_handle!(temiya_error_free, TemiyaError, HandleKind::Error);

/// Compiles a NUL-terminated UTF-8 Script.
///
/// # Safety
/// Handles and `source` must be live, and `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_script_compile(
    engine: *mut TemiyaEngine,
    realm: *mut TemiyaRealm,
    source: *const c_char,
    out: *mut *mut TemiyaScript,
    error: *mut *mut TemiyaError,
) -> TemiyaStatus {
    if !is_live(HandleKind::Engine, engine)
        || !is_live(HandleKind::Realm, realm)
        || source.is_null()
        || out.is_null()
    {
        return TemiyaStatus::InvalidArgument;
    }
    // SAFETY: Validated above.
    unsafe { *out = ptr::null_mut() };
    boundary(error, || {
        // SAFETY: `source` is a valid NUL-terminated string by contract.
        let source = unsafe { CStr::from_ptr(source) }
            .to_str()
            .map_err(|error| (TemiyaStatus::InvalidArgument, error.to_string()))?;
        // SAFETY: Handles are live by contract.
        let script = unsafe { (*engine).0.compile(&(*realm).0, source) }
            .map_err(|error| (TemiyaStatus::Exception, error.to_string()))?;
        // SAFETY: `out` is writable by contract.
        unsafe { *out = register(HandleKind::Script, TemiyaScript(script)) };
        Ok(())
    })
}

/// Defines a unary numeric C callback as a JavaScript global function.
///
/// The callback may reenter Temiya. Its `user_data` pointer is passed through
/// unchanged, and must remain valid until the Realm is freed.
///
/// # Safety
/// Handles and `name` must be live, `name` must be NUL-terminated UTF-8, and
/// `user_data` must satisfy the callback's own contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_realm_define_number_function(
    engine: *mut TemiyaEngine,
    realm: *mut TemiyaRealm,
    name: *const c_char,
    callback: Option<TemiyaNumberCallback>,
    user_data: *mut c_void,
    error: *mut *mut TemiyaError,
) -> TemiyaStatus {
    if !is_live(HandleKind::Engine, engine)
        || !is_live(HandleKind::Realm, realm)
        || name.is_null()
        || callback.is_none()
    {
        return TemiyaStatus::InvalidArgument;
    }
    boundary(error, || {
        // SAFETY: `name` is a valid C string by contract.
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .map_err(|error| (TemiyaStatus::InvalidArgument, error.to_string()))?;
        let callback = callback.expect("callback checked above");
        let user_data = user_data as usize;
        // SAFETY: Handles were registry-validated above.
        unsafe {
            (*engine)
                .0
                .register_global_function(&(*realm).0, name, move |context, _, arguments| {
                    let input = arguments
                        .first()
                        .ok_or_else(|| HostError::type_error("one number argument required"))?
                        .value()
                        .to_number()
                        .map_err(|error| HostError::type_error(error.to_string()))?;
                    let mut output = 0.0;
                    // SAFETY: The host owns the callback and user data lifetime.
                    let status = callback(user_data as *mut c_void, input, &mut output);
                    if status == TemiyaStatus::Ok {
                        Ok(context.value(Value::Number(output)))
                    } else {
                        Err(HostError::type_error(format!(
                            "C host callback failed with status {}",
                            status as u32
                        )))
                    }
                })
        }
        .map_err(|error| (TemiyaStatus::Exception, error.to_string()))?;
        Ok(())
    })
}

/// Evaluates a compiled Script and returns an owned value.
///
/// # Safety
/// Handles must be live and `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_script_evaluate(
    engine: *mut TemiyaEngine,
    script: *mut TemiyaScript,
    out: *mut *mut TemiyaValue,
    error: *mut *mut TemiyaError,
) -> TemiyaStatus {
    if !is_live(HandleKind::Engine, engine) || !is_live(HandleKind::Script, script) || out.is_null()
    {
        return TemiyaStatus::InvalidArgument;
    }
    // SAFETY: Validated above.
    unsafe { *out = ptr::null_mut() };
    boundary(error, || {
        // SAFETY: Handles are live by contract.
        let value = unsafe { (*engine).0.evaluate(&(*script).0) }
            .map_err(|error: Exception| (TemiyaStatus::Exception, error.to_string()))?;
        // SAFETY: `out` is writable by contract.
        unsafe { *out = register(HandleKind::Value, TemiyaValue(value)) };
        Ok(())
    })
}

/// Reads a Number value into `out`.
///
/// # Safety
/// `value` must be live and `out` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_value_to_number(
    value: *const TemiyaValue,
    out: *mut f64,
) -> TemiyaStatus {
    if !is_live(HandleKind::Value, value) || out.is_null() {
        return TemiyaStatus::InvalidArgument;
    }
    // SAFETY: Pointers are live/writable by contract.
    match unsafe { (*value).0.value() } {
        Value::Number(number) => {
            // SAFETY: Validated above.
            unsafe { *out = *number };
            TemiyaStatus::Ok
        }
        _ => TemiyaStatus::InvalidArgument,
    }
}

/// Returns a borrowed NUL-terminated error message.
///
/// # Safety
/// `error` must be a live error handle. The pointer is valid until it is freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn temiya_error_message(error: *const TemiyaError) -> *const c_char {
    if !is_live(HandleKind::Error, error) {
        return ptr::null();
    }
    // SAFETY: The handle is live by contract.
    unsafe { (*error).0.as_ptr() }
}
