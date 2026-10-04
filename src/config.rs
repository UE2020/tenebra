use crate::tls::TlsMode;
use anyhow::{anyhow, Context};
use rand::{distr::Alphanumeric, Rng};
use serde::Deserialize;
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
};

pub const CONFIG_FILENAME: &str = "config.toml";

const CONFIG_TEMPLATE: &str = include_str!("default.toml");

/// Return the directory that holds Tenebra's configuration file, creating it if
/// it does not already exist.
pub fn config_dir() -> anyhow::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let dir = PathBuf::from(r"C:\tenebra");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory {}", dir.display()))?;
        Ok(dir)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let dir = dirs::config_dir()
            .context("Failed to find config directory")?
            .join("tenebra");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory {}", dir.display()))?;
        Ok(dir)
    }
}

/// Full path to the configuration file.
pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_FILENAME))
}

fn default_target_bitrate() -> u32 {
    4000
}

fn default_port() -> u16 {
    8080
}

fn default_true() -> bool {
    true
}

fn default_vbv_buf_capacity() -> u32 {
    120
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_target_bitrate")]
    pub target_bitrate: u32,
    #[serde(default)]
    pub startx: i32,
    #[serde(default)]
    pub starty: i32,
    #[serde(default)]
    pub endx: Option<i32>,
    #[serde(default)]
    pub endy: Option<i32>,

    // Windows-only
    #[serde(default)]
    pub windows_monitor_index: Option<i32>,
    #[serde(default)]
    pub windows_capture_api: Option<String>,
    #[serde(default)]
    pub windows_quality_vs_speed: Option<u32>,

    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_true")]
    pub sound_forwarding: bool,
    #[serde(default, alias = "hwencode")]
    pub vaapi: bool,
    #[serde(default)]
    pub vapostproc: bool,
    #[serde(default)]
    pub no_bwe: bool,
    #[serde(default)]
    pub full_chroma: bool,
    #[serde(default = "default_true")]
    pub tcp_upnp: bool,
    #[serde(default = "default_vbv_buf_capacity")]
    pub vbv_buf_capacity: u32,
    #[serde(default)]
    pub tls: TlsMode,
    #[serde(default)]
    pub cert: Option<PathBuf>,
    #[serde(default)]
    pub key: Option<PathBuf>,
}

impl std::fmt::Display for Config {
    #[rustfmt::skip]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "Server configuration")?;
        writeln!(f, "\tTarget bitrate:                    {} Kbit/s", self.target_bitrate)?;
        writeln!(f, "\tStart x-coordinate:                {}", self.startx)?;
        writeln!(f, "\tStart y-coordinate:                {}", self.starty)?;
        writeln!(f, "\tEnd x-coordinate:                  {:?}", self.endx)?;
        writeln!(f, "\tEnd y-coordinate:                  {:?}", self.endy)?;
        writeln!(f, "\tPort:                              {}", self.port)?;
        writeln!(f, "\tSound forwarding:                  {}", bool_to_str(self.sound_forwarding))?;
        writeln!(f, "\tHardware accelerated encoding:     {}", bool_to_str(self.vaapi))?;
        writeln!(f, "\tVA-API format conversion:          {}", bool_to_str(self.vapostproc))?;
        writeln!(f, "\tBandwidth estimation:              {}", bool_to_str(!self.no_bwe))?;
        writeln!(f, "\tFull color encoding:               {}", bool_to_str(self.full_chroma))?;
        writeln!(f, "\tAutomatic ICE-TCP UPnP forwarding: {}", bool_to_str(self.tcp_upnp))?;
        writeln!(f, "\tVBV Buffer capacity:               {} ms", self.vbv_buf_capacity)?;
        writeln!(f, "\tTLS:                               {}", self.tls.as_str())?;
        writeln!(f, "\tCertificate:                       {:?}", self.cert)?;

        Ok(())
    }
}

fn bool_to_str(b: bool) -> &'static str {
    match b {
        true => "on",
        false => "off",
    }
}

fn generate_password() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

fn free_port() -> u16 {
    match TcpListener::bind("0.0.0.0:0") {
        Ok(listener) => match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => 8080,
        },
        Err(_) => 8080,
    }
}

fn render_template(password: &str, port: u16) -> String {
    CONFIG_TEMPLATE
        .replace(
            "password = \"placeholder\"",
            &format!("password = \"{password}\""),
        )
        .replace("port = 8080", &format!("port = {port}"))
}

/// Write a fresh configuration file to `path`, creating parent directories as
/// needed. Returns the generated password.
pub fn write_new_config(path: &Path) -> anyhow::Result<String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }
    }

    let password = generate_password();
    let port = free_port();
    let contents = render_template(&password, port);
    std::fs::write(path, contents)
        .with_context(|| format!("Failed to write config file {}", path.display()))?;

    Ok(password)
}

/// Replace only the line assigning `password` (or append one if absent),
/// preserving every other byte and comment in the file.
fn rewrite_password(path: &Path, password: &str) -> anyhow::Result<()> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;

    let mut replaced = false;
    let mut out = String::with_capacity(contents.len() + 32);
    for line in contents.split_inclusive('\n') {
        if !replaced && line.trim_start().starts_with("password") {
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(&format!("password = \"{password}\"{ending}"));
            replaced = true;
        } else {
            out.push_str(line);
        }
    }

    if !replaced {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("password = \"{password}\"\n"));
    }

    std::fs::write(path, out)
        .with_context(|| format!("Failed to write config file {}", path.display()))?;
    Ok(())
}

/// Load the configuration from `path`, creating and populating a fresh file if
/// it does not yet exist.
pub fn load_or_init(path: &Path) -> anyhow::Result<Config> {
    if !path.exists() {
        let password = write_new_config(path)?;
        log::info!(
            "No config file found. Wrote a new configuration to {} with password: {}",
            path.display(),
            password
        );

        let cfg: Config = toml::from_str(
            &std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file {}", path.display()))?,
        )
        .with_context(|| format!("Failed to parse config file {}", path.display()))?;

        cfg.validate()?;
        return Ok(cfg);
    }

    let mut cfg: Config = toml::from_str(
        &std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file {}", path.display()))?,
    )
    .with_context(|| format!("Failed to parse config file {}", path.display()))?;

    if cfg.password.is_empty() || cfg.password == "placeholder" {
        let password = generate_password();
        rewrite_password(path, &password)?;
        cfg.password = password;
    }

    cfg.validate()?;
    Ok(cfg)
}

impl Config {
    /// Validate the configuration, reporting every problem at once.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut problems: Vec<String> = Vec::new();

        if self.password.is_empty() {
            problems.push("password must not be empty".to_string());
        }
        if self.target_bitrate == 0 {
            problems.push("target_bitrate must be greater than 0".to_string());
        }
        if self.port == 0 {
            problems.push("port must not be 0".to_string());
        }
        if self.full_chroma && self.vaapi {
            problems.push("full_chroma cannot be combined with hwencode".to_string());
        }
        if self.vapostproc && !self.vaapi {
            problems.push("vapostproc requires hwencode to be enabled".to_string());
        }

        if matches!(self.tls, TlsMode::Custom) {
            check_readable(&mut problems, "cert", &self.cert);
            check_readable(&mut problems, "key", &self.key);
        }

        if let Some(api) = &self.windows_capture_api {
            let api = api.to_ascii_lowercase();
            if api != "dxgi" && api != "wgc" {
                problems.push(format!(
                    "windows_capture_api must be \"dxgi\" or \"wgc\" (got {:?})",
                    self.windows_capture_api
                ));
            }
        }

        if let Some(index) = self.windows_monitor_index {
            if index < -1 {
                problems.push(format!("windows_monitor_index must be >= -1 (got {index})"));
            }
        }

        if !problems.is_empty() {
            return Err(anyhow!(
                "Invalid configuration:\n  - {}",
                problems.join("\n  - ")
            ));
        }

        Ok(())
    }
}

/// Record a problem when a required file is absent or cannot be opened.
fn check_readable(problems: &mut Vec<String>, field: &str, path: &Option<PathBuf>) {
    match path {
        None => problems.push(format!("{field} must be set when tls = \"custom\"")),
        Some(path) => {
            if !path.exists() {
                problems.push(format!("{field} file {} does not exist", path.display()));
            } else if std::fs::File::open(path).is_err() {
                problems.push(format!("{field} file {} is not readable", path.display()));
            }
        }
    }
}
