use std::process::Command;

#[test]
fn version_matches_package() {
    let output = Command::new(env!("CARGO_BIN_EXE_tear"))
        .arg("--version")
        .output()
        .expect("run tear");
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        concat!("tear ", env!("CARGO_PKG_VERSION"), "\n").as_bytes()
    );
}

#[test]
fn help_and_bad_input_have_distinct_exit_codes() {
    for (args, code) in [(&["--help"][..], 0), (&["--unknown"][..], 2), (&[][..], 2)] {
        let status = Command::new(env!("CARGO_BIN_EXE_tear"))
            .args(args)
            .output()
            .expect("run tear")
            .status;
        assert_eq!(status.code(), Some(code));
    }
}
