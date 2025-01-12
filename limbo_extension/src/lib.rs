use std::ffi::CString;
use std::os::raw::{c_char, c_void};

pub type ResultCode = i32;

pub const RESULT_OK: ResultCode = 0;
pub const RESULT_ERROR: ResultCode = 1;
// TODO: more error types

pub type ExtensionEntryPoint = extern "C" fn(api: *const ExtensionApi) -> ResultCode;
pub type ScalarFunction = extern "C" fn(argc: i32, *const *const c_void) -> Value;

#[repr(C)]
pub struct ExtensionApi {
    pub ctx: *mut c_void,
    pub register_scalar_function:
        extern "C" fn(ctx: *mut c_void, name: *const c_char, func: ScalarFunction) -> ResultCode,
}

#[macro_export]
macro_rules! register_extension {
    (
        scalars: { $( $scalar_name:expr => $scalar_func:ident ),* $(,)? },
        //aggregates: { $( $agg_name:expr => ($step_func:ident, $finalize_func:ident) ),* $(,)? },
        //virtual_tables: { $( $vt_name:expr => $vt_impl:expr ),* $(,)? }
    ) => {
        #[no_mangle]
        pub unsafe extern "C" fn register_extension(api: *const $crate::ExtensionApi) -> $crate::ResultCode {
            if api.is_null() {
                return $crate::RESULT_ERROR;
            }

            register_scalar_functions! { api, $( $scalar_name => $scalar_func ),* }
            // TODO:
            //register_aggregate_functions! { $( $agg_name => ($step_func, $finalize_func) ),* }
            //register_virtual_tables! { $( $vt_name => $vt_impl ),* }
            $crate::RESULT_OK
        }
    }
}

#[macro_export]
macro_rules! register_scalar_functions {
    ( $api:expr, $( $fname:expr => $fptr:ident ),* ) => {
        unsafe {
            $(
                let cname = std::ffi::CString::new($fname).unwrap();
                ((*$api).register_scalar_function)((*$api).ctx, cname.as_ptr(), $fptr);
            )*
        }
    }
}

/// Provide a cleaner interface to define scalar functions to extension authors
/// . e.g.
/// ```
///  fn scalar_func(args: &[Value]) -> Value {
///     if args.len() != 1 {
///          return Value::null();
///     }
///      Value::from_integer(args[0].integer * 2)
///  }
///  ```
///
#[macro_export]
macro_rules! declare_scalar_functions {
    (
        $(
            #[args(min = $min_args:literal, max = $max_args:literal)]
            fn $func_name:ident ($args:ident : &[Value]) -> Value $body:block
        )*
    ) => {
        $(
            extern "C" fn $func_name(
                argc: i32,
                argv: *const *const std::os::raw::c_void
            ) -> $crate::Value {
                if !($min_args..=$max_args).contains(&argc) {
                    println!("{}: Invalid argument count", stringify!($func_name));
                    return $crate::Value::null();// TODO: error code
                }
                if argc == 0 || argv.is_null() {
                    let $args: &[$crate::Value] = &[];
                    $body
                } else {
                    unsafe {
                        let ptr_slice = std::slice::from_raw_parts(argv, argc as usize);
                        let mut values = Vec::with_capacity(argc as usize);
                        for &ptr in ptr_slice {
                            let val_ptr = ptr as *const $crate::Value;
                            if val_ptr.is_null() {
                                values.push($crate::Value::null());
                            } else {
                                values.push(std::ptr::read(val_ptr));
                            }
                        }
                        let $args: &[$crate::Value] = &values[..];
                        $body
                    }
                }
            }
        )*
    };
}

#[derive(PartialEq, Eq)]
#[repr(C)]
pub enum ValueType {
    Null,
    Integer,
    Float,
    Text,
    Blob,
}

// TODO: perf, these can be better expressed
#[repr(C)]
pub struct Value {
    pub value_type: ValueType,
    pub integer: i64,
    pub float: f64,
    pub text: TextValue,
    pub blob: Blob,
}

#[repr(C)]
pub struct TextValue {
    text: *const c_char,
    len: usize,
}

impl std::fmt::Display for TextValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.text.is_null() {
            return write!(f, "<null>");
        }
        let slice = unsafe { std::slice::from_raw_parts(self.text as *const u8, self.len) };
        match std::str::from_utf8(slice) {
            Ok(s) => write!(f, "{}", s),
            Err(e) => write!(f, "<invalid UTF-8: {:?}>", e),
        }
    }
}

impl TextValue {
    pub fn is_null(&self) -> bool {
        self.text.is_null()
    }

    pub fn new(text: *const c_char, len: usize) -> Self {
        Self { text, len }
    }

    pub fn null() -> Self {
        Self {
            text: std::ptr::null(),
            len: 0,
        }
    }
}

#[repr(C)]
pub struct Blob {
    pub data: *const u8,
    pub size: usize,
}

impl Blob {
    pub fn new(data: *const u8, size: usize) -> Self {
        Self { data, size }
    }
    pub fn null() -> Self {
        Self {
            data: std::ptr::null(),
            size: 0,
        }
    }
}

impl Value {
    pub fn null() -> Self {
        Self {
            value_type: ValueType::Null,
            integer: 0,
            float: 0.0,
            text: TextValue::null(),
            blob: Blob::null(),
        }
    }

    pub fn from_integer(value: i64) -> Self {
        Self {
            value_type: ValueType::Integer,
            integer: value,
            float: 0.0,
            text: TextValue::null(),
            blob: Blob::null(),
        }
    }
    pub fn from_float(value: f64) -> Self {
        Self {
            value_type: ValueType::Float,
            integer: 0,
            float: value,
            text: TextValue::null(),
            blob: Blob::null(),
        }
    }

    pub fn from_text(value: String) -> Self {
        let cstr = CString::new(&*value).unwrap();
        let ptr = cstr.as_ptr();
        let len = value.len();
        std::mem::forget(cstr);
        Self {
            value_type: ValueType::Text,
            integer: 0,
            float: 0.0,
            text: TextValue::new(ptr, len),
            blob: Blob::null(),
        }
    }

    pub fn from_blob(value: &[u8]) -> Self {
        Self {
            value_type: ValueType::Blob,
            integer: 0,
            float: 0.0,
            text: TextValue::null(),
            blob: Blob::new(value.as_ptr(), value.len()),
        }
    }
}
