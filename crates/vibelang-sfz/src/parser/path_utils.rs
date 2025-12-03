use std::path::{Path, PathBuf};

/// Normalize a path string based on the current operating system
///
/// This function handles cross-platform path normalization:
/// - On Windows: Keeps backslashes
/// - On other platforms: Converts backslashes to forward slashes
///
/// # Arguments
///
/// * `path` - The path string to normalize
///
/// # Returns
///
/// * `String` - The normalized path string
///
/// # Example
///
/// ```
/// use sfz_parser::path_utils::normalize_path;
///
/// // On non-Windows systems, backslashes will be converted to forward slashes
/// let path = "samples\\piano\\C4.wav";
/// let normalized = normalize_path(path);
/// 
/// #[cfg(windows)]
/// assert_eq!(normalized, "samples\\piano\\C4.wav");
///
/// #[cfg(not(windows))]
/// assert_eq!(normalized, "samples/piano/C4.wav");
/// ```
pub fn normalize_path(path: &str) -> String {
    if cfg!(windows) {
        path.to_string()
    } else {
        path.replace('\\', "/")
    }
}

/// Combine a default path with a sample path
///
/// This function handles combining a default path with a sample path following
/// SFZ format rules:
/// - If the sample path is absolute, it's used as-is
/// - If the sample path is relative, it's combined with the default path
/// - Path separators are normalized for the current OS
///
/// # Arguments
///
/// * `default_path` - The default path (usually from the control section)
/// * `sample_path` - The sample path (usually from a region section)
///
/// # Returns
///
/// * `PathBuf` - The combined path
///
/// # Example
///
/// ```
/// use sfz_parser::path_utils::combine_sample_path;
/// use std::path::PathBuf;
///
/// let default_path = "samples/piano/";
/// let sample_path = "C4.wav";
///
/// let combined = combine_sample_path(default_path, sample_path);
/// assert_eq!(combined, PathBuf::from("samples/piano/C4.wav"));
/// ```
///
/// When the sample path is absolute, the default path is ignored:
///
/// ```
/// use sfz_parser::path_utils::combine_sample_path;
/// use std::path::PathBuf;
///
/// let default_path = "samples/piano/";
/// 
/// // On Windows, an absolute path starts with a drive letter
/// #[cfg(windows)]
/// {
///     let sample_path = "C:\\samples\\C4.wav";
///     let combined = combine_sample_path(default_path, sample_path);
///     assert_eq!(combined, PathBuf::from("C:\\samples\\C4.wav"));
/// }
///
/// // On Unix-like systems, an absolute path starts with a slash
/// #[cfg(not(windows))]
/// {
///     let sample_path = "/samples/C4.wav";
///     let combined = combine_sample_path(default_path, sample_path);
///     assert_eq!(combined, PathBuf::from("/samples/C4.wav"));
/// }
/// ```
pub fn combine_sample_path(default_path: &str, sample_path: &str) -> PathBuf {
    let normalized_sample_path = normalize_path(sample_path);
    
    // Check if the sample path is absolute
    let path = Path::new(&normalized_sample_path);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    
    // Combine default path with sample path
    let normalized_default_path = normalize_path(default_path);
    let mut combined_path = normalized_default_path;
    
    // Ensure there's a trailing separator
    if !combined_path.is_empty() && !combined_path.ends_with('/') && !combined_path.ends_with('\\') {
        combined_path.push('/');
    }
    
    combined_path.push_str(&normalized_sample_path);
    PathBuf::from(combined_path)
}

/// Resolve a sample path to an absolute path using SFZ resolution rules
///
/// This function fully resolves a sample path considering:
/// 1. The sample path itself
/// 2. The default_path specified in the SFZ file
/// 3. The location of the SFZ file
///
/// It follows these resolution rules:
/// 1. If the sample path is absolute, use it directly
/// 2. If the sample path is relative and default_path is provided, combine them
/// 3. If the resulting path is still relative and sfz_file_path is provided, 
///    resolve it relative to the SFZ file's directory
///
/// # Arguments
///
/// * `sample_path` - The sample path from the region section
/// * `default_path` - Optional default path from the control section
/// * `sfz_file_path` - Optional path to the SFZ file for resolving relative paths
///
/// # Returns
///
/// * `PathBuf` - The fully resolved absolute path
///
/// # Example
///
/// ```
/// use sfz_parser::path_utils::resolve_absolute_path;
/// use std::path::PathBuf;
///
/// // Without default path, relative to SFZ location
/// let sample_path = "samples/piano.wav";
/// let sfz_file_path = Some(PathBuf::from("/music/instruments/piano.sfz"));
/// let resolved = resolve_absolute_path(sample_path, None, sfz_file_path.as_deref());
/// assert_eq!(resolved, PathBuf::from("/music/instruments/samples/piano.wav"));
/// 
/// // With default path and SFZ location
/// let sample_path = "piano.wav";
/// let default_path = Some("samples/");
/// let sfz_file_path = Some(PathBuf::from("/music/instruments/piano.sfz"));
/// let resolved = resolve_absolute_path(sample_path, default_path, sfz_file_path.as_deref());
/// assert_eq!(resolved, PathBuf::from("/music/instruments/samples/piano.wav"));
/// 
/// // Absolute path (ignores default_path and sfz_file_path)
/// #[cfg(not(windows))]
/// {
///     let sample_path = "/absolute/path/piano.wav";
///     let default_path = Some("samples/");
///     let sfz_file_path = Some(PathBuf::from("/music/instruments/piano.sfz"));
///     let resolved = resolve_absolute_path(sample_path, default_path, sfz_file_path.as_deref());
///     assert_eq!(resolved, PathBuf::from("/absolute/path/piano.wav"));
/// }
/// ```
pub fn resolve_absolute_path(sample_path: &str, default_path: Option<&str>, sfz_file_path: Option<&Path>) -> PathBuf {
    // Step 1: Normalize the sample path
    let normalized_sample_path = normalize_path(sample_path);
    let mut path = PathBuf::from(&normalized_sample_path);
    
    // Step 2: If the path is already absolute, return it
    if path.is_absolute() {
        return path;
    }
    
    // Step 3: Apply default_path if available
    if let Some(default_path) = default_path {
        path = combine_sample_path(default_path, &normalized_sample_path);
    }
    
    // Step 4: If still relative and we have an SFZ file path, make it absolute
    if !path.is_absolute() {
        if let Some(sfz_path) = sfz_file_path {
            let sfz_dir = sfz_path.parent().unwrap_or_else(|| Path::new(""));
            path = sfz_dir.join(path);
        }
    }
    
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_path() {
        if cfg!(windows) {
            assert_eq!(normalize_path("samples\\piano\\C4.wav"), "samples\\piano\\C4.wav");
        } else {
            assert_eq!(normalize_path("samples\\piano\\C4.wav"), "samples/piano/C4.wav");
        }
        
        // Forward slashes should remain unchanged on all platforms
        assert_eq!(normalize_path("samples/piano/C4.wav"), "samples/piano/C4.wav");
    }
    
    #[test]
    fn test_combine_sample_path() {
        // Test with forward slashes
        assert_eq!(
            combine_sample_path("samples/piano/", "C4.wav"),
            PathBuf::from("samples/piano/C4.wav")
        );
        
        // Test with backslashes (which should be normalized)
        if cfg!(windows) {
            assert_eq!(
                combine_sample_path("samples\\piano\\", "C4.wav"),
                PathBuf::from("samples\\piano\\C4.wav")
            );
        } else {
            assert_eq!(
                combine_sample_path("samples\\piano\\", "C4.wav"),
                PathBuf::from("samples/piano/C4.wav")
            );
        }
        
        // Test without trailing slash
        assert_eq!(
            combine_sample_path("samples/piano", "C4.wav"),
            PathBuf::from("samples/piano/C4.wav")
        );
        
        // Test with absolute path
        if cfg!(windows) {
            assert_eq!(
                combine_sample_path("samples/piano/", "C:\\samples\\C4.wav"),
                PathBuf::from("C:\\samples\\C4.wav")
            );
        } else {
            assert_eq!(
                combine_sample_path("samples/piano/", "/samples/C4.wav"),
                PathBuf::from("/samples/C4.wav")
            );
        }
    }
    
    #[test]
    fn test_resolve_absolute_path() {
        // Test with absolute sample path
        if cfg!(windows) {
            let sample_path = "C:\\samples\\piano.wav";
            let resolved = resolve_absolute_path(sample_path, None, None);
            assert_eq!(resolved, PathBuf::from("C:\\samples\\piano.wav"));
            
            // Default path should be ignored for absolute paths
            let resolved = resolve_absolute_path(
                sample_path,
                Some("ignored/path/"),
                Some(Path::new("D:\\music\\instruments\\piano.sfz"))
            );
            assert_eq!(resolved, PathBuf::from("C:\\samples\\piano.wav"));
        } else {
            let sample_path = "/samples/piano.wav";
            let resolved = resolve_absolute_path(sample_path, None, None);
            assert_eq!(resolved, PathBuf::from("/samples/piano.wav"));
            
            // Default path should be ignored for absolute paths
            let resolved = resolve_absolute_path(
                sample_path,
                Some("ignored/path/"),
                Some(Path::new("/music/instruments/piano.sfz"))
            );
            assert_eq!(resolved, PathBuf::from("/samples/piano.wav"));
        }
        
        // Test with default path
        let sample_path = "piano.wav";
        let default_path = Some("samples/");
        let resolved = resolve_absolute_path(sample_path, default_path, None);
        assert_eq!(resolved, PathBuf::from("samples/piano.wav"));
        
        // Test with SFZ file path
        let sample_path = "piano.wav";
        if cfg!(windows) {
            let sfz_path = Some(Path::new("C:\\music\\instruments\\piano.sfz"));
            let resolved = resolve_absolute_path(sample_path, None, sfz_path);
            assert_eq!(resolved, PathBuf::from("C:\\music\\instruments\\piano.wav"));
        } else {
            let sfz_path = Some(Path::new("/music/instruments/piano.sfz"));
            let resolved = resolve_absolute_path(sample_path, None, sfz_path);
            assert_eq!(resolved, PathBuf::from("/music/instruments/piano.wav"));
        }
        
        // Test with default path and SFZ file path
        let sample_path = "piano.wav";
        let default_path = Some("samples/");
        if cfg!(windows) {
            let sfz_path = Some(Path::new("C:\\music\\instruments\\piano.sfz"));
            let resolved = resolve_absolute_path(sample_path, default_path, sfz_path);
            assert_eq!(resolved, PathBuf::from("C:\\music\\instruments\\samples\\piano.wav"));
        } else {
            let sfz_path = Some(Path::new("/music/instruments/piano.sfz"));
            let resolved = resolve_absolute_path(sample_path, default_path, sfz_path);
            assert_eq!(resolved, PathBuf::from("/music/instruments/samples/piano.wav"));
        }
        
        // Test with mixed slashes
        let sample_path = "piano\\with\\backslashes.wav";
        let default_path = Some("samples/with/forward/slashes/");
        if cfg!(windows) {
            let sfz_path = Some(Path::new("C:\\music\\instruments\\piano.sfz"));
            let resolved = resolve_absolute_path(sample_path, default_path, sfz_path);
            // On Windows, backslashes are preserved
            assert_eq!(resolved, PathBuf::from("C:\\music\\instruments\\samples\\with\\forward\\slashes\\piano\\with\\backslashes.wav"));
        } else {
            let sfz_path = Some(Path::new("/music/instruments/piano.sfz"));
            let resolved = resolve_absolute_path(sample_path, default_path, sfz_path);
            // On Unix, all backslashes are converted to forward slashes
            assert_eq!(resolved, PathBuf::from("/music/instruments/samples/with/forward/slashes/piano/with/backslashes.wav"));
        }
    }
} 