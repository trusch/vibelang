use std::collections::HashMap;

use crate::parser::error::Error;
use crate::parser::types::{SfzFile, SfzSection, SfzSectionType};

/// Result type alias for parser functions
type Result<T> = std::result::Result<T, Error>;

/// Parse an SFZ file from a string
///
/// This function parses the raw text content of an SFZ file and constructs
/// a structured `SfzFile` object representing all sections and opcodes.
///
/// # SFZ File Format
///
/// SFZ files consist of:
///
/// 1. **Section headers**: Enclosed in angle brackets, like `<region>` or `<global>`
/// 2. **Opcodes**: Parameter=value pairs, like `sample=piano_C4.wav` or `key=60`
/// 3. **Comments**: Lines starting with `//` or inline comments after `//`
///
/// # SFZ Inheritance Model
///
/// SFZ uses a hierarchical inheritance model where opcodes cascade down from
/// higher-level sections to lower-level sections:
///
/// ```text
/// <global>       // Global parameters apply to all regions
/// volume=0
///
/// <master>       // Master parameters override global parameters
/// volume=-6      // This overrides the global volume
///
/// <group>        // Group parameters override master parameters
/// lovel=64
/// hivel=127
///
/// <region>       // Region parameters override group parameters
/// sample=C4.wav  // Sample-specific parameters
/// key=60
/// ```
///
/// # SFZ Parsing Process
///
/// The parsing process involves:
///
/// 1. Reading the file line by line
/// 2. Identifying section headers and creating appropriate section objects
/// 3. Collecting opcode=value pairs within each section
/// 4. Implementing inheritance by copying opcodes from parent sections
/// 5. Building the complete SFZ structure
///
/// # Returns
///
/// * `Result<SfzFile>` - The parsed SFZ file or an error
pub fn parse_sfz(content: &str) -> Result<SfzFile> {
    let mut sfz = SfzFile::new();
    
    let mut current_section: Option<SfzSection> = None;
    let mut current_group: Option<SfzSection> = None;
    let mut current_master: Option<SfzSection> = None;
    
    // Process each line in the content
    for line in content.lines() {
        let line = line.trim();
        
        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        
        // Handle section headers
        if line.starts_with('<') && line.contains('>') {
            if let Some(section) = current_section.take() {
                // Add the completed section to the SFZ file
                match section.section_type {
                    SfzSectionType::Global => sfz.global = Some(section),
                    SfzSectionType::Control => sfz.control = Some(section),
                    SfzSectionType::Master => {
                        current_master = Some(section.clone());
                        sfz.masters.push(section);
                    },
                    SfzSectionType::Group => {
                        current_group = Some(section.clone());
                        sfz.groups.push(section);
                    },
                    SfzSectionType::Region => sfz.regions.push(section),
                    SfzSectionType::Curve => sfz.curves.push(section),
                    SfzSectionType::Effect => sfz.effects.push(section),
                }
            }
            
            // Parse the section header
            let section_type = parse_section_header(line)?;
            let mut opcodes = HashMap::new();
            
            // Inherit opcodes from parent sections
            match section_type {
                SfzSectionType::Region => {
                    // Region inherits from current group and master
                    if let Some(group) = &current_group {
                        for (k, v) in &group.opcodes {
                            opcodes.insert(k.clone(), v.clone());
                        }
                    }
                    if let Some(master) = &current_master {
                        for (k, v) in &master.opcodes {
                            opcodes.insert(k.clone(), v.clone());
                        }
                    }
                    if let Some(global) = &sfz.global {
                        for (k, v) in &global.opcodes {
                            opcodes.insert(k.clone(), v.clone());
                        }
                    }
                },
                SfzSectionType::Group => {
                    // Group inherits from current master
                    if let Some(master) = &current_master {
                        for (k, v) in &master.opcodes {
                            opcodes.insert(k.clone(), v.clone());
                        }
                    }
                    if let Some(global) = &sfz.global {
                        for (k, v) in &global.opcodes {
                            opcodes.insert(k.clone(), v.clone());
                        }
                    }
                },
                SfzSectionType::Master => {
                    // Master inherits from global
                    if let Some(global) = &sfz.global {
                        for (k, v) in &global.opcodes {
                            opcodes.insert(k.clone(), v.clone());
                        }
                    }
                },
                _ => {}
            }
            
            current_section = Some(SfzSection {
                section_type,
                opcodes,
            });
        } else if line.contains('=') {
            // Handle opcode definitions
            if let Some(ref mut section) = current_section {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let opcode = parts[0].trim();
                    
                    // Remove inline comments from the value
                    let mut value = parts[1].trim();
                    if let Some(comment_pos) = value.find("//") {
                        value = value[..comment_pos].trim();
                    }
                    
                    section.opcodes.insert(opcode.to_string(), value.to_string());
                }
            }
        }
    }
    
    // Add the final section if there is one
    if let Some(section) = current_section {
        match section.section_type {
            SfzSectionType::Global => sfz.global = Some(section),
            SfzSectionType::Control => sfz.control = Some(section),
            SfzSectionType::Master => sfz.masters.push(section),
            SfzSectionType::Group => sfz.groups.push(section),
            SfzSectionType::Region => sfz.regions.push(section),
            SfzSectionType::Curve => sfz.curves.push(section),
            SfzSectionType::Effect => sfz.effects.push(section),
        }
    }
    
    Ok(sfz)
}

/// Parse a section header from a line
///
/// This function extracts the section type from a section header line in an SFZ file.
/// Section headers in SFZ are enclosed in angle brackets, like `<region>` or `<global>`.
///
/// # SFZ Section Headers
///
/// SFZ section headers mark the beginning of a new section. The section continues
/// until the next section header or the end of the file. All opcodes between the
/// section header and the next section header belong to that section.
///
/// Valid section headers include:
/// - `<global>` - Global settings for all regions
/// - `<control>` - Control settings for the instrument
/// - `<master>` - Master settings for a group of regions
/// - `<group>` - Group settings for related regions
/// - `<region>` - Individual sample region
/// - `<curve>` - Response curve definition
/// - `<effect>` - Effect settings
///
/// # Arguments
///
/// * `line` - The line containing the section header
///
/// # Returns
///
/// * `Result<SfzSectionType>` - The parsed section type or an error
///
/// # Errors
///
/// Will return an error if:
/// - The section header is malformed (missing angle brackets)
/// - The section type is not recognized
fn parse_section_header(line: &str) -> Result<SfzSectionType> {
    // Extract the section name from <section_name>
    let start = line.find('<').unwrap_or(0) + 1;
    let end = line.find('>').unwrap_or(line.len());
    
    if start >= end || start == 0 || end == line.len() + 1 {
        return Err(Error::Parse(format!("Invalid section header: {}", line)));
    }
    
    let section_name = &line[start..end].trim().to_lowercase();
    
    match SfzSectionType::from_header(section_name) {
        Some(section_type) => Ok(section_type),
        None => Err(Error::Parse(format!("Unknown section type: {}", section_name))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_sfz() {
        let content = r#"
        <control>
        default_path=samples/piano/
        
        <global>
        volume=0
        
        <region>
        sample=piano_C3.wav
        key=60
        "#;
        
        let sfz = parse_sfz(content).expect("Failed to parse SFZ");
        
        assert!(sfz.control.is_some());
        assert!(sfz.global.is_some());
        assert_eq!(sfz.regions.len(), 1);
        
        let control = sfz.control.unwrap();
        assert_eq!(control.get_opcode_str("default_path"), Some("samples/piano/"));
        
        let global = sfz.global.unwrap();
        assert_eq!(global.get_opcode_str("volume"), Some("0"));
        
        let region = &sfz.regions[0];
        assert_eq!(region.get_opcode_str("sample"), Some("piano_C3.wav"));
        assert_eq!(region.get_opcode_str("key"), Some("60"));
    }
} 