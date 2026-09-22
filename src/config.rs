use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled_on_start: bool,
    pub require_rdp: bool,
    pub recenter_cursor: bool,
    pub target_scope: String,
    pub title_contains: String,
    pub process_names: Vec<String>,
    pub min_cutoff: f64,
    pub beta: f64,
    pub derivative_cutoff: f64,
    pub deadzone: f64,
    pub max_delta: f64,
    pub max_output: f64,
    pub sensitivity: f64,
    pub poll_interval_ms: u64,
    pub recenter_settle_ms: u64,
    pub config_reload_secs: u64,
    pub config_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled_on_start: true,
            require_rdp: true,
            recenter_cursor: true,
            target_scope: "minecraft".to_owned(),
            title_contains: "minecraft".to_owned(),
            process_names: vec!["javaw.exe".to_owned(), "java.exe".to_owned()],
            min_cutoff: 1.2,
            beta: 0.015,
            derivative_cutoff: 1.0,
            deadzone: 0.15,
            max_delta: 300.0,
            max_output: 40.0,
            sensitivity: 1.0,
            poll_interval_ms: 8,
            recenter_settle_ms: 3,
            config_reload_secs: 5,
            config_path: default_config_path(),
        }
    }
}

impl Config {
    pub fn filter_config(&self) -> crate::filter::FilterConfig {
        crate::filter::FilterConfig {
            min_cutoff: self.min_cutoff,
            beta: self.beta,
            derivative_cutoff: self.derivative_cutoff,
            deadzone: self.deadzone,
            max_delta: self.max_delta,
            max_output: self.max_output,
            sensitivity: self.sensitivity,
        }
    }

    pub fn load() -> Self {
        Self::load_from_path(Self::default())
    }

    pub fn load_from_path(mut config: Self) -> Self {
        let path = config.config_path.clone();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                for (line_number, line) in contents.lines().enumerate() {
                    if let Err(error) = config.apply_line(line) {
                        eprintln!(
                            "配置文件 {} 第 {} 行已忽略: {}",
                            path.display(),
                            line_number + 1,
                            error
                        );
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Err(error) = config.save() {
                    eprintln!("无法创建配置文件 {}: {error}", path.display());
                }
            }
            Err(error) => eprintln!("无法读取配置文件 {}: {error}", path.display()),
        }
        config.sanitize();
        config
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = self.config_path.with_extension("tmp");
        std::fs::write(&temporary, self.to_text())?;
        if let Err(rename_error) = std::fs::rename(&temporary, &self.config_path) {
            if let Err(write_error) = std::fs::write(&self.config_path, self.to_text()) {
                let _ = std::fs::remove_file(&temporary);
                return Err(write_error).or(Err(rename_error));
            }
            let _ = std::fs::remove_file(&temporary);
        }
        Ok(())
    }

    pub fn to_text(&self) -> String {
        let mut text = String::from("# Remote Camera configuration v2\n");
        let entries = [
            ("enabled_on_start", self.enabled_on_start.to_string()),
            ("require_rdp", self.require_rdp.to_string()),
            ("recenter_cursor", self.recenter_cursor.to_string()),
            ("target_scope", self.target_scope.clone()),
            ("title_contains", self.title_contains.clone()),
            ("process_names", self.process_names.join(",")),
            ("min_cutoff", self.min_cutoff.to_string()),
            ("beta", self.beta.to_string()),
            ("derivative_cutoff", self.derivative_cutoff.to_string()),
            ("deadzone", self.deadzone.to_string()),
            ("max_delta", self.max_delta.to_string()),
            ("max_output", self.max_output.to_string()),
            ("sensitivity", self.sensitivity.to_string()),
            ("poll_interval_ms", self.poll_interval_ms.to_string()),
            ("recenter_settle_ms", self.recenter_settle_ms.to_string()),
            ("config_reload_secs", self.config_reload_secs.to_string()),
        ];
        for (key, value) in entries {
            let _ = writeln!(text, "{key}={value}");
        }
        text
    }

    pub fn sanitize(&mut self) {
        self.title_contains = self.title_contains.trim().to_ascii_lowercase();
        if self.title_contains.is_empty() {
            self.title_contains = "minecraft".to_owned();
        }
        self.target_scope = match self.target_scope.trim().to_ascii_lowercase().as_str() {
            "desktop" | "rdp" | "all" => "desktop".to_owned(),
            _ => "minecraft".to_owned(),
        };
        self.process_names = self
            .process_names
            .iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect();
        if self.process_names.is_empty() {
            self.process_names = vec!["javaw.exe".to_owned(), "java.exe".to_owned()];
        }
        self.min_cutoff = finite_clamp(self.min_cutoff, 0.05, 30.0, 1.2);
        self.beta = finite_clamp(self.beta, 0.0, 2.0, 0.015);
        self.derivative_cutoff = finite_clamp(self.derivative_cutoff, 0.05, 30.0, 1.0);
        self.deadzone = finite_clamp(self.deadzone, 0.0, 20.0, 0.15);
        self.max_delta = finite_clamp(self.max_delta, 4.0, 4_000.0, 300.0);
        self.max_output = finite_clamp(self.max_output, 1.0, 500.0, 40.0);
        self.sensitivity = finite_clamp(self.sensitivity, 0.05, 8.0, 1.0);
        self.poll_interval_ms = self.poll_interval_ms.clamp(4, 50);
        self.recenter_settle_ms = self.recenter_settle_ms.clamp(0, 50);
        self.config_reload_secs = self.config_reload_secs.clamp(1, 300);
    }

    fn apply_line(&mut self, line: &str) -> Result<(), String> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return Ok(());
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| "需要 key=value 格式".to_owned())?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "enabled_on_start" => self.enabled_on_start = parse_bool(value)?,
            "require_rdp" => self.require_rdp = parse_bool(value)?,
            "recenter_cursor" => self.recenter_cursor = parse_bool(value)?,
            "target_scope" => self.target_scope = value.to_owned(),
            "title_contains" => self.title_contains = value.to_owned(),
            "process_names" => {
                self.process_names = value.split(',').map(str::trim).map(str::to_owned).collect()
            }
            "min_cutoff" => self.min_cutoff = parse_float(value)?,
            "beta" => self.beta = parse_float(value)?,
            "derivative_cutoff" => self.derivative_cutoff = parse_float(value)?,
            "deadzone" => self.deadzone = parse_float(value)?,
            "max_delta" => self.max_delta = parse_float(value)?,
            "max_output" => self.max_output = parse_float(value)?,
            "sensitivity" => self.sensitivity = parse_float(value)?,
            "poll_interval_ms" => self.poll_interval_ms = parse_integer(value)?,
            "recenter_settle_ms" => self.recenter_settle_ms = parse_integer(value)?,
            "config_reload_secs" => self.config_reload_secs = parse_integer(value)?,
            _ => return Err(format!("未知字段 {key}")),
        }
        Ok(())
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("无效布尔值 {value}")),
    }
}

fn parse_float(value: &str) -> Result<f64, String> {
    let value = value
        .parse::<f64>()
        .map_err(|_| format!("无效数字 {value}"))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("数字必须有限 {value}"))
    }
}

fn parse_integer(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("无效整数 {value}"))
}

fn finite_clamp(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn default_config_path() -> PathBuf {
    if let Ok(app_data) = std::env::var("APPDATA") {
        return PathBuf::from(app_data)
            .join("RemoteCamera")
            .join("config.cfg");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("remote-camera")
            .join("config.cfg");
    }
    PathBuf::from("remote-camera.cfg")
}

pub fn config_from_path(path: impl AsRef<Path>) -> Config {
    Config::load_from_path(Config {
        config_path: path.as_ref().to_path_buf(),
        ..Config::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_lines_do_not_destroy_defaults() {
        let mut config = Config::default();
        assert!(config.apply_line("smoothing=0.4").is_err());
        assert_eq!(config.min_cutoff, 1.2);
        assert!(config.apply_line("min_cutoff=2.5").is_ok());
        assert_eq!(config.min_cutoff, 2.5);
    }

    #[test]
    fn sanitize_restores_safe_ranges() {
        let mut config = Config {
            min_cutoff: -1.0,
            max_output: f64::NAN,
            process_names: vec![" JAVA.EXE ".to_owned(), "".to_owned()],
            ..Config::default()
        };
        config.sanitize();
        assert_eq!(config.min_cutoff, 0.05);
        assert_eq!(config.max_output, 40.0);
        assert_eq!(config.process_names, vec!["java.exe"]);
    }

    #[test]
    fn config_text_round_trips_core_values() {
        let original = Config {
            beta: 0.11,
            target_scope: "desktop".to_owned(),
            process_names: vec!["java.exe".to_owned()],
            ..Config::default()
        };
        let mut parsed = Config::default();
        for line in original.to_text().lines() {
            parsed.apply_line(line).unwrap();
        }
        parsed.sanitize();
        assert_eq!(parsed.beta, original.beta);
        assert_eq!(parsed.target_scope, original.target_scope);
        assert_eq!(parsed.process_names, original.process_names);
    }

    #[test]
    fn unknown_target_scope_falls_back_to_minecraft() {
        let mut config = Config {
            target_scope: "anything".to_owned(),
            ..Config::default()
        };
        config.sanitize();
        assert_eq!(config.target_scope, "minecraft");
    }
}
