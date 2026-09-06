fn main() {
    println!("cargo:rerun-if-changed=../../assets/pet-repair.ico");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let icon = root
        .join("../../assets/pet-repair.ico")
        .canonicalize()
        .unwrap();
    let rc = out.join("icon.rc");
    let res = out.join("icon.res");
    std::fs::write(
        &rc,
        format!(
            "1 ICON \"{}\"\n",
            icon.display()
                .to_string()
                .replace('\\', "/")
                .trim_start_matches("//?/")
        ),
    )
    .unwrap();
    let compiler = std::env::var("RC").unwrap_or_else(|_| "rc".into());
    let status = std::process::Command::new(compiler)
        .arg("/fo")
        .arg(&res)
        .arg(&rc)
        .status()
        .expect("Windows resource compiler is required");
    assert!(status.success(), "icon compilation failed");
    println!(
        "cargo:rustc-link-arg-bin=pet-repair-native-ui={}",
        res.display()
    );
    slint_build::compile_with_config(
        "repair.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent".into()),
    )
    .unwrap();
}
