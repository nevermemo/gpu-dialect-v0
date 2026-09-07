//! SPIR-V artifact access and format-level validation.
//!
//! Executable modules are produced by `slangc` from descriptor Slang source.
//! This module only contains format-level helpers for inspecting those bytes.

const MAGIC: u32 = 0x0723_0203;

/// Performs format-level checks without requiring SPIRV-Tools or a GPU driver.
/// Semantic validation remains the responsibility of `spirv-val` and the
/// target API.
pub fn validate_structure(words: &[u32]) -> Result<(), &'static str> {
    if words.len() < 5 {
        return Err("SPIR-V module is shorter than its header");
    }
    if words[0] != MAGIC {
        return Err("invalid SPIR-V magic number");
    }
    if words[3] == 0 {
        return Err("SPIR-V ID bound must be greater than zero");
    }
    if words[4] != 0 {
        return Err("reserved SPIR-V schema word must be zero");
    }

    let mut offset = 5;
    let mut has_memory_model = false;
    let mut has_entry_point = false;
    let mut function_depth = 0u32;
    while offset < words.len() {
        let instruction = words[offset];
        let word_count = (instruction >> 16) as usize;
        let opcode = instruction & 0xffff;
        if word_count == 0 {
            return Err("SPIR-V instruction has zero word count");
        }
        if offset + word_count > words.len() {
            return Err("SPIR-V instruction extends beyond the module");
        }
        match opcode {
            14 => has_memory_model = true,
            15 => has_entry_point = true,
            54 => function_depth = function_depth.saturating_add(1),
            56 => {
                function_depth = function_depth
                    .checked_sub(1)
                    .ok_or("OpFunctionEnd appears outside a function")?;
            }
            _ => {}
        }
        offset += word_count;
    }
    if !has_memory_model {
        return Err("SPIR-V module has no memory model");
    }
    if !has_entry_point {
        return Err("SPIR-V module has no entry point");
    }
    if function_depth != 0 {
        return Err("SPIR-V function is not terminated");
    }
    Ok(())
}

pub fn words_as_le_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_truncated_instruction() {
        let words = [MAGIC, 0x0001_0000, 0, 2, 0, (3 << 16) | 14, 0];
        assert_eq!(
            validate_structure(&words),
            Err("SPIR-V instruction extends beyond the module")
        );
    }
}
