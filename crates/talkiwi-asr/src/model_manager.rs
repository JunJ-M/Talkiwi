//! Model management — path resolution, existence checking, and download.
//!
//! Provides utilities for managing Whisper GGML model files:
//! - Path resolution from config or default data directory
//! - Model availability checking
//! - Download URL generation for HuggingFace-hosted models
//! - Async model download with progress reporting (behind `download` feature)

use std::path::{Path, PathBuf};

/// Supported Whisper model sizes with their approximate file sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl ModelSize {
    /// Parse a model size string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "tiny" => Some(Self::Tiny),
            "base" => Some(Self::Base),
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" | "large-v3" => Some(Self::Large),
            _ => None,
        }
    }

    /// Returns the model size as a string used in file names and URLs.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large-v3",
        }
    }

    /// Approximate model file size in bytes (GGML format).
    pub fn approx_size_bytes(&self) -> u64 {
        match self {
            Self::Tiny => 75_000_000,
            Self::Base => 142_000_000,
            Self::Small => 466_000_000,
            Self::Medium => 1_500_000_000,
            Self::Large => 2_900_000_000,
        }
    }

    /// Human-readable size string.
    pub fn approx_size_display(&self) -> &'static str {
        match self {
            Self::Tiny => "~75 MB",
            Self::Base => "~142 MB",
            Self::Small => "~466 MB",
            Self::Medium => "~1.5 GB",
            Self::Large => "~2.9 GB",
        }
    }
}

/// Status of a model on disk.
#[derive(Debug, Clone)]
pub struct ModelStatus {
    /// Whether the model file exists.
    pub exists: bool,
    /// Full path to the model file.
    pub path: PathBuf,
    /// Model size name (e.g., "small").
    pub size_name: String,
    /// Actual file size in bytes (0 if not exists).
    pub file_size_bytes: u64,
}

/// Resolve the model file path from config or defaults.
///
/// Priority:
/// 1. `explicit_path` if provided (from config `whisper_model_path`)
/// 2. `data_dir/models/ggml-{size}.bin`
pub fn resolve_model_path(
    explicit_path: Option<&str>,
    model_size: &str,
    data_dir: &Path,
) -> PathBuf {
    if let Some(path) = explicit_path {
        PathBuf::from(path)
    } else {
        data_dir
            .join("models")
            .join(format!("ggml-{}.bin", model_size))
    }
}

/// Check model status on disk.
pub fn check_model_status(
    explicit_path: Option<&str>,
    model_size: &str,
    data_dir: &Path,
) -> ModelStatus {
    let path = resolve_model_path(explicit_path, model_size, data_dir);
    let (exists, file_size_bytes) = if path.exists() {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        (true, size)
    } else {
        (false, 0)
    };

    ModelStatus {
        exists,
        path,
        size_name: model_size.to_string(),
        file_size_bytes,
    }
}

/// Generate the download URL for a Whisper GGML model.
///
/// Uses HuggingFace's whisper.cpp model repository as the canonical source.
pub fn model_download_url(size: &str) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        size
    )
}

/// Download progress callback type.
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send>;

/// Download a whisper model to the specified path.
///
/// Creates parent directories if they don't exist.
/// Reports progress via the callback: `(bytes_downloaded, total_bytes)`.
///
/// Only available with the `download` feature.
#[cfg(feature = "download")]
pub async fn download_model(
    url: &str,
    dest: &Path,
    progress: Option<ProgressCallback>,
) -> anyhow::Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    tracing::info!(url = %url, dest = %dest.display(), "starting model download");

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        anyhow::bail!("download failed with status {}: {}", response.status(), url);
    }

    let total_size = response.content_length().unwrap_or(0);

    // Write to a temp file first, then rename to avoid partial files
    let temp_path = dest.with_extension("bin.downloading");
    let mut file = tokio::fs::File::create(&temp_path).await?;
    let mut downloaded: u64 = 0;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if let Some(ref cb) = progress {
            cb(downloaded, total_size);
        }
    }

    file.flush().await?;
    drop(file);

    // Atomic rename
    tokio::fs::rename(&temp_path, dest).await?;

    tracing::info!(
        dest = %dest.display(),
        size_bytes = downloaded,
        "model download complete"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn model_size_parsing() {
        assert_eq!(ModelSize::parse("tiny"), Some(ModelSize::Tiny));
        assert_eq!(ModelSize::parse("Small"), Some(ModelSize::Small));
        assert_eq!(ModelSize::parse("LARGE"), Some(ModelSize::Large));
        assert_eq!(ModelSize::parse("xxl"), None);
    }

    #[test]
    fn model_size_roundtrip() {
        for size in [
            ModelSize::Tiny,
            ModelSize::Base,
            ModelSize::Small,
            ModelSize::Medium,
            ModelSize::Large,
        ] {
            let name = size.as_str();
            // large-v3 parses back to Large
            let parsed = ModelSize::parse(name);
            assert_eq!(parsed, Some(size), "roundtrip failed for {name}");
        }
    }

    #[test]
    fn resolve_model_path_explicit() {
        let path = resolve_model_path(Some("/custom/path/model.bin"), "small", Path::new("/data"));
        assert_eq!(path, PathBuf::from("/custom/path/model.bin"));
    }

    #[test]
    fn resolve_model_path_default() {
        let path = resolve_model_path(None, "small", Path::new("/data"));
        assert_eq!(path, PathBuf::from("/data/models/ggml-small.bin"));
    }

    #[test]
    fn model_download_url_format() {
        let url = model_download_url("small");
        assert!(url.contains("ggml-small.bin"));
        assert!(url.starts_with("https://huggingface.co/"));
    }

    #[test]
    fn check_model_status_nonexistent() {
        let status = check_model_status(None, "small", Path::new("/nonexistent"));
        assert!(!status.exists);
        assert_eq!(status.file_size_bytes, 0);
        assert_eq!(status.size_name, "small");
    }
}
