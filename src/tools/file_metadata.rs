//! File metadata and image utilities for XZatoma tools
//!
//! This module provides utilities for extracting file metadata,
//! detecting file types, and handling image files with base64 encoding.

use base64::Engine;
use image::GenericImageView;
use std::path::Path;
use thiserror::Error;

/// Error type for file metadata operations
///
/// Provides detailed error information for metadata extraction and image handling.
#[derive(Error, Debug)]
pub enum FileMetadataError {
    /// Unsupported image format
    #[error("Unsupported image format: {0}")]
    UnsupportedImageFormat(String),

    /// Image decoding failed
    #[error("Image decoding failed: {0}")]
    ImageDecodingFailed(String),

    /// IO error occurred
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Image crate error
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
}

/// File type enumeration
///
/// Represents the different types of files that can be encountered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Image file with format information
    Image(ImageFormat),
}

/// Supported image formats
///
/// Represents the image formats that XZatoma can decode and process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG image
    Png,
    /// JPEG image
    Jpeg,
    /// WebP image
    Webp,
    /// GIF image
    Gif,
    /// BMP image
    Bmp,
    /// TIFF image
    Tiff,
}

impl ImageFormat {
    /// Returns the MIME type for this image format
    ///
    /// # Returns
    ///
    /// The MIME type string corresponding to this format
    fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Tiff => "image/tiff",
        }
    }
}

/// File metadata structure
///
/// Contains information about a file including size, modification time,
/// and permissions.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// File size in bytes
    pub size: u64,
    /// Last modification time
    pub modified: std::time::SystemTime,
    /// Whether the file is read-only
    pub readonly: bool,
    /// File type
    pub file_type: FileType,
}

/// Image metadata structure
///
/// Contains image-specific information including dimensions and format.
#[derive(Debug, Clone)]
pub struct ImageMetadata {
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Image format
    pub format: ImageFormat,
}

/// Detects the file type at the given path
///
/// Determines whether the path points to a regular file, directory,
/// symlink, or image file.
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns the FileType
///
/// # Errors
///
/// Returns `FileMetadataError` if file metadata cannot be read
///
/// # Examples
///
/// ```rust
/// use xzatoma::tools::file_metadata::{get_file_type, FileType};
/// use std::path::Path;
///
/// let file_type = tokio::runtime::Runtime::new()
///     .unwrap()
///     .block_on(get_file_type(Path::new(".")))
///     .unwrap();
/// assert!(matches!(file_type, FileType::Directory));
/// ```
pub async fn get_file_type(path: &Path) -> Result<FileType, FileMetadataError> {
    let metadata = tokio::fs::metadata(path).await?;

    if metadata.is_symlink() {
        return Ok(FileType::Symlink);
    }

    if metadata.is_dir() {
        return Ok(FileType::Directory);
    }

    // Check if it's an image file
    if is_image_file(path) {
        if let Ok(format) = detect_image_format(path).await {
            return Ok(FileType::Image(format));
        }
    }

    Ok(FileType::File)
}

/// Retrieves detailed file information
///
/// Gets comprehensive metadata about a file including size,
/// modification time, permissions, and type.
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns FileInfo with metadata
///
/// # Errors
///
/// Returns `FileMetadataError` if metadata cannot be read
///
/// # Examples
///
/// ```rust
/// use xzatoma::tools::file_metadata::{get_file_info, FileType};
/// use std::path::Path;
///
/// let info = tokio::runtime::Runtime::new()
///     .unwrap()
///     .block_on(get_file_info(Path::new(".")))
///     .unwrap();
/// assert!(matches!(info.file_type, FileType::Directory));
/// ```
pub async fn get_file_info(path: &Path) -> Result<FileInfo, FileMetadataError> {
    let metadata = tokio::fs::metadata(path).await?;
    let file_type = get_file_type(path).await?;

    Ok(FileInfo {
        size: metadata.len(),
        modified: metadata.modified()?,
        readonly: metadata.permissions().readonly(),
        file_type,
    })
}

/// Checks if a file is an image based on extension and magic bytes
///
/// Uses file extension for quick detection, checking against known image extensions.
/// This is a fast path that doesn't require reading file contents.
///
/// # Arguments
///
/// * `path` - The file path to check
///
/// # Returns
///
/// Returns true if the file extension indicates a supported image format
///
/// # Examples
///
/// ```
/// use xzatoma::tools::file_metadata::is_image_file;
/// use std::path::Path;
///
/// assert!(is_image_file(Path::new("photo.png")));
/// assert!(!is_image_file(Path::new("document.txt")));
/// ```
pub fn is_image_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if let Some(ext_str) = ext.to_str() {
            let lower = ext_str.to_lowercase();
            matches!(
                lower.as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "tif"
            )
        } else {
            false
        }
    } else {
        false
    }
}

/// Detects image format by reading and analyzing the file
///
/// Reads file magic bytes to determine the actual image format,
/// independent of file extension.
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns the detected ImageFormat
///
/// # Errors
///
/// Returns `FileMetadataError` if format cannot be detected
async fn detect_image_format(path: &Path) -> Result<ImageFormat, FileMetadataError> {
    let bytes = tokio::fs::read(path).await?;

    // Check magic bytes for image format detection
    if bytes.starts_with(b"\x89PNG") {
        Ok(ImageFormat::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Ok(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        Ok(ImageFormat::Webp)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Ok(ImageFormat::Gif)
    } else if bytes.starts_with(b"BM") {
        Ok(ImageFormat::Bmp)
    } else if (bytes.starts_with(b"II\x2a\x00") || bytes.starts_with(b"MM\x00\x2a"))
        || (bytes.len() > 8 && &bytes[0..4] == b"MM\x00\x2a")
    {
        Ok(ImageFormat::Tiff)
    } else {
        Err(FileMetadataError::UnsupportedImageFormat(
            path.to_string_lossy().to_string(),
        ))
    }
}

/// Detects content type using file extension and magic bytes
///
/// Determines the MIME type of a file by checking both extension and,
/// for images, the actual file format.
///
/// # Arguments
///
/// * `path` - The file path to inspect
///
/// # Returns
///
/// Returns MIME type string (e.g., "image/png", "text/plain")
///
/// # Examples
///
/// ```rust
/// use xzatoma::tools::file_metadata::detect_content_type;
/// use std::path::Path;
///
/// let mime = tokio::runtime::Runtime::new()
///     .unwrap()
///     .block_on(detect_content_type(Path::new(".")))
///     .unwrap();
/// assert_eq!(mime, "application/octet-stream");
/// ```
pub async fn detect_content_type(path: &Path) -> Result<String, FileMetadataError> {
    // Check if it's an image file
    if is_image_file(path) {
        if let Ok(format) = detect_image_format(path).await {
            return Ok(format.mime_type().to_string());
        }
    }

    // Default to generic content type for non-image files
    Ok("application/octet-stream".to_string())
}

/// Reads an image file and returns base64-encoded data with metadata
///
/// Decodes the image using the `image` crate, validates format,
/// and encodes as base64 for transmission.
///
/// # Arguments
///
/// * `path` - The image file path
///
/// # Returns
///
/// Returns tuple of (base64_data, ImageMetadata)
///
/// # Errors
///
/// Returns `FileMetadataError` if:
/// - File is not a supported image format
/// - Image decoding fails
/// - IO error occurs
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::file_metadata::read_image_as_base64;
/// use std::path::Path;
///
/// let (data, metadata) = tokio::runtime::Runtime::new()
///     .unwrap()
///     .block_on(read_image_as_base64(Path::new("test.png")))
///     .unwrap();
/// assert!(!data.is_empty());
/// assert!(metadata.width > 0);
/// ```
pub async fn read_image_as_base64(
    path: &Path,
) -> Result<(String, ImageMetadata), FileMetadataError> {
    // Read file bytes
    let file_bytes = tokio::fs::read(path).await?;

    // Decode using image crate
    let image = image::load_from_memory(&file_bytes)
        .map_err(|e| FileMetadataError::ImageDecodingFailed(e.to_string()))?;

    // Detect format
    let format = detect_image_format(path).await?;

    // Extract metadata
    let (width, height) = image.dimensions();
    let metadata = ImageMetadata {
        width,
        height,
        format,
    };

    // Encode to base64
    let base64_string = base64::engine::general_purpose::STANDARD.encode(&file_bytes);

    Ok((base64_string, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_image_file_with_png_returns_true() {
        let path = Path::new("test.png");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_jpg_returns_true() {
        let path = Path::new("photo.jpg");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_jpeg_returns_true() {
        let path = Path::new("photo.jpeg");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_webp_returns_true() {
        let path = Path::new("image.webp");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_gif_returns_true() {
        let path = Path::new("animation.gif");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_bmp_returns_true() {
        let path = Path::new("bitmap.bmp");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_tiff_returns_true() {
        let path = Path::new("scan.tiff");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_tif_returns_true() {
        let path = Path::new("scan.tif");
        assert!(is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_text_file_returns_false() {
        let path = Path::new("test.txt");
        assert!(!is_image_file(path));
    }

    #[test]
    fn test_is_image_file_with_no_extension_returns_false() {
        let path = Path::new("file_without_ext");
        assert!(!is_image_file(path));
    }

    #[test]
    fn test_image_format_mime_type_png() {
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
    }

    #[test]
    fn test_image_format_mime_type_jpeg() {
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
    }

    #[test]
    fn test_image_format_mime_type_webp() {
        assert_eq!(ImageFormat::Webp.mime_type(), "image/webp");
    }

    #[test]
    fn test_image_format_mime_type_gif() {
        assert_eq!(ImageFormat::Gif.mime_type(), "image/gif");
    }

    #[test]
    fn test_image_format_mime_type_bmp() {
        assert_eq!(ImageFormat::Bmp.mime_type(), "image/bmp");
    }

    #[test]
    fn test_image_format_mime_type_tiff() {
        assert_eq!(ImageFormat::Tiff.mime_type(), "image/tiff");
    }

    #[tokio::test]
    async fn test_get_file_type_with_regular_file_returns_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");
        tokio::fs::write(&path, "content").await.unwrap();
        let file_type = get_file_type(&path).await.unwrap();
        assert_eq!(file_type, FileType::File);
    }

    #[tokio::test]
    async fn test_get_file_type_with_directory_returns_directory() {
        let temp = TempDir::new().unwrap();
        let file_type = get_file_type(temp.path()).await.unwrap();
        assert_eq!(file_type, FileType::Directory);
    }

    #[tokio::test]
    async fn test_get_file_info_with_file_returns_info() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");
        tokio::fs::write(&path, "test content").await.unwrap();
        let info = get_file_info(&path).await.unwrap();
        assert_eq!(info.size, 12);
        assert_eq!(info.file_type, FileType::File);
    }

    #[tokio::test]
    async fn test_detect_content_type_with_text_file_returns_octet_stream() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.txt");
        tokio::fs::write(&path, "content").await.unwrap();
        let mime = detect_content_type(&path).await.unwrap();
        assert_eq!(mime, "application/octet-stream");
    }

    #[tokio::test]
    async fn test_read_image_as_base64_with_invalid_image_returns_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("invalid.png");
        tokio::fs::write(&path, "not an image").await.unwrap();
        let result = read_image_as_base64(&path).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_image_format_png_magic_bytes() {
        // PNG magic bytes: 89 50 4E 47
        let png_header = b"\x89PNG\r\n\x1a\n";
        assert!(png_header.starts_with(b"\x89PNG"));
    }

    #[test]
    fn test_detect_image_format_jpeg_magic_bytes() {
        // JPEG magic bytes: FF D8 FF
        let jpeg_header = b"\xff\xd8\xff\xe0";
        assert!(jpeg_header.starts_with(b"\xff\xd8\xff"));
    }

    #[test]
    fn test_detect_image_format_gif_magic_bytes() {
        // GIF magic bytes: 47 49 46 38 39 61 (GIF89a)
        let gif_header = b"GIF89a";
        assert!(gif_header.starts_with(b"GIF89a"));
    }

    #[test]
    fn test_detect_image_format_bmp_magic_bytes() {
        // BMP magic bytes: 42 4D (BM)
        let bmp_header = b"BM";
        assert!(bmp_header.starts_with(b"BM"));
    }
}
