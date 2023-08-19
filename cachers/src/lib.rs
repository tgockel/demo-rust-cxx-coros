use std::{cell::Cell, ffi, fmt, mem, ptr, sync::{Arc, Mutex, MutexGuard}, ops::{Deref, DerefMut}};

use bytes::Bytes;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(C)]
#[must_use]
#[non_exhaustive]
pub enum ErrorCode {
    Ok = 0,
    NotImplemented,
    InvalidArgument,
    Empty,
    HasData,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self, f)
    }
}

pub struct Error {
    code: ErrorCode,
    message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "code={} message=\"{}\"", self.code(), self.message())
    }
}

impl std::error::Error for Error {}

thread_local! {
    static CURRENT_ERROR: Cell<Option<Error>> = Cell::new(None);
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let mut message = message.into();
        message.push('\0');
        Self { code, message }
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn save_to_thread_local(self) {
        CURRENT_ERROR.with(|r| {
            r.set(Some(self))
        })
    }

    pub fn take_thread_local() -> Option<Self> {
        CURRENT_ERROR.with(|r| {
            r.take()
        })
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[no_mangle]
pub extern "C" fn cachers_current_errstr() -> *const ffi::c_char {
    CURRENT_ERROR.with(|r| {
        match r.take() {
            None => ptr::null(),
            Some(err) => {
                // conversion is safe because `Error::new` adds NUL
                let out = err.message.as_ptr() as *const _;
                r.set(Some(err));
                out
            }
        }
    })
}

fn wrap_err_call<F>(f: F) -> ErrorCode
    where F: FnOnce() -> Result<()>
{
    match f() {
        Ok(()) => {
            Error::take_thread_local();
            ErrorCode::Ok
        }
        Err(e) => {
            let code = e.code();
            e.save_to_thread_local();
            code
        }
    }
}

struct NonNullAligned<T> {
    inner: ptr::NonNull<T>,
}

impl<T> NonNullAligned<T> {
    pub fn from_arg(name: &str, src: *mut T) -> Result<Self> {
        let Some(inner) = ptr::NonNull::new(src) else {
            return Err(Error::new(ErrorCode::InvalidArgument, format!("pointer `{name}` is null")));
        };
        if src.align_offset(mem::align_of::<T>()) != 0 {
            return Err(Error::new(ErrorCode::InvalidArgument, format!("pointer `{name}` appears misaligned")));
        }

        Ok(Self { inner })
    }

    pub fn as_ref(&self) -> &T {
        // safety: checked at construction time
        unsafe { self.inner.as_ref() }
    }

    pub fn as_mut(&mut self) -> &mut T {
        // safety: checked at construction time
        unsafe { self.inner.as_mut() }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.inner.as_ptr()
    }
}

macro_rules! non_null_arg {
    ($arg:ident) => {
        let $arg = NonNullAligned::from_arg(stringify!($arg), $arg)?;
    };
    (mut $arg:ident) => {
        let mut $arg = NonNullAligned::from_arg(stringify!($arg), $arg)?;
    };
    ($arg:ident: [$typ:ty; $arg_len:ident]) => {
        let $arg = NonNullAligned::from_arg(stringify!($arg), $arg as *mut $typ)?;
        let $arg = unsafe { std::slice::from_raw_parts($arg.as_ptr(), $arg_len) };
    };
}

trait NativeArc: Sized {
    fn from_native_take(src: NonNullAligned<Self>) -> Arc<Self> {
        unsafe { Arc::from_raw(src.as_ref()) }
    }

    fn from_native(src: NonNullAligned<Self>) -> Arc<Self> {
        let out = Self::from_native_take(src);
        mem::forget(out.clone());
        out
    }

    fn into_native(self: Arc<Self>) -> *mut Self {
        Arc::into_raw(self) as *mut _
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Interesting Code Starts Here                                                                                       //
////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// The database.
pub struct Database {
    _requests: Mutex<Option<i32>>,
}

impl Database {
    pub fn new() -> Result<Arc<Self>> {
        Ok(Arc::new(Database { _requests: Default::default() }))
    }

    pub(crate) fn from_native_take(src: NonNullAligned<Self>) -> Arc<Self> {
        unsafe { Arc::from_raw(src.as_ref()) }
    }

    pub(crate) fn from_native(src: NonNullAligned<Self>) -> Arc<Self> {
        let out = Self::from_native_take(src);
        mem::forget(out.clone());
        out
    }

    pub(crate) fn into_native(self: Arc<Self>) -> *mut Self {
        Arc::into_raw(self) as *mut _
    }

    pub(crate) fn get(self: &Arc<Self>, key: &[u8]) -> Arc<ResponseInner> {
        let response = ResponseInner {
            header: Bytes::copy_from_slice(key),
            data: Mutex::new(ResponseInnerData::Some(Bytes::copy_from_slice(key))),
        };
        Arc::new(response)
    }
}

#[no_mangle]
pub extern "C" fn cachers_open(out: *mut *mut Database) -> ErrorCode {
    wrap_err_call(|| {
        non_null_arg!(mut out);

        let db = Database::new()?;
        *out.as_mut() = db.into_native();
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn cachers_release(db: *mut Database) -> ErrorCode {
    wrap_err_call(|| {
        non_null_arg!(db);
        let db = Database::from_native_take(db);
        let db_weak = Arc::downgrade(&db);
        drop(db);

        if let Some(_) = db_weak.upgrade() {
            println!("Database released while still in use!");
        }

        Ok(())
    })
}

pub struct ResponseInner {
    header: Bytes,
    data: Mutex<ResponseInnerData>,
}

enum ResponseInnerData {
    None,
    Some(Bytes),
    Callback {
        func: unsafe extern "C" fn(response: *const ResponseInfo, cxt: *mut ffi::c_void),
        context: *mut ffi::c_void,
    }
}

impl NativeArc for ResponseInner {}

#[repr(C)]
pub struct ResponseInfo {
    token: *mut ResponseInner,
    error_code: ErrorCode,
    header: *const ffi::c_void,
    header_size: usize,
    data_state: DataState,
    data: *const ffi::c_void,
    data_size: usize,
}

impl ResponseInfo {
    fn from_locked(value: Arc<ResponseInner>, datalock: MutexGuard<'_, ResponseInnerData>) -> Self {
        let data = match datalock.deref() {
            ResponseInnerData::None => None,
            ResponseInnerData::Some(b) => Some(b),
            _ => todo!(),
        };
        let mut out = ResponseInfo {
            token: ptr::null_mut(),
            error_code: ErrorCode::Ok,
            header: value.header.as_ptr() as *const _,
            header_size: value.header.len(),
            data_state: DataState::Complete,
            data: data.map_or(ptr::null(), |x| x.as_ptr() as *const _),
            data_size: data.map_or(0, |x| x.len()),
        };
        drop(datalock);
        out.token = ResponseInner::into_native(value);
        out
    }
}

impl From<Arc<ResponseInner>> for ResponseInfo {
    fn from(value: Arc<ResponseInner>) -> Self {
        // HACK
        Self::from_locked(value.clone(), value.data.lock().unwrap())
        /*
        let datalock = value.data.lock().unwrap();
        let data = match datalock.deref() {
            ResponseInnerData::None => None,
            ResponseInnerData::Some(b) => Some(b),
            _ => todo!(),
        };
        let mut out = ResponseInfo {
            token: ptr::null_mut(),
            error_code: ErrorCode::Ok,
            header: value.header.as_ptr() as *const _,
            header_size: value.header.len(),
            data_state: DataState::Complete,
            data: data.map_or(ptr::null(), |x| x.as_ptr() as *const _),
            data_size: data.map_or(0, |x| x.len()),
        };
        drop(datalock);
        out.token = ResponseInner::into_native(value);
        out
        */
    }
}

#[repr(C)]
pub enum DataState {
    /// No data is associated -- it will never arrive.
    None,
    /// The data has been fetched.
    Complete,
    InProgress,
    Error,
}

#[no_mangle]
pub extern "C" fn cachers_get(
    db: *mut Database,
    key: *const ffi::c_void,
    key_len: usize,
    out: *mut ResponseInfo
) -> ErrorCode {
    wrap_err_call(|| {
        non_null_arg!(db);
        non_null_arg!(key: [u8; key_len]);
        non_null_arg!(mut out);

        let db = Database::from_native(db);
        let resp = db.get(key);

        *out.as_mut() = ResponseInfo::from(resp);

        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn cachers_response_get_or_bind(
    token: *mut ResponseInner,
    callback: Option<unsafe extern "C" fn(response: *const ResponseInfo, cxt: *mut ffi::c_void)>,
    callback_cxt: *mut ffi::c_void,
    maybe_out: *mut ResponseInfo,
) -> ErrorCode {
    wrap_err_call(|| {
        non_null_arg!(token);
        let Some(callback) = callback else {
            return Err(Error::new(ErrorCode::InvalidArgument, "`callback` can not be null"))
        };
        non_null_arg!(mut maybe_out);

        let response = ResponseInner::from_native(token);
        let mut datalock = response.data.lock().unwrap();
        match datalock.deref() {
            ResponseInnerData::None => {
                *datalock.deref_mut() = ResponseInnerData::Callback { func: callback, context: callback_cxt };
                return Ok(());
            }
            ResponseInnerData::Some(_) => {
                // HACK
                *maybe_out.as_mut() = ResponseInfo::from_locked(response.clone(), datalock);
                return Ok(())
            }
            _ => todo!(),
        }
    })
}

#[no_mangle]
pub extern "C" fn cachers_response_token_release(
    token: *mut ResponseInner
) -> ErrorCode {
    wrap_err_call(|| {
        non_null_arg!(token);
        drop(ResponseInner::from_native_take(token));
        Ok(())
    })
}
