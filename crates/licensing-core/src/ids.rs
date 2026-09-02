//! UUIDv7 generation (ADR-009).
//!
//! Time-ordered identifiers generated in application code so any replica can
//! mint ids without coordination, and Postgres B-tree indexes stay write-
//! friendly.

use uuid::Uuid;

/// Generate a fresh UUIDv7.
#[must_use]
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

/// Extract the Unix epoch millisecond prefix of a UUIDv7.
///
/// Returns `None` for non-v7 ids. Useful for diagnostics and tests; not for
/// correctness paths.
#[must_use]
pub fn unix_ms(id: Uuid) -> Option<u64> {
    if id.get_version_num() != 7 {
        return None;
    }
    let b = id.as_bytes();
    let ms = u64::from(b[0]) << 40
        | u64::from(b[1]) << 32
        | u64::from(b[2]) << 24
        | u64::from(b[3]) << 16
        | u64::from(b[4]) << 8
        | u64::from(b[5]);
    Some(ms)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn ids_are_version_seven() {
        for _ in 0..1000 {
            let id = new_id();
            assert_eq!(id.get_version_num(), 7);
            assert!(unix_ms(id).is_some());
        }
    }

    #[test]
    fn ids_are_unique() {
        let set: HashSet<Uuid> = (0..10_000).map(|_| new_id()).collect();
        assert_eq!(set.len(), 10_000);
    }

    #[test]
    fn millisecond_prefix_never_moves_backwards() {
        let mut last = 0_u64;
        for i in 0..500 {
            if i == 250 {
                // Cross at least one millisecond boundary.
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let ms = unix_ms(new_id()).expect("v7 id");
            assert!(ms >= last, "timestamp prefix regressed: {ms} < {last}");
            last = ms;
        }
    }

    #[test]
    fn unix_ms_rejects_non_v7() {
        assert_eq!(unix_ms(Uuid::nil()), None);
    }
}
