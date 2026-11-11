use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub repos: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub groups: Vec<Group>,
    #[serde(default)]
    pub selected: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            groups: vec![Group {
                name: "기본".to_string(),
                members: Vec::new(),
                repos: Vec::new(),
            }],
            selected: 0,
        }
    }
}

impl Config {
    pub fn current(&self) -> &Group {
        &self.groups[self.selected]
    }

    pub fn current_mut(&mut self) -> &mut Group {
        &mut self.groups[self.selected]
    }
}

/// 작업 그룹 도입 전 구버전 설정({members, repos})도 읽을 수 있는 형태
#[derive(Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    groups: Vec<Group>,
    #[serde(default)]
    selected: usize,
    #[serde(default)]
    members: Vec<String>,
    #[serde(default)]
    repos: Vec<String>,
}

pub fn path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("설정 디렉터리를 찾을 수 없습니다")?
        .join("gh-team");
    Ok(dir.join("config.json"))
}

pub fn load() -> Result<Config> {
    let p = path()?;
    if !p.exists() {
        return Ok(Config::default());
    }
    let raw_str = std::fs::read_to_string(&p)
        .with_context(|| format!("설정 파일을 읽을 수 없습니다: {}", p.display()))?;
    let raw: RawConfig = serde_json::from_str(&raw_str)
        .with_context(|| format!("설정 파일 파싱 실패: {}", p.display()))?;

    let mut cfg = if raw.groups.is_empty() {
        // 구버전 설정은 "기본" 그룹으로 마이그레이션
        Config {
            groups: vec![Group {
                name: "기본".to_string(),
                members: raw.members,
                repos: raw.repos,
            }],
            selected: 0,
        }
    } else {
        Config {
            groups: raw.groups,
            selected: raw.selected,
        }
    };
    if cfg.selected >= cfg.groups.len() {
        cfg.selected = 0;
    }
    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<()> {
    let p = path()?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&p, raw)
        .with_context(|| format!("설정 파일 저장 실패: {}", p.display()))?;
    Ok(())
}
