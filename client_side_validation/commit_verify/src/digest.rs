// Client-side-validation foundation libraries for Tondi.
//
// SPDX-License-Identifier: Apache-2.0
//
// Adapted for Tondi by AvatoLabs
// Based on original work by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems (InDCS), Switzerland.
// Copyright (C) 2025 AvatoLabs.
// All rights under the above copyrights are reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not
// use this file except in compliance with the License. You may obtain a copy of
// the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations under
// the License.

//! Cryptographic digest traits and implementations for Tondi L1.
//!
//! Tondi uses BLAKE3 as its universal hash function for all cryptographic
//! commitments, transaction IDs, block hashes, and merkle trees.

use amplify::num::u24;

/// A generic cryptographic digest trait.
///
/// This trait abstracts over different hash functions, allowing the same
/// commitment logic to work with BLAKE3 on Tondi.
pub trait DigestExt<const BYTE_LEN: usize = 32> {
    /// Initialize a digest with a given tag for domain separation.
    ///
    /// Tondi uses BLAKE3's `derive_key` with "tondi.{domain}:{tag}" context.
    fn from_tag(tag: impl AsRef<[u8]>) -> Self;

    /// Digest raw byte slice.
    fn input_raw(&mut self, data: &[u8]);

    /// Digest raw byte slice returning self.
    fn with_raw(mut self, data: &[u8]) -> Self
    where Self: Sized {
        self.input_raw(data);
        self
    }

    /// Digest bytes, adding the data length to the digest (preventing length
    /// extension attack).
    ///
    /// Returns self.
    fn with_len<const MAX: usize>(mut self, data: &[u8]) -> Self
    where Self: Sized {
        self.input_with_len::<MAX>(data);
        self
    }

    /// Digest bytes, adding the data length to the digest (preventing length
    /// extension attack).
    fn input_with_len<const MAX: usize>(&mut self, data: &[u8]) {
        let len = data.len();
        match MAX {
            0..=0xFF => self.input_raw(&(len as u8).to_le_bytes()),
            0x100..=0xFFFF => self.input_raw(&(len as u16).to_le_bytes()),
            0x10000..=0xFFFFFF => self.input_raw(&u24::with(len as u32).to_le_bytes()),
            0x1000000..=0xFFFFFFFF => self.input_raw(&(len as u32).to_le_bytes()),
            _ => panic!("data too large"),
        }
        self.input_raw(data);
    }

    /// Compute the final cryptographic digest.
    fn finish(self) -> [u8; BYTE_LEN];
}

// =============================================================================
// TONDI BLAKE3 IMPLEMENTATION
// =============================================================================

/// BLAKE3 hasher for Tondi L1.
///
/// This is the primary and only hash function for Tondi. It implements
/// domain separation via BLAKE3's `derive_key` functionality for most operations,
/// but uses **Prefix Mode** for transaction IDs to match Tondi's native implementation.
///
/// # Domain Separation Modes
///
/// - **KDF Mode** (most operations): Uses `new_derive_key()` with context strings
///   - Format: "tondi.{domain}:{tag}"
///   - Examples: "tondi.commitment:urn:tondi:rgb:schema#2025-01-01", "tondi.block.hash"
///
/// - **Prefix Mode** (transaction IDs only): Uses regular hasher with `b"TransactionID"` prefix
///   - This matches Tondi's native implementation in `crypto/hashes/src/hashers.rs`
///   - Behavior: `blake3("TransactionID" + tx_data)` instead of KDF mode
#[derive(Clone)]
pub struct TondiBlake3 {
    hasher: blake3::Hasher,
}

impl std::fmt::Debug for TondiBlake3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TondiBlake3")
            .field("hasher", &"<blake3::Hasher>")
            .finish()
    }
}

impl TondiBlake3 {
    /// Create a new BLAKE3 hasher without domain separation.
    pub fn new() -> Self {
        Self {
            hasher: blake3::Hasher::new(),
        }
    }

    /// Create a BLAKE3 hasher with domain separation using derive_key.
    ///
    /// The context string should follow Tondi's domain separation format.
    pub fn from_context(context: &str) -> Self {
        Self {
            hasher: blake3::Hasher::new_derive_key(context),
        }
    }

    /// Create a BLAKE3 hasher for commitment operations.
    pub fn for_commitment(tag: &str) -> Self {
        let context = format!("tondi.commitment:{}", tag);
        Self::from_context(&context)
    }

    /// Create a BLAKE3 hasher for merkle tree operations.
    pub fn for_merkle(tag: &str) -> Self {
        let context = format!("tondi.merkle:{}", tag);
        Self::from_context(&context)
    }

    /// Create a BLAKE3 hasher for transaction operations.
    ///
    /// # Critical: Tondi Compatibility
    ///
    /// This method MUST match Tondi's native implementation exactly:
    /// - Uses **Prefix Mode** (not KDF Mode)
    /// - Prefix: `b"TransactionID"` (13 bytes)
    /// - Behavior: `blake3("TransactionID" + tx_data)`
    ///
    /// Tondi's implementation (from `crypto/hashes/src/hashers.rs`):
    /// ```rust
    /// let prefix = Vec::from(b"TransactionID");
    /// // ... later in finalize:
    /// blake3_256(&self.0) // where self.0 = prefix + data
    /// ```
    ///
    /// This is fundamentally different from BLAKE3's KDF mode (`new_derive_key`),
    /// which modifies the hash state's initialization flags and compresses the
    /// context into the IV. Prefix mode simply concatenates the prefix with the data.
    pub fn for_transaction() -> Self {
        // Use Prefix Mode to match Tondi's native implementation
        // 1. Create a regular hasher (not KDF)
        let mut hasher = blake3::Hasher::new();
        
        // 2. Write the prefix first (matching Tondi's Vec<u8> prefix behavior)
        hasher.update(b"TransactionID");
        
        Self { hasher }
    }

    /// Create a BLAKE3 hasher for block operations.
    pub fn for_block() -> Self {
        Self::from_context("tondi.block.hash")
    }

    /// Create a BLAKE3 hasher for script hash operations.
    pub fn for_script_hash() -> Self {
        Self::from_context("tondi.script.hash")
    }

    /// Create a BLAKE3 hasher for public key hash operations.
    pub fn for_pubkey_hash() -> Self {
        Self::from_context("tondi.pubkey.hash")
    }

    /// Create a BLAKE3 hasher for Copperoot (Taproot) operations.
    pub fn for_copperoot(domain: &str) -> Self {
        let context = format!("tondi.copperoot:{}", domain);
        Self::from_context(&context)
    }

    /// Update the hasher with data.
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Finalize and return the hash.
    pub fn finalize(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }

    /// Compute a one-shot BLAKE3 hash.
    pub fn hash(data: &[u8]) -> [u8; 32] {
        blake3::hash(data).into()
    }

    /// Compute a one-shot BLAKE3 hash with domain separation.
    pub fn hash_with_context(context: &str, data: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_derive_key(context);
        hasher.update(data);
        hasher.finalize().into()
    }

    /// Compute a truncated hash (for 20-byte hashes like pubkey hashes).
    pub fn hash_truncated_20(data: &[u8]) -> [u8; 20] {
        let full = Self::hash(data);
        let mut result = [0u8; 20];
        result.copy_from_slice(&full[..20]);
        result
    }

    /// Compute a truncated hash with domain separation.
    pub fn hash_with_context_truncated_20(context: &str, data: &[u8]) -> [u8; 20] {
        let full = Self::hash_with_context(context, data);
        let mut result = [0u8; 20];
        result.copy_from_slice(&full[..20]);
        result
    }
}

impl Default for TondiBlake3 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::io::Write for TondiBlake3 {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl DigestExt for TondiBlake3 {
    fn from_tag(tag: impl AsRef<[u8]>) -> Self {
        // Convert tag bytes to string for Tondi domain separation
        let tag_str = String::from_utf8_lossy(tag.as_ref());
        Self::for_commitment(&tag_str)
    }

    fn input_raw(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn finish(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

/// 20-byte truncated BLAKE3 hasher for address hashes.
///
/// This is used for public key hashes and script hashes in Tondi addresses.
#[derive(Clone)]
pub struct TondiBlake3_20 {
    inner: TondiBlake3,
}

impl std::fmt::Debug for TondiBlake3_20 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TondiBlake3_20")
            .field("inner", &self.inner)
            .finish()
    }
}

impl TondiBlake3_20 {
    /// Create a new 20-byte truncated hasher.
    pub fn new() -> Self {
        Self {
            inner: TondiBlake3::new(),
        }
    }

    /// Create with domain separation.
    pub fn from_context(context: &str) -> Self {
        Self {
            inner: TondiBlake3::from_context(context),
        }
    }

    /// Update with data.
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finalize to 20 bytes.
    pub fn finalize(self) -> [u8; 20] {
        let full = self.inner.finalize();
        let mut result = [0u8; 20];
        result.copy_from_slice(&full[..20]);
        result
    }
}

impl Default for TondiBlake3_20 {
    fn default() -> Self {
        Self::new()
    }
}

impl DigestExt<20> for TondiBlake3_20 {
    fn from_tag(tag: impl AsRef<[u8]>) -> Self {
        let tag_str = String::from_utf8_lossy(tag.as_ref());
        let context = format!("tondi.hash20:{}", tag_str);
        Self::from_context(&context)
    }

    fn input_raw(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    fn finish(self) -> [u8; 20] {
        self.finalize()
    }
}

// =============================================================================
// TYPE ALIASES
// =============================================================================

/// The default hasher type for Tondi.
pub type DefaultHasher = TondiBlake3;

/// Digest trait re-export for compatibility.
/// Note: This is a simplified version that only requires the methods we need.
pub trait Digest: Sized {
    /// Update the digest with data.
    fn update(&mut self, data: impl AsRef<[u8]>);
    /// Finalize the digest.
    fn finalize(self) -> [u8; 32];
}

impl Digest for TondiBlake3 {
    fn update(&mut self, data: impl AsRef<[u8]>) {
        self.hasher.update(data.as_ref());
    }
    
    fn finalize(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tondi_blake3_basic() {
        let mut hasher = TondiBlake3::new();
        hasher.update(b"tondi");
        let hash = hasher.finalize();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_tondi_blake3_domain_separation() {
        let hash1 = TondiBlake3::hash_with_context("tondi.test1", b"data");
        let hash2 = TondiBlake3::hash_with_context("tondi.test2", b"data");
        // Different domains should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_tondi_blake3_from_tag() {
        let mut hasher = TondiBlake3::from_tag(b"test-tag");
        hasher.input_raw(b"test data");
        let hash = hasher.finish();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_digest_ext_with_len() {
        let hasher = TondiBlake3::new().with_len::<256>(b"test data");
        let hash = hasher.finish();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_truncated_20() {
        let hash = TondiBlake3::hash_truncated_20(b"test data");
        assert_eq!(hash.len(), 20);
    }

    #[test]
    fn test_tondi_blake3_20() {
        let mut hasher = TondiBlake3_20::new();
        hasher.update(b"test data");
        let hash = hasher.finalize();
        assert_eq!(hash.len(), 20);
    }

    #[test]
    fn test_for_specific_domains() {
        let _tx_hasher = TondiBlake3::for_transaction();
        let _block_hasher = TondiBlake3::for_block();
        let _merkle_hasher = TondiBlake3::for_merkle("node");
        let _copperoot_hasher = TondiBlake3::for_copperoot("leaf");
    }
}
