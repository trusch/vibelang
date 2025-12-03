use std::collections::HashMap;
use std::path::PathBuf;

use crate::parser::path_utils::{combine_sample_path, resolve_absolute_path};

/// Represents a complete SFZ file with all sections and regions
///
/// In the SFZ format, a file is organized into a hierarchy of sections, each containing
/// parameters (opcodes) that define how samples should be played. This structure mirrors
/// that organization, providing access to all sections in the SFZ file.
///
/// # SFZ Hierarchy
///
/// The SFZ format uses a hierarchical structure to efficiently organize complex instruments:
///
/// - `<control>`: Contains global settings like default sample paths
/// - `<global>`: Contains settings that apply to all regions
/// - `<master>`: Contains settings that apply to groups of regions
/// - `<group>`: Contains settings that apply to related regions
/// - `<region>`: The basic playable unit, defining a single sample
/// - `<curve>`: Defines response curves for velocity, controllers, etc.
/// - `<effect>`: Defines audio effects settings
///
/// # Inheritance
///
/// Parameters cascade down through this hierarchy:
/// 1. Global settings apply to all regions
/// 2. Master settings override global settings
/// 3. Group settings override master settings
/// 4. Region settings override group settings
///
/// This inheritance model allows for efficiently defining complex instruments with many samples.
#[derive(Debug, Clone, PartialEq)]
pub struct SfzFile {
    /// Global section with settings applied to all regions
    ///
    /// The `<global>` section in SFZ contains parameters that apply to every region
    /// in the instrument. This provides a convenient way to set common parameters
    /// like amplitude envelopes, filters, or effects that should apply to all samples.
    ///
    /// Example:
    /// ```text
    /// <global>
    /// volume=0
    /// ampeg_release=0.5
    /// ```
    pub global: Option<SfzSection>,
    
    /// Control section with settings applied to all regions
    ///
    /// The `<control>` section contains special settings that affect the entire instrument.
    /// It commonly includes the `default_path` for samples and other global behavior controls.
    ///
    /// Example:
    /// ```text
    /// <control>
    /// default_path=samples/piano/
    /// ```
    pub control: Option<SfzSection>,
    
    /// Master sections with settings applied to groups of regions
    ///
    /// `<master>` sections act as an intermediate level between global and group.
    /// These are often used to define common parameters for a set of related groups,
    /// such as different articulations of the same instrument.
    ///
    /// Example:
    /// ```text
    /// <master>
    /// volume=-6
    /// ```
    pub masters: Vec<SfzSection>,
    
    /// Group sections with settings applied to collections of regions
    ///
    /// `<group>` sections define settings for a related collection of regions,
    /// such as velocity layers, round-robins, or different samples for the same note.
    ///
    /// Example:
    /// ```text
    /// <group>
    /// lovel=64
    /// hivel=127
    /// ```
    pub groups: Vec<SfzSection>,
    
    /// Region sections defining each individual sample
    ///
    /// `<region>` sections are the fundamental building blocks of an SFZ instrument.
    /// Each region typically defines a single sample and how it should be played.
    ///
    /// Example:
    /// ```text
    /// <region>
    /// sample=C4.wav
    /// key=60
    /// ```
    pub regions: Vec<SfzSection>,
    
    /// Curve sections for defining response curves
    ///
    /// `<curve>` sections define custom response curves for velocity, controllers,
    /// or other parameters. These allow for non-linear mapping of input values.
    ///
    /// Example:
    /// ```text
    /// <curve>
    /// curve_index=1
    /// v000=0
    /// v127=1
    /// ```
    pub curves: Vec<SfzSection>,
    
    /// Effect sections for defining audio processing
    ///
    /// `<effect>` sections define audio processing effects like reverb, delay, etc.
    ///
    /// Example:
    /// ```text
    /// <effect>
    /// bus=main
    /// reverb_level=30
    /// ```
    pub effects: Vec<SfzSection>,
    
    /// Source file path if loaded from disk
    ///
    /// This is used to resolve relative paths to samples.
    pub source_file: Option<PathBuf>,
}

impl SfzFile {
    /// Creates a new empty SFZ file structure
    ///
    /// This initializes an empty SFZ file with no sections.
    pub fn new() -> Self {
        Self {
            global: None,
            control: None,
            masters: Vec::new(),
            groups: Vec::new(),
            regions: Vec::new(),
            curves: Vec::new(),
            effects: Vec::new(),
            source_file: None,
        }
    }

    /// Add a section to the appropriate collection based on its type
    ///
    /// This method examines the section type and adds it to the correct
    /// location in the SFZ file structure.
    ///
    /// # SFZ Section Processing
    ///
    /// When parsing an SFZ file, sections are processed in order and
    /// added to the appropriate collection based on their type.
    ///
    /// # Arguments
    ///
    /// * `section` - The section to add
    pub fn add_section(&mut self, section: SfzSection) {
        match section.section_type {
            SfzSectionType::Global => self.global = Some(section),
            SfzSectionType::Control => self.control = Some(section),
            SfzSectionType::Master => self.masters.push(section),
            SfzSectionType::Group => self.groups.push(section),
            SfzSectionType::Region => self.regions.push(section),
            SfzSectionType::Curve => self.curves.push(section),
            SfzSectionType::Effect => self.effects.push(section),
        }
    }

    /// Returns true if this SFZ file contains at least one region
    ///
    /// A valid SFZ instrument typically requires at least one region to produce sound.
    pub fn has_regions(&self) -> bool {
        !self.regions.is_empty()
    }
    
    /// Get the default path from the control section if available
    ///
    /// In SFZ, the `default_path` opcode specifies a directory where samples should be looked for.
    /// It's typically defined in the `<control>` section and applies to all regions.
    ///
    /// # Returns
    ///
    /// * `Option<String>` - The default path if defined, or None
    ///
    /// # Example in SFZ
    ///
    /// ```text
    /// <control>
    /// default_path=samples/piano/
    /// ```
    pub fn get_default_path(&self) -> Option<String> {
        self.control.as_ref().and_then(|ctrl| ctrl.get_opcode_str("default_path").map(String::from))
    }
    
    /// Resolve a sample path for the specified section
    /// 
    /// This is a basic resolution method that combines the default_path with the sample path.
    /// For a more comprehensive resolution that takes into account the SFZ file location
    /// and guarantees absolute paths, use `resolve_absolute_sample_path()` instead.
    /// 
    /// # SFZ Sample Path Resolution
    ///
    /// In SFZ, sample paths can be:
    /// 1. Absolute paths (like `/samples/piano.wav` or `C:\samples\piano.wav`)
    /// 2. Relative to the default_path (like `piano.wav` with default_path=`samples/piano/`)
    /// 3. Relative to the SFZ file location if no default_path is specified
    ///
    /// This function handles the first two cases, combining default_path with relative
    /// sample paths while leaving absolute paths unchanged.
    /// 
    /// # Arguments
    /// 
    /// * `section` - The section containing the sample opcode
    /// 
    /// # Returns
    /// 
    /// * `Option<PathBuf>` - The resolved path or None if no sample is defined
    ///
    /// # Example
    ///
    /// ```no_run
    /// use sfz_parser::{parse_sfz_str, SfzSectionType, SfzSection};
    ///
    /// let sfz_content = r#"
    /// <control>
    /// default_path=samples/piano/
    ///
    /// <region>
    /// sample=C4.wav
    /// key=60
    /// "#;
    ///
    /// let sfz = parse_sfz_str(sfz_content).unwrap();
    /// let region = &sfz.regions[0];
    /// let sample_path = sfz.resolve_sample_path(region).unwrap();
    /// // Will resolve to "samples/piano/C4.wav"
    /// ```
    pub fn resolve_sample_path(&self, section: &SfzSection) -> Option<PathBuf> {
        let default_path = self.get_default_path();
        section.resolve_sample_path(default_path.as_deref())
    }
    
    /// Resolve a sample path to an absolute path
    /// 
    /// This comprehensive resolution method takes into account:
    /// 1. The sample path from the section
    /// 2. The default_path from the control section (if available)
    /// 3. The location of the SFZ file (if available)
    ///
    /// It guarantees returning an absolute path when the SFZ file location is known,
    /// making it the preferred method for use in sample players and other applications
    /// that need to load the actual sample files.
    ///
    /// # SFZ Path Resolution Rules
    ///
    /// This method follows these rules in order:
    /// 1. If the sample path is absolute, use it directly
    /// 2. If the sample path is relative and default_path is available, combine them
    /// 3. If the resulting path is still relative and the SFZ file location is known,
    ///    resolve relative to the SFZ file's directory
    ///
    /// # Arguments
    ///
    /// * `section` - The section containing the sample opcode
    ///
    /// # Returns
    ///
    /// * `Option<PathBuf>` - The resolved absolute path, or None if no sample is defined
    ///
    /// # Example
    ///
    /// ```no_run
    /// use sfz_parser::{parse_sfz_file, SfzSectionType, SfzSection};
    /// use std::path::Path;
    ///
    /// // Parse an SFZ file (which sets the source_file)
    /// let sfz = parse_sfz_file("instruments/piano.sfz").unwrap();
    /// let region = &sfz.regions[0];
    /// 
    /// // Get a guaranteed absolute path to the sample
    /// let sample_path = sfz.resolve_absolute_sample_path(region).unwrap();
    /// assert!(sample_path.is_absolute());
    /// ```
    pub fn resolve_absolute_sample_path(&self, section: &SfzSection) -> Option<PathBuf> {
        // Get the sample path from the section
        let sample_path = section.get_opcode_str("sample")?;
        
        // Get the default_path from the control section
        let default_path = self.get_default_path();
        
        // Resolve to an absolute path
        Some(resolve_absolute_path(
            sample_path,
            default_path.as_deref(),
            self.source_file.as_deref()
        ))
    }
}

impl Default for SfzFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Types of SFZ sections
///
/// The SFZ format defines different section types, each with a specific role in
/// defining how an instrument should behave. These section types form a hierarchy
/// in which parameters cascade down from global to specific regions.
///
/// # SFZ Section Structure
///
/// Sections are delimited by angle brackets, like `<region>`, and contain
/// parameter=value pairs called opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfzSectionType {
    /// Global settings that apply to all regions
    ///
    /// The `<global>` section contains parameters that apply to all regions,
    /// providing a way to set common parameters for the entire instrument.
    Global,
    
    /// Control settings for the entire instrument
    ///
    /// The `<control>` section contains special settings like default_path
    /// that affect the entire instrument's behavior.
    Control,
    
    /// Master settings that apply to groups
    ///
    /// The `<master>` section is an intermediate level that can override
    /// global settings and be overridden by group settings.
    Master,
    
    /// Group settings for collections of regions
    ///
    /// The `<group>` section contains settings for a related collection
    /// of regions, such as velocity layers or round-robins.
    Group,
    
    /// Region defining a single sample
    ///
    /// The `<region>` section is the fundamental building block, defining
    /// a single sample and how it should be played.
    Region,
    
    /// Curve defining a response curve
    ///
    /// The `<curve>` section defines a custom response curve for velocity,
    /// controllers, or other parameters.
    Curve,
    
    /// Effect settings
    ///
    /// The `<effect>` section defines audio processing effects.
    Effect,
}

impl SfzSectionType {
    /// Returns the section type based on its header name
    ///
    /// This converts an SFZ section header name (without the angle brackets)
    /// to the corresponding SfzSectionType.
    ///
    /// # Arguments
    ///
    /// * `header` - The section header name (e.g., "region", "global")
    ///
    /// # Returns
    ///
    /// * `Option<Self>` - The corresponding section type or None if not recognized
    pub fn from_header(header: &str) -> Option<Self> {
        match header.to_lowercase().as_str() {
            "global" => Some(Self::Global),
            "control" => Some(Self::Control),
            "master" => Some(Self::Master),
            "group" => Some(Self::Group),
            "region" => Some(Self::Region),
            "curve" => Some(Self::Curve),
            "effect" => Some(Self::Effect),
            _ => None,
        }
    }

    /// Returns the section header string
    ///
    /// This returns the full section header including angle brackets,
    /// as it would appear in an SFZ file.
    ///
    /// # Returns
    ///
    /// * `&'static str` - The section header string (e.g., "&lt;region&gt;")
    pub fn header_str(&self) -> &'static str {
        match self {
            Self::Global => "<global>",
            Self::Control => "<control>",
            Self::Master => "<master>",
            Self::Group => "<group>",
            Self::Region => "<region>",
            Self::Curve => "<curve>",
            Self::Effect => "<effect>",
        }
    }
}

/// Represents a section in an SFZ file containing opcodes and their values
///
/// An SFZ section represents a collection of parameters (opcodes) that define
/// how samples should be played. Each section has a type (like global, region, etc.)
/// and a set of opcode=value pairs.
///
/// # SFZ Section Format
///
/// In an SFZ file, a section looks like:
/// ```text
/// <section_type>
/// opcode1=value1
/// opcode2=value2
/// ```
///
/// Sections can define various aspects of sample playback, from basic parameters
/// like which sample to play and on which key, to complex settings for envelopes,
/// filters, effects, and more.
#[derive(Debug, Clone, PartialEq)]
pub struct SfzSection {
    /// The type of section (global, region, etc.)
    pub section_type: SfzSectionType,
    
    /// The opcodes and their values in this section
    ///
    /// In SFZ, opcodes are parameter=value pairs that define various aspects
    /// of how samples should be played. Each opcode has a name (key) and a value.
    pub opcodes: HashMap<String, String>,
}

impl SfzSection {
    /// Creates a new section with the specified type
    ///
    /// This initializes an empty section with no opcodes.
    ///
    /// # Arguments
    ///
    /// * `section_type` - The type of section to create
    pub fn new(section_type: SfzSectionType) -> Self {
        Self {
            section_type,
            opcodes: HashMap::new(),
        }
    }

    /// Adds an opcode to this section
    ///
    /// In SFZ, opcodes are parameter=value pairs that define various aspects
    /// of how samples should be played. This method adds an opcode to the section.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the opcode
    /// * `value` - The value of the opcode
    pub fn add_opcode(&mut self, name: String, value: String) {
        self.opcodes.insert(name, value);
    }

    /// Gets an opcode value as a string if it exists
    ///
    /// This method retrieves the value of an opcode as a String reference.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the opcode to retrieve
    ///
    /// # Returns
    ///
    /// * `Option<&String>` - The opcode value or None if not found
    pub fn get_opcode(&self, name: &str) -> Option<&String> {
        self.opcodes.get(name)
    }
    
    /// Gets an opcode value as a string slice if it exists
    ///
    /// This method retrieves the value of an opcode as a string slice.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the opcode to retrieve
    ///
    /// # Returns
    ///
    /// * `Option<&str>` - The opcode value or None if not found
    pub fn get_opcode_str(&self, name: &str) -> Option<&str> {
        self.opcodes.get(name).map(|s| s.as_str())
    }
    
    /// Resolve a sample path using the provided default path
    /// 
    /// This combines the sample path from this section with the default path
    /// and normalizes the result for the current OS.
    /// 
    /// # SFZ Sample Paths
    ///
    /// In SFZ, the sample opcode specifies which audio file to play. Sample paths can be:
    /// - Absolute paths: `/samples/piano.wav` or `C:\samples\piano.wav`
    /// - Relative to default_path: With `default_path=samples/`, `piano.wav` becomes `samples/piano.wav`
    ///
    /// This method handles path normalization for cross-platform compatibility and
    /// combines relative paths with the default_path.
    /// 
    /// # Arguments
    /// 
    /// * `default_path` - The default path to use for relative sample paths
    /// 
    /// # Returns
    /// 
    /// * `Option<PathBuf>` - The resolved sample path, or None if this section
    ///   doesn't have a sample opcode
    pub fn resolve_sample_path(&self, default_path: Option<&str>) -> Option<PathBuf> {
        self.get_opcode_str("sample").map(|sample_path| {
            if let Some(def_path) = default_path {
                combine_sample_path(def_path, sample_path)
            } else {
                // No default path, just normalize the sample path
                PathBuf::from(crate::parser::path_utils::normalize_path(sample_path))
            }
        })
    }
} 