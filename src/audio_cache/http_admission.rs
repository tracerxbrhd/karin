// SPDX-License-Identifier: GPL-3.0-or-later
//! Local private-cache candidates, deliberately not provider entitlements.
//!
//! OnlineStreamAuthority separately fences current playback access. A candidate
//! cannot publish through the ordinary writer or become a direct cache hit:
//! actual response policy and representation revalidation are mandatory.

use super::*;

/// The ordinary application's private HTTP-cache policy for its supported,
/// explicitly played, unprotected media. This is neither a content license nor
/// permission to export recordings or bypass a provider refusal.
pub struct PrivateHttpCachePolicy;

impl StorageAuthority for PrivateHttpCachePolicy {
    fn private_http(&self) -> bool {
        true
    }

    fn authorize(&self, identity: &ResourceIdentity) -> Result<StorageGrant> {
        Ok(StorageGrant {
            identity: *identity,
            revision: 1,
            content: None,
            basis: AdmissionBasis::PrivateHttpCandidate,
        })
    }

    fn check(&self, candidate: &StorageGrant) -> Result<()> {
        if candidate.revision == 1
            && candidate.content.is_none()
            && candidate.basis == AdmissionBasis::PrivateHttpCandidate
        {
            Ok(())
        } else {
            Err(CacheError::Revoked)
        }
    }
}

#[cfg(test)]
mod tests;
