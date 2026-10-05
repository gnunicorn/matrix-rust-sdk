use std::fs::create_dir_all;

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Args, Subcommand, ValueEnum};
use uniffi_bindgen::bindings::{GenerateOptions, TargetLanguage, generate};
use xshell::cmd;

use crate::{Result, sh, workspace};

struct PackageValues {
    name: &'static str,
    features: &'static str,
}

#[derive(ValueEnum, Clone)]
enum PackagePreset {
    CryptoSDK,
    FullSDK,
}

impl PackagePreset {
    fn values(self) -> PackageValues {
        match self {
            PackagePreset::CryptoSDK => {
                PackageValues { name: "matrix-sdk-crypto-ffi", features: "" }
            }
            PackagePreset::FullSDK => PackageValues { name: "matrix-sdk-ffi", features: "sentry" },
        }
    }
}

#[derive(Args)]
pub struct KotlinArgs {
    #[clap(subcommand)]
    cmd: KotlinCommand,
}

#[derive(Subcommand)]
enum KotlinCommand {
    /// Builds the SDK for Android as an AAR.
    BuildAndroidLibrary {
        #[clap(value_enum, long)]
        /// One of the selected presets
        preset: Option<PackagePreset>,

        #[clap(long)]
        /// The package name to build
        package: Option<String>,

        #[clap(long)]
        /// The features to enable
        features: Option<String>,

        /// Build with the release profile
        #[clap(long)]
        release: bool,

        /// Build with a custom profile, takes precedence over `--release`
        #[clap(long)]
        profile: Option<String>,

        /// Build the given target only
        #[clap(long)]
        only_target: Option<String>,

        /// Move the generated files into the given src directory
        #[clap(long)]
        src_dir: Utf8PathBuf,
    },
}

impl KotlinArgs {
    pub fn run(self) -> Result<()> {
        let sh = sh();
        let _p = sh.push_dir(workspace::root_path()?);

        match self.cmd {
            KotlinCommand::BuildAndroidLibrary {
                release,
                profile,
                src_dir,
                only_target,
                preset,
                package,
                features,
            } => {
                let profile = profile.as_deref().unwrap_or(if release { "release" } else { "dev" });
                let (package_name, features) = if let Some(preset) = preset {
                    let v = preset.values();
                    (v.name.to_owned(), v.features.to_owned())
                } else {
                    (
                        package.expect("preset or package must be provided"),
                        features.unwrap_or_default(),
                    )
                };
                build_android_library(profile, only_target, &src_dir, package_name, features)
            }
        }
    }
}

fn build_android_library(
    profile: &str,
    only_target: Option<String>,
    src_dir: &Utf8Path,
    package_name: String,
    features: String,
) -> Result<()> {
    let jni_libs_dir = src_dir.join("jniLibs");
    let jni_libs_dir_str = jni_libs_dir.as_str();

    let kotlin_generated_dir = src_dir.join("kotlin");
    create_dir_all(kotlin_generated_dir.clone())?;

    let uniffi_lib_path = if let Some(target) = only_target {
        println!("-- Building for {target} [1/1]");
        build_for_android_target(
            target.as_str(),
            profile,
            jni_libs_dir_str,
            package_name.as_str(),
            features.as_str(),
        )?
    } else {
        println!("-- Building for x86_64-linux-android[1/4]");
        build_for_android_target(
            "x86_64-linux-android",
            profile,
            jni_libs_dir_str,
            package_name.as_str(),
            features.as_str(),
        )?;
        println!("-- Building for aarch64-linux-android[2/4]");
        build_for_android_target(
            "aarch64-linux-android",
            profile,
            jni_libs_dir_str,
            package_name.as_str(),
            features.as_str(),
        )?;
        println!("-- Building for armv7-linux-androideabi[3/4]");
        build_for_android_target(
            "armv7-linux-androideabi",
            profile,
            jni_libs_dir_str,
            package_name.as_str(),
            features.as_str(),
        )?;
        println!("-- Building for i686-linux-android[4/4]");
        build_for_android_target(
            "i686-linux-android",
            profile,
            jni_libs_dir_str,
            package_name.as_str(),
            features.as_str(),
        )?
    };

    println!("-- Generate uniffi files");
    generate_uniffi_bindings(&uniffi_lib_path, &kotlin_generated_dir)?;

    println!("-- All done and hunky dory. Enjoy!");
    Ok(())
}

fn generate_uniffi_bindings(library_path: &Utf8Path, ffi_generated_dir: &Utf8Path) -> Result<()> {
    println!("-- library_path = {library_path}");
    generate(GenerateOptions {
        languages: vec![TargetLanguage::Kotlin],
        source: library_path.to_path_buf(),
        out_dir: ffi_generated_dir.to_path_buf(),
        ..GenerateOptions::default()
    })?;
    Ok(())
}

fn build_for_android_target(
    target: &str,
    profile: &str,
    dest_dir: &str,
    package_name: &str,
    features: &str,
) -> Result<Utf8PathBuf> {
    let sh = sh();
    cmd!(
        sh,
        "cargo ndk --target {target} -o {dest_dir} build --profile {profile} --package {package_name} --features {features}"
    )
    .run()?;

    // The builtin dev profile has its files stored under target/debug, all
    // other targets have matching directory names
    let profile_dir_name = if profile == "dev" { "debug" } else { profile };
    let package_camel = package_name.replace('-', "_");
    let lib_name = format!("lib{package_camel}.so");
    Ok(workspace::target_path()?.join(target).join(profile_dir_name).join(lib_name))
}
