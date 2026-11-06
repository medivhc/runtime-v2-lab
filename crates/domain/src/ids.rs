//! Strongly typed IDs. Wrapped strings so IDs are opaque and printable.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! define_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
            pub fn generate() -> Self {
                // Random suffix; uuid crate not required here.
                let rand = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                Self(format!("{}-{:x}", $prefix, rand))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(RequestId, "req");
define_id!(TaskId, "task");
define_id!(RunId, "run");
define_id!(NodeId, "node");
define_id!(MessageId, "msg");
define_id!(EvidenceId, "evd");
define_id!(ResourceId, "res");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_opaque_and_printable() {
        let id = RequestId::new("req-abc");
        assert_eq!(id.to_string(), "req-abc");
        assert_eq!(id.as_str(), "req-abc");
        let generated = RunId::generate();
        assert!(generated.as_str().starts_with("run-"));
    }

    #[test]
    fn ids_serialize_as_strings() {
        let id = TaskId::new("task-1");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"task-1\"");
        let back: TaskId = serde_json::from_str("\"task-1\"").unwrap();
        assert_eq!(back, id);
    }
}
