use fractal_society::pkgs::reviewer_grants::{is_valid, issue, revoke};

#[test]
fn valid_before_expiry_and_not_revoked() {
    let grant = issue("proof-1", "reviewer-1", 100, 50);

    assert!(is_valid(&grant, 149));
    assert_eq!(grant.proof_id, "proof-1");
    assert_eq!(grant.reviewer, "reviewer-1");
    assert_eq!(grant.granted_at, 100);
    assert!(!grant.revoked);
}

#[test]
fn revoke_invalidates_grant() {
    let mut grant = issue("proof-1", "reviewer-1", 100, 50);

    revoke(&mut grant);

    assert!(grant.revoked);
    assert!(!is_valid(&grant, 101));
}

#[test]
fn expired_at_or_after_expires_at_is_invalid() {
    let grant = issue("proof-1", "reviewer-1", 100, 50);

    assert!(!is_valid(&grant, 150));
    assert!(!is_valid(&grant, 151));
}

#[test]
fn expires_at_is_granted_at_plus_ttl() {
    let grant = issue("proof-1", "reviewer-1", 100, 50);

    assert_eq!(grant.expires_at, 150);
}
