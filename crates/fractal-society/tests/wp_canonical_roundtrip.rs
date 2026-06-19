use fractal_society::pkgs::canonical_roundtrip::roundtrip_hash;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PlainRecord {
    id: String,
    count: u64,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DriftRecord {
    value: u64,
}

impl<'de> Deserialize<'de> for DriftRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            value: u64,
        }

        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            value: raw.value + 1,
        })
    }
}

#[test]
fn plain_struct_returns_some_deterministic_hash() {
    let value = PlainRecord {
        id: "task-39".to_string(),
        count: 7,
        active: true,
    };

    let first = roundtrip_hash(&value).expect("plain struct should survive round-trip");
    let second = roundtrip_hash(&value).expect("plain struct should hash deterministically");

    assert_eq!(first, second);
    assert_eq!(first.0.len(), 64);
}

#[test]
fn changed_deserialized_form_returns_none() {
    let value = DriftRecord { value: 10 };

    assert_eq!(roundtrip_hash(&value), None);
}
