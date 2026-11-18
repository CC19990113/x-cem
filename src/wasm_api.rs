//! wasm32-unknown-unknown 导出层。
//!
//! 手写 `extern "C"`，不引 wasm-bindgen、不引任何 crate；只复用核心的纯函数
//! [`crate::module_for_source`]。
//!
//! # ABI 约定（Node 侧依赖，勿改）
//!
//! 1. 调用方先 `xcem_alloc(len)` 分配缓冲区并拷入 UTF-8 字节（path / src 各一块）。
//! 2. `xcem_scan(path_ptr, path_len, src_ptr, src_len)` 解析**单个源文件**：
//!    - 返回 `0`：成功，结果 JSON 写入内部缓冲区；
//!    - 返回 `1`：失败，原因用 `xcem_last_error_ptr` / `xcem_last_error_len` 取。
//! 3. 结果用 `xcem_result_ptr()` + `xcem_result_len()` 取出（长度是**字节数**）。
//!    每次 `xcem_scan` 覆盖上一次结果；内部缓冲区由 `Vec<u8>` 持有，不泄漏。
//! 4. 结果 JSON 是**该文件的 module 对象**，形状为
//!    `{"kind":"javascript-module","path":"...","declarations":[...]}`，
//!    不带 `schemaVersion` 外壳（与 `Manifest` 的完整 JSON 不同）。
//! 5. 调用方用完 path / src 缓冲区后调用 `xcem_free(ptr, len)`，`len` 必须
//!    与 `xcem_alloc` 时一致。
//!
//! 内部状态是单线程的：wasm32-unknown-unknown 下用 `Mutex` 包住 `Vec<u8>`，
//! 同一时刻只保存最近一次结果 / 错误。

use std::sync::Mutex;

/// 最近一次成功 scan 的 JSON 字节。
static RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
/// 最近一次错误的 UTF-8 文案（成功时清空）。
static LAST_ERROR: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// 分配 `len` 字节缓冲区，返回其指针。
///
/// 内存来自 `Vec<u8>`（capacity == len），由调用方用 [`xcem_free`] 归还。
#[no_mangle]
pub extern "C" fn xcem_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::<u8>::dangling().as_ptr();
    }
    let mut buffer = vec![0u8; len];
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// 释放 [`xcem_alloc`] 分配的缓冲区；`len` 必须与分配时一致。
///
/// # Safety
///
/// `ptr` / `len` 必须来自同一次 `xcem_alloc`，且只能释放一次。
#[no_mangle]
pub extern "C" fn xcem_free(ptr: *mut u8, len: usize) {
    if len == 0 || ptr.is_null() {
        return;
    }
    // SAFETY: 由调用方保证 ptr/len 来自同一次 xcem_alloc。
    unsafe {
        drop(Vec::from_raw_parts(ptr, len, len));
    }
}

/// 解析单个源文件。成功返回 `0`，失败返回 `1`。
///
/// 成功时结果 JSON 存入内部缓冲区，用 [`xcem_result_ptr`] /
/// [`xcem_result_len`] 读取；失败时用 [`xcem_last_error_ptr`] /
/// [`xcem_last_error_len`] 读取错误文案。
#[no_mangle]
pub extern "C" fn xcem_scan(
    path_ptr: *const u8,
    path_len: usize,
    src_ptr: *const u8,
    src_len: usize,
) -> u32 {
    // SAFETY: 调用方保证指针与长度来自 xcem_alloc 分配的、已写入的缓冲区。
    let path = match unsafe { str_from_raw(path_ptr, path_len) } {
        Ok(path) => path,
        Err(message) => {
            set_last_error(message);
            return 1;
        }
    };
    // SAFETY: 同上。
    let source = match unsafe { str_from_raw(src_ptr, src_len) } {
        Ok(source) => source,
        Err(message) => {
            set_last_error(message);
            return 1;
        }
    };

    let json = crate::module_for_source(path, source)
        .to_json_object()
        .into_bytes();

    match RESULT.lock() {
        Ok(mut result) => *result = json,
        Err(_) => {
            set_last_error("内部结果缓冲区状态异常");
            return 1;
        }
    }
    if let Ok(mut error) = LAST_ERROR.lock() {
        error.clear();
    }
    0
}

/// 结果缓冲区指针；`xcem_scan` 返回 0 后有效，下次 scan 前有效。
#[no_mangle]
pub extern "C" fn xcem_result_ptr() -> *mut u8 {
    match RESULT.lock() {
        Ok(mut result) => result.as_mut_ptr(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 结果长度（字节数）。
#[no_mangle]
pub extern "C" fn xcem_result_len() -> usize {
    match RESULT.lock() {
        Ok(result) => result.len(),
        Err(_) => 0,
    }
}

/// 最近一次错误文案的指针（UTF-8，无结尾 NUL）。
#[no_mangle]
pub extern "C" fn xcem_last_error_ptr() -> *mut u8 {
    match LAST_ERROR.lock() {
        Ok(mut error) => error.as_mut_ptr(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 最近一次错误文案的长度（字节数）。
#[no_mangle]
pub extern "C" fn xcem_last_error_len() -> usize {
    match LAST_ERROR.lock() {
        Ok(error) => error.len(),
        Err(_) => 0,
    }
}

fn set_last_error(message: &str) {
    if let Ok(mut error) = LAST_ERROR.lock() {
        error.clear();
        error.extend_from_slice(message.as_bytes());
    }
}

/// # Safety
///
/// `ptr` 必须指向至少 `len` 字节的可读内存（或 `len == 0`）。
unsafe fn str_from_raw<'a>(ptr: *const u8, len: usize) -> Result<&'a str, &'static str> {
    let bytes: &[u8] = if len == 0 {
        &[]
    } else {
        if ptr.is_null() {
            return Err("输入指针为空");
        }
        // SAFETY: 由调用方保证 ptr 指向 len 字节可读内存。
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    std::str::from_utf8(bytes).map_err(|_| "输入不是合法 UTF-8")
}
