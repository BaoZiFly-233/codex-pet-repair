fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/pet-repair.ico");
    if std::env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc" {
        println!("cargo:rustc-link-arg-bin=pet-repair=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=pet-repair=/MANIFESTINPUT:{root}/app.manifest");
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let rc = out.join("icon.rc");
        let res = out.join("icon.res");
        std::fs::write(
            &rc,
            format!(
                "1 ICON \"{}/assets/pet-repair.ico\"\n",
                root.replace('\\', "/")
            ),
        )
        .unwrap();
        let status =
            std::process::Command::new(std::env::var("RC").unwrap_or_else(|_| "rc".into()))
                .arg("/fo")
                .arg(&res)
                .arg(&rc)
                .status()
                .expect("Windows resource compiler is required");
        assert!(status.success(), "icon compilation failed");
        println!("cargo:rustc-link-arg-bin=pet-repair={}", res.display());
    } else {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let rc = out.join("app.rc");
        std::fs::write(
            &rc,
            format!(
                "1 24 \"{0}/app.manifest\"\n1 ICON \"{0}/assets/pet-repair.ico\"\n",
                root.replace('\\', "/")
            ),
        )
        .unwrap();
        let status = std::process::Command::new("x86_64-w64-mingw32-windres")
            .args(["-i"])
            .arg(rc)
            .args(["-o"])
            .arg(out.join("app.o"))
            .status()
            .unwrap();
        assert!(status.success(), "manifest resource compilation failed");
        println!(
            "cargo:rustc-link-arg-bin=pet-repair={}",
            out.join("app.o").display()
        );
    }
}
