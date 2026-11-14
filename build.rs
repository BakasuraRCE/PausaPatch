#[cfg(windows)]
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=icon.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/pausapatch.ico");
    println!("cargo:rerun-if-changed=assets/pausapatch-off.ico");
    println!("cargo:rerun-if-env-changed=PAUSAPATCH_SELF_CONTAINED");

    let requested_self_contained = std::env::var("PAUSAPATCH_SELF_CONTAINED").as_deref() == Ok("1");
    let self_contained = requested_self_contained && cfg!(windows);
    if requested_self_contained && !cfg!(windows) {
        println!(
            "cargo:warning=PAUSAPATCH_SELF_CONTAINED requires a native Windows build; using the framework-dependent build."
        );
    }

    if self_contained {
        #[cfg(windows)]
        {
            windows_reactor_setup::as_self_contained();
            inject_administrator_manifest();
        }
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let resource = if self_contained { "icon.rc" } else { "app.rc" };
        let result = embed_resource::compile(resource, embed_resource::NONE);
        if self_contained {
            let _ = result.manifest_optional();
        } else {
            result.manifest_required().unwrap();
        }
    }
}

#[cfg(windows)]
fn inject_administrator_manifest() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is not set");
    let path = Path::new(&out_dir).join("app.manifest");
    let Ok(mut manifest) = std::fs::read_to_string(&path) else {
        return;
    };
    if manifest.contains("requestedExecutionLevel") {
        return;
    }
    let Some(assembly) = manifest.find("<assembly") else {
        return;
    };
    let Some(offset) = manifest[assembly..].find('>') else {
        return;
    };
    let insert_at = assembly + offset + 1;
    let trust = r#"<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="requireAdministrator" uiAccess="false"/></requestedPrivileges></security></trustInfo>"#;
    manifest.insert_str(insert_at, trust);
    let _ = std::fs::write(&path, manifest);
}
