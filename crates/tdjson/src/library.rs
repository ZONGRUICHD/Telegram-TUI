//! Load TDLib at runtime so the CLI, tests and compiler do not require a native SDK.
use libloading::Library;
use std::os::raw::{c_char, c_double, c_int};
use std::sync::OnceLock;

pub struct Api {
    _library: Library,
    pub create: unsafe extern "C" fn() -> c_int,
    pub send: unsafe extern "C" fn(c_int, *const c_char),
    pub receive: unsafe extern "C" fn(c_double) -> *const c_char,
    pub execute: unsafe extern "C" fn(*const c_char) -> *const c_char,
    pub verbosity: unsafe extern "C" fn(c_int),
}
static API: OnceLock<Api> = OnceLock::new();

pub fn load() -> anyhow::Result<()> {
    if API.get().is_some() {
        return Ok(());
    }
    let explicit = std::env::var_os("LIBTDJSON_PATH");
    let filename = if cfg!(windows) {
        "tdjson.dll"
    } else if cfg!(target_os = "macos") {
        "libtdjson.dylib"
    } else {
        "libtdjson.so"
    };
    let paths = if let Some(path) = explicit {
        vec![std::path::PathBuf::from(path)]
    } else {
        let mut paths = vec![std::env::current_exe()?.with_file_name(filename)];
        if cfg!(target_os = "macos") {
            paths.push(format!("/opt/homebrew/lib/{filename}").into());
            paths.push(format!("/usr/local/lib/{filename}").into());
        }
        paths.push(filename.into());
        paths
    };
    let mut failures = Vec::new();
    for path in paths {
        // Symbols are copied while the owning Library stays alive for the process lifetime.
        let result: anyhow::Result<Api> = unsafe {
            (|| {
                let library = Library::new(&path)?;
                Ok(Api {
                    create: *library.get(b"td_create_client_id\0")?,
                    send: *library.get(b"td_send\0")?,
                    receive: *library.get(b"td_receive\0")?,
                    execute: *library.get(b"td_execute\0")?,
                    verbosity: *library.get(b"td_set_log_verbosity_level\0")?,
                    _library: library,
                })
            })()
        };
        match result {
            Ok(api) => {
                let _ = API.set(api);
                return Ok(());
            }
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }
    anyhow::bail!(
        "无法加载 TDLib。安装官方 TDLib，或将 LIBTDJSON_PATH 指向动态库完整路径。\n{}",
        failures.join("\n")
    )
}

pub fn api() -> &'static Api {
    API.get()
        .expect("call load_library before TDLib operations")
}
