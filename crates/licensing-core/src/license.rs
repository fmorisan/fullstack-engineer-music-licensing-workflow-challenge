//! The license negotiation state machine.
//!
//! This is the single authoritative implementation of the licensing workflow
//! rules (see `AGENTS.md`); services must not re-implement them. The rules
//! come from `docs/architecture/00-design.md`:
//!
//! - A license is created in [`LicenseState::Offer`] by the movie studio.
//! - The record label may counter-offer (with a new fee) from `OFFER`.
//! - The studio may re-offer (with a new fee) from `COUNTER_OFFER`.
//! - Acceptance: the studio may only accept a `COUNTER_OFFER`; the label may
//!   only accept an `OFFER`.
//! - Either party may reject from any non-terminal state.
//! - `ACCEPTED` and `REJECTED` are terminal.
//! - Administrators are not a licensing party and may perform no action.

use std::fmt;
use std::str::FromStr;

use crate::role::Role;

/// Lifecycle state of a license negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LicenseState {
    /// The movie studio has set an offer.
    Offer,
    /// The record label has countered with a new fee.
    CounterOffer,
    /// The current deal was accepted (terminal).
    Accepted,
    /// The deal was rejected (terminal).
    Rejected,
}

impl LicenseState {
    /// Every state, in declaration order.
    pub const ALL: [LicenseState; 4] = [
        LicenseState::Offer,
        LicenseState::CounterOffer,
        LicenseState::Accepted,
        LicenseState::Rejected,
    ];

    /// State of a newly created license.
    pub const INITIAL: Self = Self::Offer;

    /// Canonical wire name, as stored and serialized.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            LicenseState::Offer => "OFFER",
            LicenseState::CounterOffer => "COUNTER_OFFER",
            LicenseState::Accepted => "ACCEPTED",
            LicenseState::Rejected => "REJECTED",
        }
    }

    /// Whether no further transitions are possible from this state.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, LicenseState::Accepted | LicenseState::Rejected)
    }
}

impl fmt::Display for LicenseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failed to parse a [`LicenseState`] from a string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown license state `{0}` (expected OFFER, COUNTER_OFFER, ACCEPTED, or REJECTED)")]
pub struct ParseLicenseStateError(String);

impl FromStr for LicenseState {
    type Err = ParseLicenseStateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "OFFER" => Ok(LicenseState::Offer),
            "COUNTER_OFFER" => Ok(LicenseState::CounterOffer),
            "ACCEPTED" => Ok(LicenseState::Accepted),
            "REJECTED" => Ok(LicenseState::Rejected),
            other => Err(ParseLicenseStateError(other.to_owned())),
        }
    }
}

/// Actions that drive license state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LicenseAction {
    /// The studio re-offers with a new fee after a counter-offer.
    Offer,
    /// The label counters with a new fee.
    CounterOffer,
    /// Accept the current deal (studio accepts counters, label accepts offers).
    Accept,
    /// Reject the deal (either party, from any non-terminal state).
    Reject,
}

impl LicenseAction {
    /// Every action, in declaration order.
    pub const ALL: [LicenseAction; 4] = [
        LicenseAction::Offer,
        LicenseAction::CounterOffer,
        LicenseAction::Accept,
        LicenseAction::Reject,
    ];

    /// Canonical wire name, as used by the license update API.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            LicenseAction::Offer => "OFFER",
            LicenseAction::CounterOffer => "COUNTER_OFFER",
            LicenseAction::Accept => "ACCEPT",
            LicenseAction::Reject => "REJECT",
        }
    }
}

impl fmt::Display for LicenseAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a [`transition`] attempt failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    /// The role may never perform this action, regardless of state.
    #[error("role {} may not perform action {}", .role.as_str(), .action.as_str())]
    ActionForbiddenForRole {
        /// The attempted action.
        action: LicenseAction,
        /// The acting role.
        role: Role,
    },
    /// The action is not legal from the current state (for this role).
    #[error("action {} is not legal from state {}", .action.as_str(), .from.as_str())]
    IllegalFromState {
        /// The attempted action.
        action: LicenseAction,
        /// The state it was attempted from.
        from: LicenseState,
    },
}

/// Apply `action` performed by `role` to a license in state `from`.
///
/// Returns the next state, or a [`TransitionError`] explaining the rejection.
/// Fee changes are the caller's concern: `Offer` and `CounterOffer` imply a
/// new fee supplied with the request; `Accept` keeps the current fee.
///
/// # Errors
///
/// - [`TransitionError::ActionForbiddenForRole`] for role violations
///   (e.g. a label attempting to offer, or an admin attempting anything).
/// - [`TransitionError::IllegalFromState`] for state violations
///   (e.g. accepting an already-rejected license).
pub fn transition(
    from: LicenseState,
    action: LicenseAction,
    role: Role,
) -> Result<LicenseState, TransitionError> {
    match (action, role) {
        (LicenseAction::Offer, Role::Studio) => match from {
            LicenseState::CounterOffer => Ok(LicenseState::Offer),
            _ => Err(TransitionError::IllegalFromState { action, from }),
        },
        (LicenseAction::CounterOffer, Role::Label) => match from {
            LicenseState::Offer => Ok(LicenseState::CounterOffer),
            _ => Err(TransitionError::IllegalFromState { action, from }),
        },
        (LicenseAction::Accept, Role::Studio) => match from {
            LicenseState::CounterOffer => Ok(LicenseState::Accepted),
            _ => Err(TransitionError::IllegalFromState { action, from }),
        },
        (LicenseAction::Accept, Role::Label) => match from {
            LicenseState::Offer => Ok(LicenseState::Accepted),
            _ => Err(TransitionError::IllegalFromState { action, from }),
        },
        (LicenseAction::Reject, Role::Studio | Role::Label) => match from {
            LicenseState::Offer | LicenseState::CounterOffer => Ok(LicenseState::Rejected),
            _ => Err(TransitionError::IllegalFromState { action, from }),
        },
        (LicenseAction::Offer | LicenseAction::CounterOffer, _)
        | (LicenseAction::Accept | LicenseAction::Reject, Role::Admin) => {
            Err(TransitionError::ActionForbiddenForRole { action, role })
        }
    }
}

/// Predicate form of [`transition`].
#[must_use]
pub fn can_transition(from: LicenseState, action: LicenseAction, role: Role) -> bool {
    transition(from, action, role).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The complete expected outcome table: (state, action, role) -> result.
    ///
    /// Kept as an independent data oracle (not derived from `transition`) so
    /// the exhaustive matrix test below actually validates the implementation
    /// against the rules as written in the design doc. Every combination is
    /// spelled out on purpose; merging identical arms would defeat that.
    #[allow(clippy::match_same_arms, clippy::result_large_err)]
    fn expected(
        state: LicenseState,
        action: LicenseAction,
        role: Role,
    ) -> Result<LicenseState, TransitionError> {
        use LicenseAction as A;
        use LicenseState as S;
        use Role as R;

        let forbidden = || TransitionError::ActionForbiddenForRole { action, role };
        let illegal = || TransitionError::IllegalFromState {
            action,
            from: state,
        };

        match (state, action, role) {
            // OFFER action: studio re-offers, only from COUNTER_OFFER.
            (S::Offer, A::Offer, R::Studio) => Err(illegal()),
            (S::CounterOffer, A::Offer, R::Studio) => Ok(S::Offer),
            (S::Accepted, A::Offer, R::Studio) => Err(illegal()),
            (S::Rejected, A::Offer, R::Studio) => Err(illegal()),
            (_, A::Offer, R::Label | R::Admin) => Err(forbidden()),

            // COUNTER_OFFER action: label only, only from OFFER.
            (S::Offer, A::CounterOffer, R::Label) => Ok(S::CounterOffer),
            (S::CounterOffer, A::CounterOffer, R::Label) => Err(illegal()),
            (S::Accepted, A::CounterOffer, R::Label) => Err(illegal()),
            (S::Rejected, A::CounterOffer, R::Label) => Err(illegal()),
            (_, A::CounterOffer, R::Studio | R::Admin) => Err(forbidden()),

            // ACCEPT action: studio accepts COUNTER_OFFERs, label accepts OFFERs.
            (S::Offer, A::Accept, R::Studio) => Err(illegal()),
            (S::CounterOffer, A::Accept, R::Studio) => Ok(S::Accepted),
            (S::Accepted, A::Accept, R::Studio) => Err(illegal()),
            (S::Rejected, A::Accept, R::Studio) => Err(illegal()),
            (S::Offer, A::Accept, R::Label) => Ok(S::Accepted),
            (S::CounterOffer, A::Accept, R::Label) => Err(illegal()),
            (S::Accepted, A::Accept, R::Label) => Err(illegal()),
            (S::Rejected, A::Accept, R::Label) => Err(illegal()),
            (_, A::Accept, R::Admin) => Err(forbidden()),

            // REJECT action: either party, from non-terminal states.
            (S::Offer, A::Reject, R::Studio | R::Label) => Ok(S::Rejected),
            (S::CounterOffer, A::Reject, R::Studio | R::Label) => Ok(S::Rejected),
            (S::Accepted, A::Reject, R::Studio | R::Label) => Err(illegal()),
            (S::Rejected, A::Reject, R::Studio | R::Label) => Err(illegal()),
            (_, A::Reject, R::Admin) => Err(forbidden()),
        }
    }

    #[test]
    fn exhaustive_matrix_matches_the_rules() {
        for state in LicenseState::ALL {
            for action in LicenseAction::ALL {
                for role in Role::ALL {
                    let got = transition(state, action, role);
                    let want = expected(state, action, role);
                    assert_eq!(
                        got, want,
                        "mismatch for state={state}, action={action}, role={role}"
                    );
                }
            }
        }
    }

    #[test]
    fn initial_state_is_offer_and_non_terminal() {
        assert_eq!(LicenseState::INITIAL, LicenseState::Offer);
        assert!(!LicenseState::INITIAL.is_terminal());
    }

    #[test]
    fn terminal_states_are_terminal() {
        assert!(LicenseState::Accepted.is_terminal());
        assert!(LicenseState::Rejected.is_terminal());
    }

    #[test]
    fn happy_path_negotiation_walks_the_full_lifecycle() {
        use LicenseAction as A;
        let mut state = LicenseState::INITIAL;
        state = transition(state, A::CounterOffer, Role::Label).unwrap();
        assert_eq!(state, LicenseState::CounterOffer);
        state = transition(state, A::Offer, Role::Studio).unwrap();
        assert_eq!(state, LicenseState::Offer);
        state = transition(state, A::CounterOffer, Role::Label).unwrap();
        state = transition(state, A::Accept, Role::Studio).unwrap();
        assert_eq!(state, LicenseState::Accepted);
    }

    #[test]
    fn can_transition_agrees_with_transition() {
        for state in LicenseState::ALL {
            for action in LicenseAction::ALL {
                for role in Role::ALL {
                    assert_eq!(
                        can_transition(state, action, role),
                        transition(state, action, role).is_ok()
                    );
                }
            }
        }
    }

    #[test]
    fn parses_and_serializes_wire_names() {
        for state in LicenseState::ALL {
            assert_eq!(LicenseState::from_str(state.as_str()), Ok(state));
            assert_eq!(
                serde_json::to_string(&state).unwrap(),
                format!("\"{}\"", state.as_str())
            );
        }
        assert!(LicenseState::from_str("COUNTER-OFFER").is_err());
        assert!(serde_json::from_str::<LicenseState>("\"offer\"").is_err());
    }

    proptest! {
        /// Terminal states are absorbing: nothing any role does can move them.
        #[test]
        fn terminal_states_absorb_everything(
            state in proptest::sample::select(LicenseState::ALL.to_vec()),
            action in proptest::sample::select(LicenseAction::ALL.to_vec()),
            role in proptest::sample::select(Role::ALL.to_vec()),
        ) {
            prop_assume!(state.is_terminal());
            prop_assert!(transition(state, action, role).is_err());
        }

        /// Acceptance duality: label accepts offers, studio accepts counters,
        /// and nobody else accepts anything.
        #[test]
        fn accept_succeeds_only_for_the_counterparty(
            state in proptest::sample::select(LicenseState::ALL.to_vec()),
            role in proptest::sample::select(Role::ALL.to_vec()),
        ) {
            let ok = matches!(
                (state, role),
                (LicenseState::Offer, Role::Label) | (LicenseState::CounterOffer, Role::Studio)
            );
            prop_assert_eq!(transition(state, LicenseAction::Accept, role).is_ok(), ok);
        }

        /// Any state reachable from INITIAL by legal transitions is one of
        /// the four known states, and the walk stays within that set.
        #[test]
        fn random_action_sequences_stay_within_the_state_space(
            actions in proptest::collection::vec(
                (
                    proptest::sample::select(LicenseAction::ALL.to_vec()),
                    proptest::sample::select(Role::ALL.to_vec())
                ),
                0..24
            ),
        ) {
            let mut state = LicenseState::INITIAL;
            for (action, role) in actions {
                if let Ok(next) = transition(state, action, role) {
                    prop_assert!(LicenseState::ALL.contains(&next));
                    state = next;
                }
            }
        }
    }
}
