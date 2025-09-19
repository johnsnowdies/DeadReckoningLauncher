use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::Result;
use reqwest::blocking::Client;
use semver::Version;
use zip::ZipArchive;

use crate::app_config::AppConfig;

#[derive(Debug)]
pub enum UpdaterError {
    NetworkError(String),
    VersionParseError(String),
    FileSystemError(String),
    ZipExtractionError(String),
    NoUpdateUrlConfigured,
    NoUpdatesAvailable,
}

impl std::fmt::Display for UpdaterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdaterError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            UpdaterError::VersionParseError(msg) => write!(f, "Version parse error: {}", msg),
            UpdaterError::FileSystemError(msg) => write!(f, "File system error: {}", msg),
            UpdaterError::ZipExtractionError(msg) => write!(f, "Zip extraction error: {}", msg),
            UpdaterError::NoUpdateUrlConfigured => write!(f, "No update URL configured"),
            UpdaterError::NoUpdatesAvailable => write!(f, "No updates available"),
        }
    }
}

impl std::error::Error for UpdaterError {}

#[derive(Debug)]
pub struct Updater {
    config: AppConfig,
    client: Client,
    updates_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PatchInfo {
    pub version: Version,
    pub download_url: String,
}

#[derive(Debug)]
pub enum UpdateProgress {
    CheckingForUpdates,
    UpdatesAvailable(Vec<PatchInfo>),
    Downloading {
        current: usize,
        total: usize,
        version: String,
        progress: f32, // 0.0 to 1.0
    },
    Extracting {
        current: usize,
        total: usize,
        version: String,
    },
    Complete,
    Error(UpdaterError),
}

impl Updater {
    pub fn new(config: AppConfig) -> Result<Self, UpdaterError> {
        let client = Client::new();
        
        // Создаем директорию для обновлений
        let mut updates_dir = std::env::current_dir()
            .map_err(|e| UpdaterError::FileSystemError(format!("Failed to get current directory: {}", e)))?;
        updates_dir.push("updates");
        
        if !updates_dir.exists() {
            fs::create_dir_all(&updates_dir)
                .map_err(|e| UpdaterError::FileSystemError(format!("Failed to create updates directory: {}", e)))?;
        }
        
        Ok(Updater {
            config,
            client,
            updates_dir,
        })
    }
    
    pub fn check_for_updates(&self) -> Result<Vec<PatchInfo>, UpdaterError> {
        let update_url = self.config.update_url.as_ref().ok_or(UpdaterError::NoUpdateUrlConfigured)?;
        
        // Получаем список доступных патчей
        let response = self.client.get(update_url)
            .send()
            .map_err(|e| UpdaterError::NetworkError(format!("Failed to fetch update list: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(UpdaterError::NetworkError(format!("Server returned error: {}", response.status())));
        }
        
        let content = response.text()
            .map_err(|e| UpdaterError::NetworkError(format!("Failed to read response: {}", e)))?;
        
        // Парсим строки как URL-ы патчей
        let mut available_patches = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue; // Пропускаем пустые строки и комментарии
            }
            
            // Предполагаем формат имени файла: patch-X.Y.Z.zip
            if let Some(file_name) = Path::new(line).file_name() {
                if let Some(file_name_str) = file_name.to_str() {
                    if file_name_str.starts_with("patch-") && file_name_str.ends_with(".zip") {
                        let version_str = &file_name_str[6..file_name_str.len() - 4]; // Убираем 'patch-' и '.zip'
                        match Version::parse(version_str) {
                            Ok(version) => {
                                available_patches.push(PatchInfo {
                                    version,
                                    download_url: line.to_string(),
                                });
                            },
                            Err(_) => continue, // Пропускаем некорректные версии
                        }
                    }
                }
            }
        }
        
        // Сортируем патчи по версии
        available_patches.sort_by(|a, b| a.version.cmp(&b.version));
        
        if available_patches.is_empty() {
            return Err(UpdaterError::NoUpdatesAvailable);
        }
        
        Ok(available_patches)
    }
    
    pub fn download_patch(&self, patch: &PatchInfo, progress_callback: &mut dyn FnMut(UpdateProgress)) 
        -> Result<PathBuf, UpdaterError> {
        let file_name = format!("patch-{}.zip", patch.version);
        let output_path = self.updates_dir.join(&file_name);
        
        // Создаем временный файл
        let mut output_file = File::create(&output_path)
            .map_err(|e| UpdaterError::FileSystemError(format!("Failed to create output file: {}", e)))?;
        
        // Скачиваем файл
        let mut response = self.client.get(&patch.download_url)
            .send()
            .map_err(|e| UpdaterError::NetworkError(format!("Failed to download patch: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(UpdaterError::NetworkError(format!("Server returned error: {}", response.status())));
        }
        
        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded = 0;
        let mut buffer = [0u8; 8192];
        
        while let Ok(n) = response.read(&mut buffer) {
            if n == 0 {
                break;
            }
            
            output_file.write_all(&buffer[..n])
                .map_err(|e| UpdaterError::FileSystemError(format!("Failed to write to file: {}", e)))?;
            
            downloaded += n as u64;
            
            if total_size > 0 {
                let progress = downloaded as f32 / total_size as f32;
                progress_callback(UpdateProgress::Downloading {
                    current: 1, // Мы скачиваем по одному файлу за раз
                    total: 1,
                    version: patch.version.to_string(),
                    progress,
                });
            }
        }
        
        Ok(output_path)
    }
    
    pub fn apply_patch(&self, patch_path: &Path, progress_callback: &mut dyn FnMut(UpdateProgress)) 
        -> Result<(), UpdaterError> {
        let file = File::open(patch_path)
            .map_err(|e| UpdaterError::FileSystemError(format!("Failed to open patch file: {}", e)))?;
        
        let mut archive = ZipArchive::new(file)
            .map_err(|e| UpdaterError::ZipExtractionError(format!("Failed to open zip archive: {}", e)))?;
        
        let total_files = archive.len();
        
        // Получаем имя файла патча для извлечения версии
        let file_name = patch_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| UpdaterError::ZipExtractionError(format!("Failed to access file in archive: {}", e)))?;
            
            let outpath = match file.enclosed_name() {
                Some(path) => path.to_owned(),
                None => continue,
            };
            
            // Информируем о прогрессе
            progress_callback(UpdateProgress::Extracting {
                current: i + 1,
                total: total_files,
                version: file_name.to_string(),
            });
            
            if file.is_dir() {
                fs::create_dir_all(&outpath)
                    .map_err(|e| UpdaterError::FileSystemError(format!("Failed to create directory: {}", e)))?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)
                            .map_err(|e| UpdaterError::FileSystemError(format!("Failed to create parent directory: {}", e)))?;
                    }
                }
                
                let mut outfile = File::create(&outpath)
                    .map_err(|e| UpdaterError::FileSystemError(format!("Failed to create output file: {}", e)))?;
                
                io::copy(&mut file, &mut outfile)
                    .map_err(|e| UpdaterError::FileSystemError(format!("Failed to write output file: {}", e)))?;
            }
        }
        
        Ok(())
    }
    
    pub fn update(&mut self, mut progress_callback: impl FnMut(UpdateProgress)) -> Result<String, UpdaterError> {
        progress_callback(UpdateProgress::CheckingForUpdates);
        
        // Получаем текущую версию
        let current_version = self.config.version
            .as_ref()
            .ok_or_else(|| UpdaterError::VersionParseError("No version in config".to_string()))
            .and_then(|v| Version::parse(v).map_err(|e| UpdaterError::VersionParseError(e.to_string())))?;
        
        // Получаем доступные патчи от сервера
        let patches = self.check_for_updates()?;
        progress_callback(UpdateProgress::UpdatesAvailable(patches.clone()));
        
        // Отфильтруем только те патчи, версии которых выше текущей
        let mut applicable_patches: Vec<&PatchInfo> = patches.iter()
            .filter(|patch| patch.version > current_version)
            .collect();
        
        // Сортируем патчи по версии от низшей к высшей
        applicable_patches.sort_by(|a, b| a.version.cmp(&b.version));
        
        // Проверяем, есть ли патчи для установки
        if applicable_patches.is_empty() {
            return Err(UpdaterError::NoUpdatesAvailable);
        }
        
        // Применяем патчи последовательно
        for (i, patch) in applicable_patches.iter().enumerate() {
            // Скачиваем патч
            let patch_path = self.download_patch(patch, &mut progress_callback)?;
            
            // Применяем патч
            self.apply_patch(&patch_path, &mut progress_callback)?;
            
            // Обновляем версию в конфиге после каждого патча
            if let Some(config_version) = &mut self.config.version {
                *config_version = patch.version.to_string();
            } else {
                self.config.version = Some(patch.version.to_string());
            }
        }
        
        // Возвращаем новую версию
        let new_version = self.config.version.clone().unwrap_or_else(|| "unknown".to_string());
        
        progress_callback(UpdateProgress::Complete);
        
        Ok(new_version)
    }
}