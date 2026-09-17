//! Frozen-core pin proof, exact RSS corpus, VMBC wire, and firmware C ABI boundary.

#![cfg(feature = "host")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustscript_embedded::{RunOutcome, run_source_file};
use vm::{compile_source_file, decode_program, encode_program};

const FROZEN_RUSTSCRIPT_REV: &str = "b1d6cffede77f49410bf63525f30b9a46b02dc01";
const RUSTSCRIPT_GIT: &str = "https://github.com/rustscript-lang/rustscript.git";
const EXPECTED_BUNDLED_RSS: usize = 4;
const REQUIRED_LOCK_PACKAGES: &[&str] =
    &["pd-vm", "pd-vm-nostd", "pd-host-function", "pd-host-schema"];
const HOST_STATE_TOKENS: &[&str] = &[
    "HostState",
    "HostStateRef",
    "HostStateMut",
    "HostStateEffect",
    "HostStateProvider",
    "ReadState",
    "WriteState",
];
const DESCRIPTOR_TOKENS: &[&str] = &[
    "HostApiBuilder",
    "HostApiCatalog",
    "HostFunctionRegistry",
    "HostModuleDescriptor",
    "HostFunctionDescriptor",
    "pd_host_function",
];
const FIRMWARE_HOST_ABI: &[(&str, u8)] = &[
    ("gpio::configure", 2),
    ("gpio::digital_write", 2),
    ("gpio::digital_read", 1),
    ("gpio::analog_read", 1),
    ("gpio::pwm_write", 4),
    ("i2c::open", 3),
    ("i2c::close", 0),
    ("i2c::transmit", 2),
    ("i2c::transmit_register", 3),
    ("i2c::receive", 2),
    ("i2c::receive_register", 3),
    ("mcu::delay_ms", 1),
    ("mcu::delay_us", 1),
    ("mcu::millis", 0),
    ("mcu::micros", 0),
    ("mcu::cpu_frequency_mhz", 0),
    ("mcu::free_heap", 0),
    ("mcu::flash_size", 0),
    ("mcu::random", 0),
    ("mcu::restart", 0),
    ("mcu::deep_sleep_us", 1),
    ("wifi::connect", 2),
    ("wifi::disconnect", 0),
    ("wifi::is_connected", 0),
    ("wifi::rssi", 0),
    ("wifi::local_ip", 0),
    ("bluetooth::enable", 0),
    ("bluetooth::disable", 0),
    ("bluetooth::is_enabled", 0),
    ("serial::write_line", 1),
    ("serial::available", 0),
    ("serial::read_bytes", 1),
];

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rustscript_lock_source() -> String {
    format!("git+{RUSTSCRIPT_GIT}?rev={FROZEN_RUSTSCRIPT_REV}#{FROZEN_RUSTSCRIPT_REV}")
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if matches!(name, ".git" | "target" | ".pio") {
                continue;
            }
            collect_files(&path, out);
            continue;
        }
        out.push(path);
    }
}

fn bundled_rss_files() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_files(&manifest_dir(), &mut paths);
    paths.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rss"));
    paths.sort();
    paths
}

fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_files(dir, &mut paths);
    paths.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"));
    paths.sort();
    paths
}

fn lock_package_blocks(lockfile: &str) -> Vec<&str> {
    lockfile.split("\n[[package]]").collect()
}

fn lock_package_name(block: &str) -> Option<&str> {
    block.lines().find_map(|line| {
        line.strip_prefix("name = \"")
            .and_then(|rest| rest.strip_suffix('"'))
    })
}

fn lock_package_source(block: &str) -> Option<&str> {
    block
        .lines()
        .find_map(|line| line.strip_prefix("source = "))
}

fn scratch_root() -> PathBuf {
    let root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir().join("target"));
    root.join("micro-frozen-core-corpus")
}

fn compile_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rustscript-compile-vmbc"))
}

fn compile_rss_to_vmbc(source: &Path, output: &Path) {
    let compiled = compile_source_file(source)
        .unwrap_or_else(|error| panic!("{} failed to compile: {error}", source.display()));
    let encoded = encode_program(&compiled.program)
        .unwrap_or_else(|error| panic!("{} VMBC encode failed: {error}", source.display()));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).expect("vmbc output directory");
    }
    fs::write(output, &encoded).expect("write vmbc");

    let status = Command::new(compile_bin())
        .arg(source)
        .arg(output)
        .status()
        .unwrap_or_else(|error| panic!("run rustscript-compile-vmbc: {error}"));
    assert!(
        status.success(),
        "{} failed through rustscript-compile-vmbc",
        source.display()
    );
    let bytes = fs::read(output).expect("read compiled vmbc");
    assert!(
        !bytes.is_empty(),
        "{} produced empty VMBC",
        source.display()
    );
    decode_program(&bytes)
        .unwrap_or_else(|error| panic!("{} std VMBC decode failed: {error}", source.display()));
}

#[test]
fn rustscript_crates_are_pinned_to_the_frozen_full_sha() {
    let cargo_toml = fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("Cargo.toml");
    let cargo_lock = fs::read_to_string(manifest_dir().join("Cargo.lock")).expect("Cargo.lock");

    assert!(
        cargo_toml.contains(&format!("rev = \"{FROZEN_RUSTSCRIPT_REV}\"")),
        "Cargo.toml must pin the frozen full SHA"
    );
    assert!(
        cargo_toml.contains(&format!("git = \"{RUSTSCRIPT_GIT}\"")),
        "Cargo.toml must use the canonical HTTPS Git remote"
    );
    assert!(
        cargo_toml.contains("pd_vm_nostd") && cargo_toml.contains("optional = true"),
        "pd-vm-nostd must remain optional for firmware no_std builds"
    );
    assert!(
        cargo_toml.contains("default-features = false"),
        "pd-vm must keep default-features disabled"
    );
    assert!(
        !cargo_toml.contains("path = \"../") && !cargo_toml.contains("path = '/"),
        "production RustScript crates must not use path pins"
    );
    assert!(
        !cargo_toml.contains("/home/"),
        "production RustScript crates must not use machine-specific paths"
    );

    let expected_source = rustscript_lock_source();
    let mut proven = Vec::new();
    for block in lock_package_blocks(&cargo_lock) {
        let Some(name) = lock_package_name(block) else {
            continue;
        };
        let Some(source) = lock_package_source(block) else {
            continue;
        };
        if !source.contains("github.com/rustscript-lang/rustscript") {
            continue;
        }
        assert_eq!(
            source.trim(),
            format!("\"{expected_source}\""),
            "Cargo.lock {name} must use the canonical HTTPS source at the pinned full rev"
        );
        proven.push(name.to_string());
    }
    for required in REQUIRED_LOCK_PACKAGES {
        assert!(
            proven.iter().any(|name| name == required),
            "Cargo.lock must prove {required} source: {proven:?}"
        );
    }
}

#[test]
fn production_sources_do_not_install_pd_vm_host_descriptors() {
    let sources = rust_sources(&manifest_dir().join("src"));
    assert!(!sources.is_empty(), "expected Rust sources");
    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for token in DESCRIPTOR_TOKENS {
            assert!(
                !source.contains(token),
                "{} is a firmware/no_std consumer and must not define pd-vm hosts ({token})",
                path.display()
            );
        }
    }
}

#[test]
fn bundled_rss_corpus_has_the_exact_checked_in_count() {
    let corpus = bundled_rss_files();
    assert_eq!(
        corpus.len(),
        EXPECTED_BUNDLED_RSS,
        "bundled RSS count drifted: {corpus:?}"
    );
}

#[test]
fn bundled_rss_compiles_through_frozen_compiler_and_runs_where_supported() {
    let corpus = bundled_rss_files();
    assert_eq!(corpus.len(), EXPECTED_BUNDLED_RSS);
    let scratch = scratch_root();
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).expect("scratch corpus");

    for source in &corpus {
        let name = source
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("utf-8 rss stem");
        let output = scratch.join(format!("{name}.vmbc"));
        compile_rss_to_vmbc(source, &output);

        match name {
            "blinky" | "math" => {
                let RunOutcome::Halted { .. } = run_source_file(source)
                    .unwrap_or_else(|error| panic!("{} failed to run: {error}", source.display()));
            }
            "esp32-blinky" | "framework-api-smoke" => {}
            other => panic!("unexpected bundled RSS {other}"),
        }
    }
}

#[test]
fn firmware_host_abi_is_the_static_c_dispatch_table() {
    let firmware = fs::read_to_string(manifest_dir().join("firmware/host_framework.cpp"))
        .expect("host_framework.cpp");
    let header = firmware
        .split("constexpr host_export HOST_EXPORTS[] = {")
        .nth(1)
        .and_then(|rest| rest.split("};").next())
        .expect("HOST_EXPORTS table");
    let mut parsed = Vec::new();
    for line in header.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(name_start) = trimmed.find("{\"") else {
            continue;
        };
        let rest = &trimmed[name_start + 2..];
        let name_end = rest.find('"').expect("host name terminator");
        let name = &rest[..name_end];
        let after_name = &rest[name_end + 1..];
        let arity = after_name
            .trim_start_matches(['"', ',', ' '])
            .split(',')
            .next()
            .expect("arity")
            .trim()
            .parse::<u8>()
            .unwrap_or_else(|_| panic!("arity for {name}: {after_name}"));
        parsed.push((name.to_string(), arity));
    }
    let expected = FIRMWARE_HOST_ABI
        .iter()
        .map(|(name, arity)| ((*name).to_string(), *arity))
        .collect::<Vec<_>>();
    assert_eq!(
        parsed, expected,
        "firmware HOST_EXPORTS is the guest-visible ABI"
    );

    let smoke = manifest_dir().join("programs/framework-api-smoke.rss");
    let compiled = compile_source_file(&smoke).expect("framework API should compile");
    for import in &compiled.program.imports {
        let Some((_, arity)) = FIRMWARE_HOST_ABI
            .iter()
            .find(|(name, _)| *name == import.name)
        else {
            panic!(
                "compiled import {} is outside the firmware C ABI",
                import.name
            );
        };
        assert_eq!(
            import.arity, *arity,
            "compiled arity for {} must match firmware dispatch",
            import.name
        );
    }
    assert!(
        compiled.program.host_import_schemas().is_empty()
            || compiled
                .program
                .host_import_schemas()
                .iter()
                .all(Option::is_none),
        "firmware wildcard hosts must not carry pd-vm catalog/effect metadata"
    );
}

#[test]
fn vmbc_guest_sections_parse_or_reject_and_host_state_stays_off_the_wire() {
    let source = manifest_dir().join("programs/esp32-blinky.rss");
    let compiled = compile_source_file(&source).expect("esp32 program should compile");
    let bytes = encode_program(&compiled.program).expect("program should encode");
    let decoded = decode_program(&bytes).expect("current VMBC must decode");
    assert_eq!(decoded.imports.len(), compiled.program.imports.len());
    for token in HOST_STATE_TOKENS {
        let needle = token.as_bytes();
        assert!(
            !bytes.windows(needle.len()).any(|window| window == needle),
            "private {token} metadata must not appear in firmware VMBC"
        );
    }

    let mut unsupported = bytes.clone();
    unsupported[4..6].copy_from_slice(&14u16.to_le_bytes());
    decode_program(&unsupported).expect_err("unsupported VMBC version must reject");

    let mut trailing = bytes.clone();
    trailing.push(0xFF);
    decode_program(&trailing).expect_err("trailing VMBC bytes must reject");
}

#[cfg(feature = "esp32c3")]
mod nostd_wire {
    use super::*;
    use pd_vm_nostd::decode_program as decode_nostd;

    #[test]
    fn no_std_decoder_accepts_current_firmware_vmbc_and_rejects_unknown_sections() {
        let source = manifest_dir().join("programs/esp32-blinky.rss");
        let compiled = compile_source_file(&source).expect("esp32 program should compile");
        let bytes = encode_program(&compiled.program).expect("program should encode");
        let decoded = decode_nostd(&bytes).expect("no_std decoder must load firmware VMBC");
        assert_eq!(decoded.imports().len(), compiled.program.imports.len());
        for (import, expected) in decoded
            .imports()
            .iter()
            .zip(compiled.program.imports.iter())
        {
            assert_eq!(import.name, expected.name);
            assert_eq!(import.arity, expected.arity);
        }

        let mut unsupported = bytes.clone();
        unsupported[4..6].copy_from_slice(&14u16.to_le_bytes());
        decode_nostd(&unsupported).expect_err("no_std decoder must reject unsupported version");

        let mut trailing = bytes.clone();
        trailing.push(0xFF);
        decode_nostd(&trailing).expect_err("no_std decoder must reject trailing bytes");
    }
}
