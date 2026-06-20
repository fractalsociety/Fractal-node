//! Sybil-detection package.
//!
//! Flags suspicious review patterns from reviewer/subject records.

use std::collections::HashSet;

/// Minimal review relation used for pattern analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReviewRecord {
    /// Reviewer identity.
    pub reviewer: String,
    /// Reviewed subject identity.
    pub subject: String,
}

/// Suspicious review pattern found in a review set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuspiciousPattern {
    /// Reviewer reviewed their own subject.
    SelfReview {
        /// Reviewer identity.
        reviewer: String,
    },
    /// Same reviewer reviewed the same subject more than once.
    DuplicateReview {
        /// Reviewer identity.
        reviewer: String,
        /// Reviewed subject identity.
        subject: String,
    },
    /// Pair of subjects reviewed each other.
    CircularReview {
        /// Cycle identities in traversal order.
        cycle: Vec<String>,
    },
}

/// Analyze review records for self-review, duplicate-review, and two-node circular review patterns.
pub fn analyze(reviews: &[ReviewRecord]) -> Vec<SuspiciousPattern> {
    let mut patterns = Vec::new();
    let mut seen_pairs: HashSet<(&str, &str)> = HashSet::new();
    let mut duplicate_pairs: HashSet<(&str, &str)> = HashSet::new();
    let mut circular_pairs: HashSet<(String, String)> = HashSet::new();

    for review in reviews {
        if review.reviewer == review.subject {
            patterns.push(SuspiciousPattern::SelfReview {
                reviewer: review.reviewer.clone(),
            });
        }

        let pair = (review.reviewer.as_str(), review.subject.as_str());
        if !seen_pairs.insert(pair) && duplicate_pairs.insert(pair) {
            patterns.push(SuspiciousPattern::DuplicateReview {
                reviewer: review.reviewer.clone(),
                subject: review.subject.clone(),
            });
        }

        let reverse = (review.subject.as_str(), review.reviewer.as_str());
        if review.reviewer != review.subject && seen_pairs.contains(&reverse) {
            let cycle_key = ordered_pair(&review.reviewer, &review.subject);
            if circular_pairs.insert(cycle_key) {
                patterns.push(SuspiciousPattern::CircularReview {
                    cycle: vec![review.reviewer.clone(), review.subject.clone()],
                });
            }
        }
    }

    patterns
}

fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}
