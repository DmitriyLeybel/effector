use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    net::SocketAddr,
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow, bail};
use rand::RngExt;

const DEFAULT_MCP_ADDRESS: &str = "127.0.0.1:37654";
const TOKEN_BYTES: usize = 32;

#[derive(Clone)]
pub struct McpSettings {
    pub address: SocketAddr,
    pub token: String,
}

impl McpSettings {
    pub fn endpoint(&self) -> String {
        format!("http://{}/mcp", self.address)
    }
}

pub fn load_mcp_settings() -> Result<McpSettings> {
    let address =
        std::env::var("EFFECTOR_MCP_ADDRESS").unwrap_or_else(|_| DEFAULT_MCP_ADDRESS.to_owned());
    let address = parse_mcp_address(&address)?;

    let token = match std::env::var("EFFECTOR_MCP_TOKEN") {
        Ok(token) => validate_token(token)?,
        Err(std::env::VarError::NotPresent) => load_or_create_token()?,
        Err(error) => return Err(error).context("read EFFECTOR_MCP_TOKEN"),
    };
    Ok(McpSettings { address, token })
}

fn parse_mcp_address(value: &str) -> Result<SocketAddr> {
    let address = value
        .parse::<SocketAddr>()
        .context("parse EFFECTOR_MCP_ADDRESS")?;
    if !address.ip().is_loopback() {
        bail!("Effector MCP must bind to a loopback address");
    }
    Ok(address)
}

pub fn state_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("EFFECTOR_STATE_DIR") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Ok(path);
        }
    }
    let base = dirs::config_dir().ok_or_else(|| anyhow!("no per-user config directory found"))?;
    Ok(base.join("effector"))
}

fn load_or_create_token() -> Result<String> {
    let directory = state_dir()?;
    fs::create_dir_all(&directory)
        .with_context(|| format!("create state directory {}", directory.display()))?;
    load_or_create_token_in(&directory)
}

fn load_or_create_token_in(directory: &std::path::Path) -> Result<String> {
    let path = directory.join("mcp-token");

    match read_token(&path) {
        Ok(token) => return Ok(token),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == ErrorKind::NotFound) => {}
        Err(error) => return Err(error),
    }

    let bytes: [u8; TOKEN_BYTES] = rand::rng().random();
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let temporary_path = directory.join(format!(
        ".mcp-token.{}.{}.tmp",
        std::process::id(),
        rand::rng().random::<u64>()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let result = (|| -> Result<String> {
        let mut file = options
            .open(&temporary_path)
            .with_context(|| format!("create temporary MCP token {}", temporary_path.display()))?;
        file.write_all(token.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);

        match fs::hard_link(&temporary_path, &path) {
            Ok(()) => Ok(token),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => read_token(&path),
            Err(error) => {
                Err(error).with_context(|| format!("publish MCP token {}", path.display()))
            }
        }
    })();
    let _ = fs::remove_file(&temporary_path);
    result
}

fn read_token(path: &std::path::Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect MCP token {}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("MCP token must be a regular file: {}", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!(
                "MCP token permissions must not allow group or other access: {}",
                path.display()
            );
        }
    }
    let mut contents = String::new();
    fs::File::open(path)
        .with_context(|| format!("open MCP token {}", path.display()))?
        .read_to_string(&mut contents)?;
    validate_token(contents.trim().to_owned())
}

fn validate_token(token: String) -> Result<String> {
    if token.len() != TOKEN_BYTES * 2 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("Effector MCP token must be a 64-character hexadecimal value");
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use std::{fs, thread};

    use uuid::Uuid;

    use super::{load_or_create_token_in, parse_mcp_address, read_token};

    fn temporary_directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("effector-settings-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn token_creation_is_atomic_across_concurrent_callers() {
        let directory = temporary_directory();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let directory = directory.clone();
                thread::spawn(move || load_or_create_token_in(&directory).unwrap())
            })
            .collect();
        let tokens: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        assert!(tokens.iter().all(|token| token == &tokens[0]));
        assert_eq!(tokens[0].len(), 64);
        assert_eq!(read_token(&directory.join("mcp-token")).unwrap(), tokens[0]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mcp_address_must_be_loopback() {
        assert!(parse_mcp_address("127.0.0.1:37654").is_ok());
        assert!(parse_mcp_address("[::1]:37654").is_ok());
        let error = parse_mcp_address("0.0.0.0:37654").unwrap_err().to_string();
        assert!(error.contains("loopback"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn token_reader_rejects_permissive_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = temporary_directory();
        let path = directory.join("mcp-token");
        fs::write(
            &path,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let error = read_token(&path).unwrap_err().to_string();
        assert!(error.contains("permissions"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn token_reader_rejects_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = temporary_directory();
        let target = directory.join("target");
        let path = directory.join("mcp-token");
        fs::write(
            &target,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
        )
        .unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&target, &path).unwrap();

        let error = read_token(&path).unwrap_err().to_string();
        assert!(error.contains("regular file"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }
}
