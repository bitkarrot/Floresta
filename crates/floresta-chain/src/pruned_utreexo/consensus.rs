//! A collection of functions that implement the consensus rules for the Bitcoin Network.
//! This module contains functions that are used to verify blocks and transactions, and doesn't
//! assume anything about the chainstate, so it can be used in any context.
//! We use this to avoid code reuse among the different implementations of the chainstate.

use floresta_common::prelude::*;

use bitcoin::{
    blockdata::constants::COIN_VALUE,
    consensus::Encodable,
    hashes::{sha256, Hash},
    util::uint::Uint256,
    Block, BlockHash, BlockHeader, OutPoint, Transaction, TxOut,
};
use core::ffi::c_uint;
use rustreexo::accumulator::{node_hash::NodeHash, proof::Proof, stump::Stump};
use sha2::{Digest, Sha512_256};

use crate::{BlockValidationErrors, BlockchainError, ChainParams};
/// This struct contains all the information and methods needed to validate a block,
/// it is used by the [ChainState] to validate blocks and transactions.
#[derive(Debug, Clone)]
pub struct Consensus {
    /// The parameters of the chain we are validating, it is usually hardcoded
    /// constants. See [ChainParams] for more information.
    pub parameters: ChainParams,
}

impl Consensus {
    /// Returns the amount of block subsidy to be paid in a block, given it's height.
    /// Bitcoin Core source: https://github.com/bitcoin/bitcoin/blob/2b211b41e36f914b8d0487e698b619039cc3c8e2/src/validation.cpp#L1501-L1512
    pub fn get_subsidy(&self, height: u32) -> u64 {
        let halvings = height / self.parameters.subsidy_halving_interval as u32;
        // Force block reward to zero when right shift is undefined.
        if halvings >= 64 {
            return 0;
        }
        let mut subsidy = 50 * COIN_VALUE;
        // Subsidy is cut in half every 210,000 blocks which will occur approximately every 4 years.
        subsidy >>= halvings;
        subsidy
    }

    /// Returns the hash of a leaf node in the utreexo accumulator.
    #[inline]
    fn get_leaf_hashes(
        transaction: &Transaction,
        vout: u32,
        height: u32,
        block_hash: BlockHash,
    ) -> sha256::Hash {
        let header_code = height << 1;

        let mut ser_utxo = Vec::new();
        let utxo = transaction.output.get(vout as usize).unwrap();
        utxo.consensus_encode(&mut ser_utxo).unwrap();
        let header_code = if transaction.is_coin_base() {
            header_code | 1
        } else {
            header_code
        };

        let leaf_hash = Sha512_256::new()
            .chain_update(block_hash)
            .chain_update(transaction.txid())
            .chain_update(vout.to_le_bytes())
            .chain_update(header_code.to_le_bytes())
            .chain_update(ser_utxo)
            .finalize();
        sha256::Hash::from_slice(leaf_hash.as_slice())
            .expect("parent_hash: Engines shouldn't be Err")
    }
    /// Verify if all transactions in a block are valid. Here we check the following:
    /// - The block must contain at least one transaction, and this transaction must be coinbase
    /// - The first transaction in the block must be coinbase
    /// - The coinbase transaction must have the correct value (subsidy + fees)
    /// - The block must not create more coins than allowed
    /// - All transactions must be valid:
    ///     - The transaction must not be coinbase (already checked)
    ///     - The transaction must not have duplicate inputs
    ///     - The transaction must not spend more coins than it claims in the inputs
    ///     - The transaction must have valid scripts
    pub fn verify_block_transactions(
        mut utxos: HashMap<OutPoint, TxOut>,
        transactions: &[Transaction],
        subsidy: u64,
        verify_script: bool,
        flags: c_uint,
    ) -> Result<bool, BlockchainError> {
        // Blocks must contain at least one transaction
        if transactions.is_empty() {
            return Err(BlockValidationErrors::EmptyBlock.into());
        }
        let mut fee = 0;
        // Skip the coinbase tx
        for (n, transaction) in transactions.iter().enumerate() {
            // We don't need to verify the coinbase inputs, as it spends newly generated coins
            if transaction.is_coin_base() {
                if n == 0 {
                    continue;
                }
                // A block must contain only one coinbase, and it should be the fist thing inside it
                return Err(BlockValidationErrors::FirstTxIsnNotCoinbase.into());
            }
            // Amount of all outputs
            let output_value = transaction.output.iter().fold(0, |acc, tx| acc + tx.value);
            // Amount of all inputs
            let in_value = transaction.input.iter().fold(0, |acc, input| {
                acc + utxos
                    .get(&input.previous_output)
                    .expect("We have all prevouts here")
                    .value
            });
            // Value in should be greater or equal to value out. Otherwise, inflation.
            if output_value > in_value {
                return Err(BlockValidationErrors::NotEnoughMoney.into());
            }
            // Fee is the difference between inputs and outputs
            fee += in_value - output_value;
            // Verify the tx script
            if verify_script {
                transaction.verify_with_flags(|outpoint| utxos.remove(outpoint), flags)?;
            }
        }
        // In each block, the first transaction, and only the first, should be coinbase
        if !transactions[0].is_coin_base() {
            return Err(BlockValidationErrors::FirstTxIsnNotCoinbase.into());
        }
        // Checks if the miner isn't trying to create inflation
        if fee + subsidy
            < transactions[0]
                .output
                .iter()
                .fold(0, |acc, out| acc + out.value)
        {
            return Err(BlockValidationErrors::BadCoinbaseOutValue.into());
        }
        Ok(true)
    }
    /// Calculates the next target for the proof of work algorithm, given the
    /// current target and the time it took to mine the last 2016 blocks.
    pub fn calc_next_work_required(
        last_block: &BlockHeader,
        first_block: &BlockHeader,
        params: ChainParams,
    ) -> u32 {
        let cur_target = last_block.target();

        let expected_timespan = Uint256::from_u64(params.pow_target_timespan).unwrap();
        let mut actual_timespan = last_block.time - first_block.time;
        // Difficulty adjustments are limited, to prevent large swings in difficulty
        // caused by malicious miners.
        if actual_timespan < params.pow_target_timespan as u32 / 4 {
            actual_timespan = params.pow_target_timespan as u32 / 4;
        }
        if actual_timespan > params.pow_target_timespan as u32 * 4 {
            actual_timespan = params.pow_target_timespan as u32 * 4;
        }
        let new_target = cur_target.mul_u32(actual_timespan);
        let new_target = new_target / expected_timespan;

        BlockHeader::compact_target_from_u256(&new_target)
    }
    /// Updates our accumulator with the new block. This is done by calculating the new
    /// root hash of the accumulator, and then verifying the proof of inclusion of the
    /// deleted nodes. If the proof is valid, we return the new accumulator. Otherwise,
    /// we return an error.
    /// This function is pure, it doesn't modify the accumulator, but returns a new one.
    pub fn update_acc(
        acc: &Stump,
        block: &Block,
        height: u32,
        proof: Proof,
        del_hashes: Vec<sha256::Hash>,
    ) -> Result<Stump, BlockchainError> {
        let block_hash = block.block_hash();
        let mut leaf_hashes = Vec::new();
        let del_hashes = del_hashes
            .iter()
            .map(|hash| NodeHash::from(hash.into_inner()))
            .collect::<Vec<_>>();
        // Verify the proof of inclusion of the deleted nodes
        if !proof.verify(&del_hashes, acc)? {
            return Err(BlockValidationErrors::InvalidProof.into());
        }
        // Get inputs from the block, we'll need this HashSet to check if an output is spent
        // in the same block. If it is, we don't need to add it to the accumulator.
        let mut block_inputs = HashSet::new();
        for transaction in block.txdata.iter() {
            for input in transaction.input.iter() {
                block_inputs.insert((input.previous_output.txid, input.previous_output.vout));
            }
        }
        // Get all leaf hashes that will be added to the accumulator
        for transaction in block.txdata.iter() {
            for (i, output) in transaction.output.iter().enumerate() {
                if !output.script_pubkey.is_provably_unspendable()
                    && !block_inputs.contains(&(transaction.txid(), i as u32))
                {
                    leaf_hashes.push(Self::get_leaf_hashes(
                        transaction,
                        i as u32,
                        height,
                        block_hash,
                    ))
                }
            }
        }
        // Convert the leaf hashes to NodeHashes used in Rustreexo
        let hashes: Vec<NodeHash> = leaf_hashes
            .iter()
            .map(|&hash| NodeHash::from(hash.into_inner()))
            .collect();
        // Update the accumulator
        let acc = acc.modify(&hashes, &del_hashes, &proof)?.0;
        Ok(acc)
    }
}
