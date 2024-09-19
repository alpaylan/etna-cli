use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Workload {
    pub language: String,
    pub name: String,
}
