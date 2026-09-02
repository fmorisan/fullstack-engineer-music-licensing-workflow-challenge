//! Scene and playback time interval math.
//!
//! All times are whole seconds; `end` is exclusive, matching the
//! `int4range(start, end)` semantics of the database exclusion constraint
//! that authoritatively prevents scene overlaps (AGENTS.md: concurrency-
//! sensitive invariants live at the database level). The helpers here give
//! services a pure pre-check and tests a property base.

/// A `[start, end)` interval of time in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimeInterval {
    /// First second of the interval.
    pub start: u32,
    /// One past the last second; exclusive.
    pub end: u32,
}

impl TimeInterval {
    /// Construct an interval; see [`TimeInterval::is_valid`] for well-formedness.
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Whether the interval is well-formed (`end > start`).
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.end > self.start
    }

    /// Length in seconds; meaningful only when [`Self::is_valid`].
    #[must_use]
    pub const fn duration_seconds(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether the two intervals share at least one second.
    ///
    /// Adjacent intervals (`a.end == b.start`) do not overlap. Behavior for
    /// invalid intervals is unspecified; validate first.
    #[must_use]
    pub const fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Strategy producing well-formed intervals within a bounded range.
    fn valid_interval() -> impl Strategy<Value = TimeInterval> {
        (0u32..500, 0u32..500).prop_map(|(a, b)| TimeInterval::new(a.min(b), a.max(b) + 1))
    }

    #[test]
    fn zero_length_intervals_are_invalid() {
        assert!(!TimeInterval::new(10, 10).is_valid());
        assert!(!TimeInterval::new(10, 5).is_valid());
        assert!(TimeInterval::new(5, 10).is_valid());
        assert_eq!(TimeInterval::new(5, 10).duration_seconds(), 5);
    }

    #[test]
    fn adjacency_is_not_overlap() {
        let a = TimeInterval::new(0, 10);
        let b = TimeInterval::new(10, 20);
        assert!(!a.overlaps(&b));
        assert!(!b.overlaps(&a));
        assert!(a.overlaps(&TimeInterval::new(9, 11)));
        assert!(a.overlaps(&TimeInterval::new(0, 1)));
        assert!(TimeInterval::new(0, u32::MAX - 1).overlaps(&a));
    }

    proptest! {
        /// Symmetry: overlap is a symmetric relation.
        #[test]
        fn overlap_is_symmetric(a in valid_interval(), b in valid_interval()) {
            prop_assert_eq!(a.overlaps(&b), b.overlaps(&a));
        }

        /// Equivalence with a brute-force point scan: two intervals overlap
        /// iff they share at least one integer second.
        #[test]
        fn overlap_matches_brute_force(
            s1 in 0u32..500,
            e1 in 0u32..500,
            s2 in 0u32..500,
            e2 in 0u32..500,
        ) {
            let a = TimeInterval::new(s1.min(e1), s1.max(e1) + 1);
            let b = TimeInterval::new(s2.min(e2), s2.max(e2) + 1);
            let brute = (a.start..a.end).any(|p| (b.start..b.end).contains(&p));
            prop_assert_eq!(a.overlaps(&b), brute);
        }

        /// A non-overlapping pair can always be ordered with a hard gap:
        /// exactly one of them ends at or before the other's start.
        #[test]
        fn disjoint_intervals_have_a_clean_ordering(
            a in valid_interval(),
            b in valid_interval(),
        ) {
            if !a.overlaps(&b) {
                prop_assert!(a.end <= b.start || b.end <= a.start);
            }
        }
    }
}
