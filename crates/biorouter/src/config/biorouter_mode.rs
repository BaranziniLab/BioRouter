use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BioRouterMode {
    Auto,
    Approve,
    SmartApprove,
    Chat,
}

impl FromStr for BioRouterMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(BioRouterMode::Auto),
            "approve" => Ok(BioRouterMode::Approve),
            "smart_approve" => Ok(BioRouterMode::SmartApprove),
            "chat" => Ok(BioRouterMode::Chat),
            _ => Err(format!("invalid mode: {}", s)),
        }
    }
}
