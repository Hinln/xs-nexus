use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=XS_BUILD_GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=XS_BUILD_DATE_EPOCH");

    let commit = env::var("XS_BUILD_GIT_COMMIT").unwrap_or_else(|_| "unknown".to_owned());
    assert!(
        commit == "unknown"
            || (commit.len() == 40
                && commit
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))),
        "XS_BUILD_GIT_COMMIT must be 'unknown' or an exact lowercase 40-character Git object ID"
    );

    let build_epoch = env::var("XS_BUILD_DATE_EPOCH").unwrap_or_else(|_| "0".to_owned());
    assert!(
        !build_epoch.is_empty() && build_epoch.bytes().all(|byte| byte.is_ascii_digit()),
        "XS_BUILD_DATE_EPOCH must be a nonnegative integer"
    );

    println!("cargo:rustc-env=XS_BUILD_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=XS_BUILD_DATE_EPOCH={build_epoch}");
}
