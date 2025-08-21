#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]
#![warn(missing_docs)]
//! Library entry for DLMM Bonding Curve.
//! 
//! This crate provides mathematically rigorous implementations of bonding curves
//! suitable for discrete liquidity market makers (DLMM), with verification tools
//! and visualization capabilities.
//!
//! # Modules
//! - [`curves`]: Price lattice & allocation mechanisms
//! - [`dlmm`]: Fee schedule and launch-phase surcharge
//! - [`verifier`]: Analytic vs numeric checks
//! - [`plot`]: Visualization (optional in binaries)

/// Price lattice and allocation mechanisms for bonding curves
pub mod curves;

/// DLMM fee schedule and launch-phase surcharge policies
pub mod dlmm;

/// Verification tools for curve properties and numerical accuracy
pub mod verifier;

/// Visualization utilities for generating charts
pub mod plot;
