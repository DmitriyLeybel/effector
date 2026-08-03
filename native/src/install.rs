use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

#[cfg(target_os = "windows")]
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::json;
use uuid::Uuid;

use crate::settings;

const NATIVE_HOST_NAME: &str = "com.effector.browser";

pub fn run(extension_id: &str) -> Result<()> {
    validate_extension_id(extension_id)?;
    let executable = executable_path()?;
    let executable = executable
        .to_str()
        .context("Effector executable path is not valid Unicode")?;
    let mcp = settings::load_mcp_settings()?;
    let manifest = json!({
        "name": NATIVE_HOST_NAME,
        "description": "Effector Chrome browser broker",
        "path": executable,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension_id}/")],
    });

    let manifest_path = install_manifest(&manifest)?;
    println!("Registered Chrome Native Messaging host:");
    println!("  {}", manifest_path.display());
    println!();
    println!("Configure this local MCP server in your client:");
    println!("  Transport: Streamable HTTP");
    println!("  URL: {}", mcp.endpoint());
    println!("  Authorization: Bearer {}", mcp.token);
    Ok(())
}

fn executable_path() -> Result<PathBuf> {
    let executable = std::env::current_exe()?;
    #[cfg(target_os = "windows")]
    return Ok(executable);
    #[cfg(not(target_os = "windows"))]
    return Ok(executable.canonicalize()?);
}

fn validate_extension_id(extension_id: &str) -> Result<()> {
    if extension_id.len() != 32 || !extension_id.bytes().all(|byte| matches!(byte, b'a'..=b'p')) {
        bail!("Chrome extension ID must be 32 lowercase letters in the range a-p");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_manifest(manifest: &serde_json::Value) -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory found")?;
    let directory = home.join(".config/google-chrome/NativeMessagingHosts");
    write_manifest(directory, manifest)
}

#[cfg(target_os = "macos")]
fn install_manifest(manifest: &serde_json::Value) -> Result<PathBuf> {
    let home = dirs::home_dir().context("no home directory found")?;
    let directory = home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts");
    write_manifest(directory, manifest)
}

#[cfg(target_os = "windows")]
fn install_manifest(manifest: &serde_json::Value) -> Result<PathBuf> {
    let path = write_manifest(settings::state_dir()?, manifest)?;
    let key = format!(r"HKCU\Software\Google\Chrome\NativeMessagingHosts\{NATIVE_HOST_NAME}");
    let status = Command::new("reg")
        .args(["add", &key, "/ve", "/t", "REG_SZ", "/d"])
        .arg(&path)
        .arg("/f")
        .status()
        .context("register Native Messaging host in Windows registry")?;
    if !status.success() {
        bail!("Windows registry command failed with {status}");
    }
    Ok(path)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn install_manifest(_manifest: &serde_json::Value) -> Result<PathBuf> {
    bail!("automatic Native Messaging installation is not supported on this platform")
}

fn write_manifest(directory: PathBuf, manifest: &serde_json::Value) -> Result<PathBuf> {
    fs::create_dir_all(&directory).with_context(|| format!("create {}", directory.display()))?;
    let path = directory.join(format!("{NATIVE_HOST_NAME}.json"));
    let temporary_path = directory.join(format!(".{NATIVE_HOST_NAME}.{}.tmp", Uuid::new_v4()));
    let mut contents = serde_json::to_vec_pretty(manifest)?;
    contents.push(b'\n');
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .with_context(|| format!("create {}", temporary_path.display()))?;
        file.write_all(&contents)?;
        file.sync_all()?;
        drop(file);

        #[cfg(target_os = "windows")]
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("replace existing manifest {}", path.display()))?;
        }
        fs::rename(&temporary_path, &path)
            .with_context(|| format!("publish native host manifest {}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result?;
    Ok(path)
}
