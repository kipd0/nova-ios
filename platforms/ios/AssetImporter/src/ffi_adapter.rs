//! Included directly into both RustBridge and GameRuntime staticlibs.
//! One ABI, same local installer, regardless of which host library is selected.
use std::{ffi::{c_char, CStr}, path::Path};

fn c_text<'a>(ptr: *const c_char) -> Result<&'a str, String> {
    if ptr.is_null() { return Err("Missing argument".into()); }
    let bytes = unsafe { CStr::from_ptr(ptr) };
    let text = bytes.to_str().map_err(|_| "Argument is not valid UTF-8".to_string())?;
    if text.is_empty() { return Err("Empty argument".into()); }
    Ok(text)
}

fn c_path<'a>(ptr: *const c_char) -> Result<&'a Path, String> {
    c_text(ptr).map(Path::new)
}

fn error_to_c(buffer: *mut c_char, capacity: usize, message: &str) {
    if buffer.is_null() || capacity == 0 { return; }
    // Buffer and capacity are owned by the Swift caller.
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer as *mut u8, capacity.min(4096)) };
    let size = message.len().min(buffer.len() - 1);
    buffer[..size].copy_from_slice(&message.as_bytes()[..size]);
    buffer[size] = 0;
}

/// Returns 0 for structurally validated installation, -1 on an error.
/// Extracts only the user-provided archive into the app's own container.
#[unsafe(no_mangle)]
pub extern "C" fn nova_ios_install_resources_zip(
    zip: *const c_char,
    target: *const c_char,
    error: *mut c_char,
    error_capacity: usize,
) -> i32 {
    let result = std::panic::catch_unwind(|| {
        let source = c_path(zip)?;
        let destination = c_path(target)?;
        nova_asset_importer::install_zip(source, destination)
            .map_err(|e| e.to_string())
    });
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(message)) => { error_to_c(error, error_capacity, &message); -1 }
        Err(_) => { error_to_c(error, error_capacity, "Archive importer failed"); -1 }
    }
}

/// Indicates only whether the prepared resource directory passes structural
/// checks, not whether game launch or upstream version compatibility works.
#[unsafe(no_mangle)]
pub extern "C" fn nova_ios_resources_present(path: *const c_char) -> bool {
    std::panic::catch_unwind(|| {
        c_path(path).is_ok_and(|p| nova_asset_importer::validate_resources(p).is_ok())
    }).unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn nova_ios_scan_mods(
    home: *const c_char,
    error: *mut c_char,
    capacity: usize,
) -> *mut c_char {
    let result = std::panic::catch_unwind(|| -> Result<_, String> {
        let json = nova_asset_importer::mods::scan_json(c_path(home)?)
            .map_err(|e| e.to_string())?;
        std::ffi::CString::new(json).map_err(|e| e.to_string())
    });
    match result {
        Ok(Ok(json)) => json.into_raw(),
        Ok(Err(message)) => { error_to_c(error, capacity, &message); std::ptr::null_mut() }
        Err(_) => { error_to_c(error, capacity, "Mod scan failed"); std::ptr::null_mut() }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nova_ios_free_mod_list(json: *mut c_char) {
    if !json.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(json) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn nova_ios_set_mod_enabled(
    home: *const c_char,
    id: *const c_char,
    enabled: bool,
    error: *mut c_char,
    capacity: usize,
) -> bool {
    let result = std::panic::catch_unwind(|| {
        let home = c_path(home)?;
        let id = c_text(id)?;
        nova_asset_importer::mods::set_enabled(home, id, enabled).map_err(|e| e.to_string())
    });
    match result {
        Ok(Ok(())) => true,
        Ok(Err(message)) => { error_to_c(error, capacity, &message); false }
        Err(_) => { error_to_c(error, capacity, "Could not save mod settings"); false }
    }
}
