use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/client.ico");
    println!("cargo:rerun-if-changed=assets/client.rc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let resource = out.join(if gnu { "client.o" } else { "client.res" });
    let status = if gnu {
        let compiler = env::var("WINDRES").unwrap_or_else(|_| {
            if env::var("TARGET").unwrap().starts_with("x86_64-") {
                "x86_64-w64-mingw32-windres".into()
            } else {
                "windres".into()
            }
        });
        Command::new(compiler)
            .args(["-I", "assets", "-i", "assets/client.rc", "-O", "coff", "-o"])
            .arg(&resource)
            .status()
    } else {
        Command::new(msvc_resource_compiler())
            .args(["/nologo", "/I", "assets", "/fo"])
            .arg(&resource)
            .arg("assets/client.rc")
            .status()
    }
    .expect("Windows resource compiler is required (Windows SDK rc.exe or MinGW windres)");
    assert!(status.success(), "compiling the client icon failed");
    println!(
        "cargo:rustc-link-arg-bin=mini-client={}",
        resource.display()
    );
}

// The SDK compiler is not always on PATH outside a VS developer terminal.
fn msvc_resource_compiler() -> PathBuf {
    let sdk = env::var_os("WindowsSdkDir").map(PathBuf::from).or_else(|| {
        env::var_os("ProgramFiles(x86)").map(|base| PathBuf::from(base).join("Windows Kits/10"))
    });
    if let Some(sdk) = sdk {
        if let Ok(entries) = std::fs::read_dir(sdk.join("bin")) {
            let mut candidates: Vec<_> = entries
                .flatten()
                .map(|entry| entry.path().join("x64/rc.exe"))
                .filter(|path| path.is_file())
                .collect();
            candidates.sort();
            if let Some(compiler) = candidates.pop() {
                return compiler;
            }
        }
    }
    PathBuf::from("rc.exe")
}
