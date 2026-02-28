use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::catalog::{SoftwareSpec, SourceSpec};
use crate::distro::{DistroInfo, PackageManager};

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedAsset {
    pub version: String,
    pub url: String,
    pub file_name: String,
}

pub fn resolve_asset(client: &Client, spec: &SoftwareSpec, distro: &DistroInfo) -> Result<ResolvedAsset, String> {
    match &spec.source {
        SourceSpec::OfficialSource { id, url, version_regex, download_url_regex } => {
            match id.as_deref() {
                Some("flutter") => resolve_flutter(client, "stable"),
                Some("android_studio") => resolve_android_studio(client),
                Some("vscode") => resolve_vscode(client, distro),
                _ => {
                    if let (Some(u), Some(v_re), Some(d_re)) = (url, version_regex, download_url_regex) {
                        resolve_generic_scraper(client, u, v_re, d_re)
                    } else if let (Some(u), None, None) = (url, version_regex, download_url_regex) {
                        resolve_static(u, "download")
                    } else {
                        Err("official_source missing valid configuration".into())
                    }
                }
            }
        },
        SourceSpec::PackageManager => resolve_package_only(spec, distro),
        SourceSpec::Github { repo, asset_pattern } => resolve_github(client, repo, asset_pattern, distro),
    }
}

#[derive(Debug, Deserialize)]
struct FlutterReleases {
    current_release: std::collections::HashMap<String, String>,
    releases: Vec<FlutterRelease>,
}

#[derive(Debug, Deserialize)]
struct FlutterRelease {
    hash: String,
    version: String,
    archive: String,
}

fn resolve_flutter(client: &Client, channel: &str) -> Result<ResolvedAsset, String> {
    let endpoint = "https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json";
    let payload: FlutterReleases = client
        .get(endpoint)
        .send()
        .map_err(|e| format!("failed to fetch flutter releases: {e}"))?
        .json()
        .map_err(|e| format!("failed to decode flutter releases json: {e}"))?;

    let hash = payload
        .current_release
        .get(channel)
        .ok_or_else(|| format!("missing current release hash for channel '{channel}'"))?;

    let release = payload
        .releases
        .iter()
        .find(|it| &it.hash == hash)
        .ok_or_else(|| "failed to resolve flutter release by hash".to_string())?;

    Ok(ResolvedAsset {
        version: release.version.clone(),
        url: format!(
            "https://storage.googleapis.com/flutter_infra_release/releases/{}",
            release.archive
        ),
        file_name: release
            .archive
            .rsplit('/')
            .next()
            .unwrap_or("flutter.tar.xz")
            .to_string(),
    })
}

fn resolve_android_studio(client: &Client) -> Result<ResolvedAsset, String> {
    let html = client
        .get("https://developer.android.com/studio")
        .send()
        .map_err(|e| format!("failed to fetch android studio page: {e}"))?
        .text()
        .map_err(|e| format!("failed reading android studio html: {e}"))?;

    let patterns = [
        r#"https://redirector\.gvt1\.com/edgedl/android/studio/ide-zips/[^"']+linux\.tar\.gz"#,
        r#"https://[^\s"']+android-studio-[^\s"']+linux\.tar\.gz"#,
    ];

    for pattern in &patterns {
        let re = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => return Err(format!("failed to compile android studio regex: {e}")),
        };
        if let Some(url_match) = re.find(&html) {
            let url = url_match.as_str().to_string();
            let file_name = url
                .rsplit('/')
                .next()
                .ok_or_else(|| "invalid android studio url".to_string())?
                .to_string();

            let version = file_name
                .trim_start_matches("android-studio-")
                .trim_end_matches("-linux.tar.gz")
                .to_string();

            return Ok(ResolvedAsset {
                version,
                url,
                file_name,
            });
        }
    }

    Err("could not resolve android studio linux tarball link from developer.android.com".to_string())
}

fn resolve_vscode(client: &Client, distro: &DistroInfo) -> Result<ResolvedAsset, String> {
    let platform = match distro.pkg_manager {
        PackageManager::Apt => "linux-deb-x64",
        PackageManager::Dnf => "linux-rpm-x64",
        _ => "linux-x64",
    };

    let base_url = format!("https://update.code.visualstudio.com/latest/{}/stable", platform);
    let resp = client.get(&base_url)
        .send()
        .map_err(|e| format!("failed to fetch vscode redirect: {e}"))?;

    let final_url = resp.url().as_str().to_string();
    let file_name = final_url.split('/').last().unwrap_or("vscode_latest").to_string();

    let version_re = Regex::new(r"(\d+\.\d+\.\d+)").unwrap();
    let version = version_re.find(&file_name)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "latest".to_string());

    Ok(ResolvedAsset {
        version,
        url: final_url,
        file_name,
    })
}

fn resolve_static(url: &str, file_name: &str) -> Result<ResolvedAsset, String> {
    Ok(ResolvedAsset {
        version: "static".to_string(),
        url: url.to_string(),
        file_name: file_name.to_string(),
    })
}

fn resolve_package_only(spec: &SoftwareSpec, distro: &DistroInfo) -> Result<ResolvedAsset, String> {
    let package_name = spec.setup_steps.iter().find_map(|s| {
        if let crate::catalog::SetupStep::Package { packages } = s {
            packages.first()
        } else {
            None
        }
    }).map(|s| s.as_str()).unwrap_or("unknown");

    let version = distro.pkg_manager.get_package_version(package_name)
        .unwrap_or_else(|| "package-manager".to_string());

    Ok(ResolvedAsset {
        version,
        url: "N/A".to_string(),
        file_name: "N/A".to_string(),
    })
}

fn resolve_generic_scraper(
    client: &Client,
    url: &str,
    version_regex: &str,
    download_url_regex: &str,
) -> Result<ResolvedAsset, String> {
    let html = client
        .get(url)
        .send()
        .map_err(|e| format!("failed to fetch {url}: {e}"))?
        .text()
        .map_err(|e| format!("failed reading {url} html: {e}"))?;

    let sys_arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    };
    let dash_arch = std::env::consts::ARCH.replace('_', "-");

    let processed_v_re = version_regex
        .replace("{arch}", sys_arch)
        .replace("{xarch}", std::env::consts::ARCH)
        .replace("{xarch_dash}", &dash_arch);
    let processed_d_re = download_url_regex
        .replace("{arch}", sys_arch)
        .replace("{xarch}", std::env::consts::ARCH)
        .replace("{xarch_dash}", &dash_arch);

    let v_re = Regex::new(&processed_v_re).map_err(|e| format!("invalid version regex: {e}"))?;
    let d_re = Regex::new(&processed_d_re).map_err(|e| format!("invalid download url regex: {e}"))?;

    let version = v_re
        .captures(&html)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .ok_or_else(|| format!("could not find version on {} using regex {}", url, version_regex))?;

    let download_url = d_re
        .find(&html)
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| format!("could not find download url on {} using regex {}", url, download_url_regex))?;

    let final_url = if let Ok(base) = reqwest::Url::parse(url) {
        base.join(&download_url).map(|u| u.to_string()).unwrap_or(download_url)
    } else {
        download_url
    };

    let file_name = final_url
        .split('/')
        .last()
        .unwrap_or("downloaded_file")
        .to_string();

    Ok(ResolvedAsset {
        version,
        url: final_url,
        file_name,
    })
}

fn resolve_github(client: &Client, repo_opt: &Option<String>, asset_pattern: &str, distro: &DistroInfo) -> Result<ResolvedAsset, String> {
    let repo = repo_opt.as_ref()
        .ok_or_else(|| "github repo not configured for this software".to_string())?;

    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let release: GitHubRelease = client
        .get(&api_url)
        .header("User-Agent", "rusty_rebase")
        .send()
        .map_err(|e| format!("failed to fetch latest release from {api_url}: {e}"))?
        .json()
        .map_err(|e| format!("failed to decode github release json: {e}"))?;

    let re = Regex::new(asset_pattern).map_err(|e| format!("invalid asset pattern regex: {e}"))?;
    let mut matched: Vec<&GitHubAsset> = release.assets.iter()
        .filter(|a| re.is_match(&a.name))
        .collect();

    if matched.is_empty() {
        return Err(format!("no asset matching '{}' found in github:{}", asset_pattern, repo));
    }

    let sys_arch = std::env::consts::ARCH;
    let preferred_ext = match distro.pkg_manager {
        crate::distro::PackageManager::Apt => ".deb",
        crate::distro::PackageManager::Dnf => ".rpm",
        _ => "___",
    };

    let score = |name: &str| -> i32 {
        let mut s = 0;
        let name_lower = name.to_lowercase();
        
        // Arch match (higher priority)
        let has_arch = match sys_arch {
            "x86_64" => name_lower.contains("x86_64") || name_lower.contains("x86-64") || name_lower.contains("amd64") || name_lower.contains("x64"),
            "aarch64" => name_lower.contains("aarch64") || name_lower.contains("arm64") || name_lower.contains("arm-64"),
            "arm" => name_lower.contains("armv7") || name_lower.contains("armhf") || (name_lower.contains("arm") && !name_lower.contains("64")),
            "x86" => name_lower.contains("i386") || name_lower.contains("x86") || name_lower.contains("386"),
            _ => false,
        };
        if has_arch { s += 100; }

        // Extension match
        if name.ends_with(preferred_ext) { s += 50; }
        else if name.ends_with(".deb") || name.ends_with(".rpm") { s += 20; }
        else if name.ends_with(".AppImage") { s += 10; }
        else if name.ends_with(".tar.gz") || name.ends_with(".tar.xz") || name.ends_with(".zip") { s += 5; }

        s
    };

    matched.sort_by_key(|a| std::cmp::Reverse(score(&a.name)));
    let asset = matched[0];

    Ok(ResolvedAsset {
        version: release.tag_name.trim_start_matches('v').to_string(),
        url: asset.browser_download_url.clone(),
        file_name: asset.name.clone(),
    })
}