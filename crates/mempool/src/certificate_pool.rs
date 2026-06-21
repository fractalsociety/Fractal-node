use std::collections::BTreeMap;

use fractal_core::{
    OwnedObjectCertificate, OwnedObjectCertificateError, OwnedObjectCertificateEvidenceError,
    OwnedObjectConflictingCertificateEvidence, OwnedObjectConflictingCertificateFinding,
    OwnedObjectVersion,
};
use fractal_crypto::{BlsPublicKey, Hash256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateFinalityRecord {
    pub certificate_hash: Hash256,
    pub certificate: OwnedObjectCertificate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertificateConflictRecord {
    pub evidence: OwnedObjectConflictingCertificateEvidence,
    pub finding: OwnedObjectConflictingCertificateFinding,
}

#[derive(Debug, thiserror::Error)]
pub enum CertificatePoolError {
    #[error("owned-object certificate invalid: {0}")]
    InvalidCertificate(#[from] OwnedObjectCertificateError),
    #[error("owned-object certificate conflict evidence invalid: {0}")]
    InvalidConflictEvidence(#[from] OwnedObjectCertificateEvidenceError),
    #[error("certificate conflicts with final certificate for object/version {object_version:?}")]
    ObjectVersionConflict {
        object_version: OwnedObjectVersion,
        conflict: Box<CertificateConflictRecord>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct CertificatePool {
    by_object_version: BTreeMap<OwnedObjectVersion, CertificateFinalityRecord>,
    conflicts: Vec<CertificateConflictRecord>,
}

impl CertificatePool {
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_object_version.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_object_version.is_empty()
    }

    #[must_use]
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }

    #[must_use]
    pub fn conflicts(&self) -> &[CertificateConflictRecord] {
        &self.conflicts
    }

    #[must_use]
    pub fn finality_for_object_version(
        &self,
        object_version: &OwnedObjectVersion,
    ) -> Option<&CertificateFinalityRecord> {
        self.by_object_version.get(object_version)
    }

    #[must_use]
    pub fn accepted_certificates(&self) -> Vec<OwnedObjectCertificate> {
        let mut out = BTreeMap::<Hash256, OwnedObjectCertificate>::new();
        for record in self.by_object_version.values() {
            out.entry(record.certificate_hash)
                .or_insert_with(|| record.certificate.clone());
        }
        out.into_values().collect()
    }

    pub fn insert(
        &mut self,
        certificate: OwnedObjectCertificate,
        validator_pubkeys: &[BlsPublicKey],
        quorum_threshold: usize,
    ) -> Result<Hash256, CertificatePoolError> {
        certificate.verify(validator_pubkeys, quorum_threshold)?;
        let certificate_hash = certificate.certificate_hash()?;
        for object_version in &certificate.object_versions {
            if let Some(existing) = self.by_object_version.get(object_version) {
                if existing.certificate_hash == certificate_hash {
                    return Ok(certificate_hash);
                }
                let evidence = OwnedObjectConflictingCertificateEvidence {
                    certificate_a: existing.certificate.clone(),
                    certificate_b: certificate.clone(),
                };
                let finding = evidence.verify(validator_pubkeys, quorum_threshold)?;
                let conflict = CertificateConflictRecord { evidence, finding };
                self.conflicts.push(conflict.clone());
                return Err(CertificatePoolError::ObjectVersionConflict {
                    object_version: object_version.clone(),
                    conflict: Box::new(conflict),
                });
            }
        }
        let record = CertificateFinalityRecord {
            certificate_hash,
            certificate: certificate.clone(),
        };
        for object_version in &certificate.object_versions {
            self.by_object_version
                .insert(object_version.clone(), record.clone());
        }
        Ok(certificate_hash)
    }
}
