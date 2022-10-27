use serde::{Deserialize, Serialize};

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct ResultAppes {
    pub outcome: String,
    pub points: Option<String>,
}
