use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::{env, fs};

fn main() {
    println!("cargo::rerun-if-changed=.env");
    println!("cargo::rerun-if-changed=.env.prod");
    println!("cargo::rerun-if-changed=java/gradle");
    println!("cargo::rerun-if-changed=java/src");
    println!("cargo::rerun-if-changed=java/build.gradle.kts");
    println!("cargo::rerun-if-changed=java/settings.gradle.kts");
    println!("cargo::rerun-if-changed=java/gradle.properties");

    set_env();
    build_java_jars();
}

fn set_env() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let env_path = [".env", ".env.prod"]
        .into_iter()
        .map(|name| manifest_dir.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| {
            println!(
                "cargo::error=missing {}/.env (copy .env.prod to .env)",
                manifest_dir.display()
            );
            exit(1);
        });

    let iter = dotenvy::from_path_iter(&env_path).unwrap_or_else(|err| {
        println!("cargo::error=failed to read {}: {err}", env_path.display());
        exit(1);
    });

    for item in iter {
        let (var_name, var_value) = item.unwrap_or_else(|err| {
            println!(
                "cargo::error=invalid line in {}: {err}",
                env_path.display()
            );
            exit(1);
        });
        if var_name == "DATABASE_URL" {
            // The sqlx database URL is a build-time detail that should not be exposed to the crate
            continue;
        }

        println!("cargo::rustc-env={var_name}={var_value}");
    }
}

fn build_java_jars() {
    let out_dir =
        dunce::canonicalize(PathBuf::from(env::var_os("OUT_DIR").unwrap()))
            .unwrap();

    println!(
        "cargo::rustc-env=JAVA_JARS_DIR={}",
        out_dir.join("java/libs").display()
    );

    let gradle_path = fs::canonicalize(
        #[cfg(target_os = "windows")]
        "java\\gradlew.bat",
        #[cfg(not(target_os = "windows"))]
        "java/gradlew",
    )
    .unwrap();

    let mut build_dir_str = OsString::from("-Dorg.gradle.project.buildDir=");
    build_dir_str.push(out_dir.join("java"));
    let exit_status = Command::new(gradle_path)
        .arg(build_dir_str)
        .arg("build")
        .arg("--no-daemon")
        .arg("--console=rich")
        .current_dir(dunce::canonicalize("java").unwrap())
        .status()
        .expect("Failed to wait on Gradle build");

    if !exit_status.success() {
        println!("cargo::error=Gradle build failed with {exit_status}");
        exit(exit_status.code().unwrap_or(1));
    }
}
