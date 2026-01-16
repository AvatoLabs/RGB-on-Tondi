// Client-side-validation foundation libraries.
//
// SPDX-License-Identifier: Apache-2.0
//
// Designed in 2019-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Written in 2024-2025 by Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
// Adapted for Tondi AnnexRet by AvatoLabs
//
// Copyright (C) 2019-2024 LNP/BP Standards Association, Switzerland.
// Copyright (C) 2024-2025 LNP/BP Laboratories,
//                         Institute for Distributed and Cognitive Systems
// (InDCS), Switzerland. Copyright (C) 2019-2025 Dr Maxim Orlovsky.
// Copyright (C) 2025 AvatoLabs. All rights reserved.
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

//! Annex Return (AnnexRet) commitment mechanism for Tondi transactions.
//!
//! This module provides functionality to embed RGB's MPC Root (32-byte commitment hash)
//! into Tondi transaction's Annex field using TLV format.
//!
//! # Format
//!
//! The Annex field format for RGB commitments:
//! - Prefix: `0x50` (TAPROOT_ANNEX_PREFIX)
//! - TLV Type: `0x20` (RGB_COMMITMENT)
//! - Length: `0x20` (32 bytes)
//! - Value: 32-byte commitment hash (MPC Root)
//!
//! Total Annex size: 1 (prefix) + 1 (type) + 1 (length) + 32 (value) = 35 bytes

use crate::CommitmentProtocol;

/// TLV Type for RGB commitment in Annex.
pub const RGB_COMMITMENT_TLV_TYPE: u8 = 0x20;

/// Length of RGB commitment hash (32 bytes).
pub const RGB_COMMITMENT_LEN: u8 = 0x20;

/// Taproot Annex prefix (must be first byte).
pub const TAPROOT_ANNEX_PREFIX: u8 = 0x50;

/// Error during AnnexRet commitment operations.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum AnnexRetError {
    /// Invalid Annex format: expected prefix `0x50` but found `{0:#02x}`
    InvalidPrefix(u8),

    /// Invalid TLV type: expected `0x20` (RGB_COMMITMENT) but found `{0:#02x}`
    InvalidTlvType(u8),

    /// Invalid TLV length: expected `32` but found `{0}`
    InvalidTlvLength(u8),

    /// Annex data is too short: expected at least `{0}` bytes but got `{1}`
    InsufficientData(usize, usize),

    /// Annex data is too long: expected `{0}` bytes but got `{1}`
    ExcessData(usize, usize),

    /// No RGB commitment found in Annex
    NoCommitment,
}

/// Protocol marker for AnnexRet commitment scheme.
pub struct AnnexRetProtocol;
impl CommitmentProtocol for AnnexRetProtocol {}

/// Creates Annex bytes with embedded RGB commitment.
///
/// # Format
///
/// The returned bytes follow this structure:
/// - Byte 0: `0x50` (TAPROOT_ANNEX_PREFIX)
/// - Byte 1: `0x20` (RGB_COMMITMENT_TLV_TYPE)
/// - Byte 2: `0x20` (RGB_COMMITMENT_LEN = 32)
/// - Bytes 3-34: 32-byte commitment hash
///
/// # Arguments
///
/// * `commitment` - The 32-byte MPC Root commitment hash
///
/// # Returns
///
/// A vector of 35 bytes containing the Annex data with embedded commitment.
pub fn create_annex_with_commitment(commitment: &[u8; 32]) -> Vec<u8> {
    let mut annex = Vec::with_capacity(35);
    annex.push(TAPROOT_ANNEX_PREFIX);
    annex.push(RGB_COMMITMENT_TLV_TYPE);
    annex.push(RGB_COMMITMENT_LEN);
    annex.extend_from_slice(commitment);
    annex
}

/// Extracts RGB commitment from Annex bytes.
///
/// # Arguments
///
/// * `annex_bytes` - The Annex field bytes from a Tondi transaction
///
/// # Returns
///
/// The 32-byte commitment hash if found, or an error if the Annex format is invalid.
///
/// # Errors
///
/// Returns `AnnexRetError` if:
/// - The Annex doesn't start with `0x50`
/// - The TLV type is not `0x20`
/// - The TLV length is not `32`
/// - The Annex is too short or contains extra data
/// - No RGB commitment is found
pub fn extract_commitment_from_annex(annex_bytes: &[u8]) -> Result<[u8; 32], AnnexRetError> {
    // Check minimum length: prefix + type + length + commitment = 35 bytes
    if annex_bytes.len() < 35 {
        return Err(AnnexRetError::InsufficientData(35, annex_bytes.len()));
    }

    // Check prefix
    if annex_bytes[0] != TAPROOT_ANNEX_PREFIX {
        return Err(AnnexRetError::InvalidPrefix(annex_bytes[0]));
    }

    // Check TLV type
    if annex_bytes[1] != RGB_COMMITMENT_TLV_TYPE {
        return Err(AnnexRetError::InvalidTlvType(annex_bytes[1]));
    }

    // Check TLV length
    if annex_bytes[2] != RGB_COMMITMENT_LEN {
        return Err(AnnexRetError::InvalidTlvLength(annex_bytes[2]));
    }

    // Extract commitment (bytes 3-34)
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(&annex_bytes[3..35]);

    // Ensure no extra data (strict: Annex must contain ONLY the RGB commitment)
    if annex_bytes.len() > 35 {
        return Err(AnnexRetError::ExcessData(35, annex_bytes.len()));
    }

    Ok(commitment)
}

/// Verifies that Annex contains a valid RGB commitment matching the expected hash.
///
/// # Arguments
///
/// * `annex_bytes` - The Annex field bytes from a Tondi transaction
/// * `expected_commitment` - The expected 32-byte commitment hash
///
/// # Returns
///
/// `Ok(())` if the Annex contains the expected commitment, or an error otherwise.
pub fn verify_annex_commitment(
    annex_bytes: &[u8],
    expected_commitment: &[u8; 32],
) -> Result<(), AnnexRetError> {
    let commitment = extract_commitment_from_annex(annex_bytes)?;
    if commitment == *expected_commitment {
        Ok(())
    } else {
        Err(AnnexRetError::NoCommitment)
    }
}

#[cfg(test)]
mod test {
    #![cfg_attr(coverage_nightly, coverage(off))]

    use super::*;

    #[test]
    fn test_create_annex() {
        let commitment = [0xABu8; 32];
        let annex = create_annex_with_commitment(&commitment);

        assert_eq!(annex.len(), 35);
        assert_eq!(annex[0], TAPROOT_ANNEX_PREFIX);
        assert_eq!(annex[1], RGB_COMMITMENT_TLV_TYPE);
        assert_eq!(annex[2], RGB_COMMITMENT_LEN);
        assert_eq!(&annex[3..35], &commitment);
    }

    #[test]
    fn test_extract_commitment() {
        let commitment = [0xCDu8; 32];
        let annex = create_annex_with_commitment(&commitment);

        let extracted = extract_commitment_from_annex(&annex).unwrap();
        assert_eq!(extracted, commitment);
    }

    #[test]
    fn test_extract_invalid_prefix() {
        let mut annex = vec![0x51u8]; // Wrong prefix
        annex.extend_from_slice(&[RGB_COMMITMENT_TLV_TYPE, RGB_COMMITMENT_LEN]);
        annex.extend_from_slice(&[0u8; 32]);

        let result = extract_commitment_from_annex(&annex);
        assert!(matches!(result, Err(AnnexRetError::InvalidPrefix(0x51))));
    }

    #[test]
    fn test_extract_invalid_tlv_type() {
        let mut annex = vec![TAPROOT_ANNEX_PREFIX];
        annex.push(0x21); // Wrong TLV type
        annex.push(RGB_COMMITMENT_LEN);
        annex.extend_from_slice(&[0u8; 32]);

        let result = extract_commitment_from_annex(&annex);
        assert!(matches!(result, Err(AnnexRetError::InvalidTlvType(0x21))));
    }

    #[test]
    fn test_extract_invalid_length() {
        let mut annex = vec![TAPROOT_ANNEX_PREFIX, RGB_COMMITMENT_TLV_TYPE];
        annex.push(0x21); // Wrong length
        annex.extend_from_slice(&[0u8; 33]);

        let result = extract_commitment_from_annex(&annex);
        assert!(matches!(result, Err(AnnexRetError::InvalidTlvLength(0x21))));
    }

    #[test]
    fn test_extract_insufficient_data() {
        let annex = vec![TAPROOT_ANNEX_PREFIX, RGB_COMMITMENT_TLV_TYPE, RGB_COMMITMENT_LEN];
        // Missing commitment bytes

        let result = extract_commitment_from_annex(&annex);
        assert!(matches!(result, Err(AnnexRetError::InsufficientData(35, 3))));
    }

    #[test]
    fn test_extract_excess_data() {
        let mut annex = create_annex_with_commitment(&[0u8; 32]);
        annex.push(0xFF); // Extra byte

        let result = extract_commitment_from_annex(&annex);
        assert!(matches!(result, Err(AnnexRetError::ExcessData(35, 36))));
    }

    #[test]
    fn test_verify_commitment() {
        let commitment = [0xEFu8; 32];
        let annex = create_annex_with_commitment(&commitment);

        assert!(verify_annex_commitment(&annex, &commitment).is_ok());

        let wrong_commitment = [0x00u8; 32];
        assert!(verify_annex_commitment(&annex, &wrong_commitment).is_err());
    }

    #[test]
    fn test_round_trip() {
        let original_commitment = [
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
        ];

        let annex = create_annex_with_commitment(&original_commitment);
        let extracted = extract_commitment_from_annex(&annex).unwrap();
        assert_eq!(extracted, original_commitment);

        assert!(verify_annex_commitment(&annex, &original_commitment).is_ok());
    }
}

