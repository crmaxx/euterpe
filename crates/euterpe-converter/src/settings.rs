use serde::{Deserialize, Serialize};

use crate::error::{ConvertError, Result};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlacPreset {
    Fast,
    #[default]
    Balanced,
    Best,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlacEncodeSettings {
    #[serde(default)]
    pub preset: FlacPreset,
    #[serde(default)]
    pub block_size: Option<usize>,
    #[serde(default)]
    pub multithread: bool,
}

impl FlacEncodeSettings {
    pub fn validate(&self) -> Result<()> {
        if let Some(bs) = self.block_size
            && (!(256..=65_535).contains(&bs) || !bs.is_power_of_two())
        {
            return Err(ConvertError::InvalidSettings(format!(
                "block_size must be a power of two between 256 and 65535, got {bs}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilePolicy {
    ReplaceInPlace,
    #[default]
    SiblingThenDelete,
}
