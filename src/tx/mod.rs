use crate::{
    db::{self, rocksdb::SerializedTx},
    packets::{
        buffer::Buffer,
        deserialize_array_owned,
        tx::{TxOutRef, TxOwned, TxRef},
        varint::{VarInt, length_varint, serialize_varint_into_slice},
    },
    tx::opcodes::OP_EQUAL,
    util::arena::Arena,
};
use bech32::hrp;
use fee::calculate_fee;
use flags::{SCRIPT_VERIFY_P2SH, SCRIPT_VERIFY_WITNESS};
use opcodes::{OP_0, OP_1, OP_CHECKSIG, OP_DUP, OP_EQUALVERIFY, OP_HASH160};
use script::get_transaction_sigop_cost;
use serde::{Deserialize, Serialize};
use size::calculate_tx_size_wus;

mod fee;
mod flags;
mod opcodes;
mod script;
mod size;

#[cfg(test)]
mod script_test;
#[cfg(test)]
mod size_test;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzedTx {
    pub fee: u64,
    pub txouts_sum: u64,
    pub size_wus: u32,
    pub sigops: u32,
    #[serde(serialize_with = "crate::util::serialize_as_hex::serialize_hash_as_hex_reversed")]
    pub block_hash: [u8; 32],
    #[serde(flatten)]
    pub tx: TxOwned,
    pub txin_values: Vec<u64>,
}

pub fn deserialize_analyzed_tx(data: &[u8], hash: [u8; 32]) -> AnalyzedTx {
    let fee = data.get_u64_le(0).expect("to deserialize fee");
    let txouts_sum = data.get_u64_le(8).expect("to deserialize txouts_sum");
    let size_wus = data.get_u32_le(16).expect("to deserialize size_wus");
    let sigops = data.get_u32_le(20).expect("to deserialize sigops");
    let block_hash = *data.get_hash(24).expect("to deserialize block hash");

    let (tx, consumed) = TxOwned::deserialize_without_txouts(
        data.with_offset(56)
            .expect("to offset before deserializing tx"),
        hash,
    )
    .expect("to deserialize tx without txouts");

    let (txin_values, _) = deserialize_array_owned(
        data.with_offset(56 + consumed)
            .expect("to offset before deserializing txin_values"),
    )
    .expect("to deserialize txin_values");

    AnalyzedTx {
        fee,
        txouts_sum,
        size_wus,
        sigops,
        block_hash,
        tx,
        txin_values,
    }
}

// txs are always analyzed when we process and save a block so in case of a reorg the number + hash fields will just be overwritten
// we must take care to analyze blocks in the correct order because of that
pub fn analyze_tx<'arena, 'data>(
    block_hash: [u8; 32],
    tx: TxRef<'data>,
    dependencies: &'data [TxOutRef<'data>],
    arena: &'arena Arena,
) -> db::rocksdb::SerializedTx<'arena> {
    let fee = if tx.is_coinbase() {
        0
    } else {
        calculate_fee(tx, dependencies)
    };

    let txouts_sum: u64 = tx.txouts().map(|x| x.value()).sum();

    let size_wus = calculate_tx_size_wus(tx);

    let mut flags = SCRIPT_VERIFY_P2SH;
    if tx.witness_data().is_some() {
        flags |= SCRIPT_VERIFY_WITNESS;
    }

    let sigops = get_transaction_sigop_cost(tx, dependencies, flags);

    let txin_values_size = length_varint(dependencies.len() as u64) + dependencies.len() * 8;
    let serialized_tx_size = tx.serialized_without_txouts_size();

    let analyzed_tx = arena
        // serialized tx size + fee (8 bytes) + txouts_sum (8 bytes) + size_wus(4 bytes) + sigops (4 bytes) + block_number (8 bytes) + block_hash (32 bytes) + txin_values size
        .try_alloc_array_fill_copy(
            serialized_tx_size + 8 + 8 + 4 + 4 + 8 + 32 + txin_values_size,
            0u8,
        )
        .expect("to allocate space for analyzed tx");

    analyzed_tx[0..8].copy_from_slice(&fee.to_le_bytes());
    analyzed_tx[8..16].copy_from_slice(&txouts_sum.to_le_bytes());
    analyzed_tx[16..20].copy_from_slice(&size_wus.to_le_bytes());
    analyzed_tx[20..24].copy_from_slice(&sigops.to_le_bytes());
    analyzed_tx[24..56].copy_from_slice(&block_hash);
    let mut for_tx = &mut analyzed_tx[56..];
    tx.serialize_without_txouts(&mut for_tx);

    let size_offset = serialize_varint_into_slice(
        dependencies.len() as u64,
        &mut analyzed_tx[56 + serialized_tx_size..],
    );
    for (index, dep) in dependencies.iter().enumerate() {
        let pos = 56 + serialized_tx_size + (8 * index) + size_offset;
        analyzed_tx[pos..pos + 8].copy_from_slice(&dep.value().to_le_bytes());
    }

    let tx_outs = serialize_txouts(tx, arena);
    SerializedTx {
        hash: tx.hash(),
        analyzed_tx: Some(analyzed_tx),
        tx_outs: Some(tx_outs),
        spent_txos: if tx.is_coinbase() {
            None
        } else {
            Some(serialize_spends(tx, arena))
        },
        address_amends: serialize_address_amends(tx, dependencies, arena),
        fee,
        txouts_sum,
        size_wus,
    }
}

fn serialize_txouts<'arena>(tx: TxRef, arena: &'arena Arena) -> &'arena [u8] {
    let mut size = length_varint(tx.txouts().count() as VarInt);
    for txout in tx.txouts() {
        size += txout.serialized_length();
    }
    let result = arena
        .try_alloc_array_fill_copy(size, 0u8)
        .expect("to allocate space for txouts");

    let mut offset = serialize_varint_into_slice(tx.txouts().count() as VarInt, result);
    for txout in tx.txouts() {
        let with_offset = result.get_mut(offset..).unwrap();
        offset += txout.flat_serialize(with_offset);
    }

    debug_assert_eq!(size, offset);

    result
}

fn serialize_spends<'arena>(tx: TxRef, arena: &'arena Arena) -> &'arena [&'arena [u8; 36]] {
    let mut result = arena
        .try_alloc_arenaarray(tx.txins().count())
        .expect("to allocate space for txspends");

    for txin in tx.txins() {
        let key = arena
            .try_alloc([0u8; 36])
            .expect("to allocate space for txspends");
        key[0..32].copy_from_slice(&txin.prevout_hash());
        key[32..36].copy_from_slice(&txin.prevout_index().to_le_bytes());
        result.push(&*key);
    }

    result.into_arena_array()
}

fn serialize_address_amends<'arena, 'data>(
    tx: TxRef,
    deps: &'data [TxOutRef<'data>],
    arena: &'arena Arena,
) -> Option<&'arena [(&'arena [u8], i64)]> {
    let amends_count = if tx.is_coinbase() {
        0
    } else {
        deps.iter()
            .filter(|txout| script_has_address(txout.script()))
            .count()
    } + {
        tx.txouts()
            .filter(|txout| script_has_address(txout.script()))
            .count()
    };
    if amends_count == 0 {
        return None;
    }

    let mut amends = arena
        .try_alloc_arenaarray(amends_count)
        .expect("to allocate space for address amends");

    for txout in deps {
        if let Some(address_bytes) =
            get_address_from_script_copy_into_arena_with_txhash(txout.script(), tx.hash(), arena)
        {
            amends.push((address_bytes, -(txout.value() as i64)));
        }
    }

    for txout in tx.txouts() {
        if let Some(address_bytes) =
            get_address_from_script_copy_into_arena_with_txhash(txout.script(), tx.hash(), arena)
        {
            amends.push((address_bytes, txout.value() as i64));
        }
    }

    amends.sort_unstable_by(|a, b| a.0.cmp(b.0));

    Some(amends.into_arena_array())
}

fn script_has_address(script: &[u8]) -> bool {
    if get_p2pk_address(script).is_some() {
        return true;
    }
    if get_p2sh_address(script).is_some() {
        return true;
    }
    if get_p2pkh_address(script).is_some() {
        return true;
    }
    if get_bech32_address(script).is_some() {
        return true;
    }

    false
}

fn get_address_from_script_copy_into_arena_with_txhash<'data, 'arena: 'data>(
    script: &'data [u8],
    txhash: [u8; 32],
    arena: &'arena Arena,
) -> Option<&'arena [u8]> {
    if let Some(p2pk) = get_p2pk_address(script) {
        let key = arena
            .try_alloc_array_fill_copy(p2pk.len() + 32, 0u8)
            .expect("to allocate space for address bytes");
        key[0..p2pk.len()].copy_from_slice(p2pk);
        key[p2pk.len()..].copy_from_slice(&txhash);
        return Some(&*key);
    }
    if let Some(p2sh) = get_p2sh_address(script) {
        let key = arena
            .try_alloc_array_fill_copy(p2sh.len() + 32, 0u8)
            .expect("to allocate space for address bytes");
        key[0..p2sh.len()].copy_from_slice(p2sh);
        key[p2sh.len()..].copy_from_slice(&txhash);
        return Some(&*key);
    }
    if let Some(p2pkh) = get_p2pkh_address(script) {
        let key = arena
            .try_alloc_array_fill_copy(p2pkh.len() + 32, 0u8)
            .expect("to allocate space for address bytes");
        key[0..p2pkh.len()].copy_from_slice(p2pkh);
        key[p2pkh.len()..].copy_from_slice(&txhash);
        return Some(&*key);
    }
    if let Some(bech32) = get_bech32_address(script) {
        let key = arena
            .try_alloc_array_fill_copy(bech32.len() + 32, 0u8)
            .expect("to allocate space for address bytes");
        key[0..bech32.len()].copy_from_slice(bech32);
        key[bech32.len()..].copy_from_slice(&txhash);
        return Some(&*key);
    }

    None
}

fn get_p2pk_address(script: &[u8]) -> Option<&[u8]> {
    // full public key
    if script.len() == 67 && script[0] == 0x41 && script[66] == OP_CHECKSIG {
        return Some(&script[1..66]);
    }

    // compressed public key
    if script.len() == 35 && script[0] == 0x21 && script[34] == OP_CHECKSIG {
        return Some(&script[1..34]);
    }

    None
}

fn get_p2sh_address(script: &[u8]) -> Option<&[u8]> {
    if script.len() == 23 && script[0] == OP_HASH160 && script[1] == 0x14 && script[22] == OP_EQUAL
    {
        return Some(&script[2..22]);
    }
    None
}

fn get_p2pkh_address(script: &[u8]) -> Option<&[u8]> {
    if !(script.len() == 25
        && script[0] == OP_DUP
        && script[1] == OP_HASH160
        && script[2] == 0x14
        && script[23] == OP_EQUALVERIFY
        && script[24] == OP_CHECKSIG)
    {
        return None;
    }

    Some(&script[3..23])
}

fn get_bech32_address(script: &[u8]) -> Option<&[u8]> {
    if script.len() == 22 && script[0] == OP_0 && script[1] == 0x14 {
        // v0 pay to witness script hash
        return Some(&script[2..]);
    }
    if script.len() == 34 && script[0] == OP_0 && script[1] == 0x20 {
        // v0 pay to witness pubkey hash
        return Some(&script[2..]);
    }
    if script.len() == 34 && script[0] == OP_1 && script[1] == 0x20 {
        // v1 taproot
        return Some(&script[2..]);
    }

    None
}

pub fn get_human_address_from_script(script: &[u8]) -> Option<String> {
    if let Some(p2pk) = get_p2pk_address_human(script) {
        return Some(p2pk);
    }
    if let Some(p2sh) = get_p2sh_address_human(script) {
        return Some(p2sh);
    }
    if let Some(p2pkh) = get_p2pkh_address_human(script) {
        return Some(p2pkh);
    }
    if let Some(bech32) = get_bech32_address_human(script) {
        return Some(bech32);
    }

    None
}

fn get_p2pk_address_human(script: &[u8]) -> Option<String> {
    // full public key
    if script.len() == 67 && script[0] == 0x41 && script[66] == OP_CHECKSIG {
        return Some(hex::encode(&script[1..66]));
    }

    // compressed public key
    if script.len() == 35 && script[0] == 0x21 && script[34] == OP_CHECKSIG {
        return Some(hex::encode(&script[1..34]));
    }

    None
}

fn get_p2sh_address_human(script: &[u8]) -> Option<String> {
    if script.len() == 23 && script[0] == OP_HASH160 && script[1] == 0x14 && script[22] == OP_EQUAL
    {
        return Some(
            bs58::encode(&script[2..22])
                .with_check_version(0x05)
                .into_string(),
        );
    }
    None
}

fn get_p2pkh_address_human(script: &[u8]) -> Option<String> {
    if script.len() == 25
        && script[0] == OP_DUP
        && script[1] == OP_HASH160
        && script[2] == 0x14
        && script[23] == OP_EQUALVERIFY
        && script[24] == OP_CHECKSIG
    {
        return Some(
            bs58::encode(&script[3..23])
                .with_check_version(0x00)
                .into_string(),
        );
    }

    None
}

fn get_bech32_address_human(script: &[u8]) -> Option<String> {
    if script.len() == 22 && script[0] == OP_0 && script[1] == 0x14 {
        // v0 pay to witness script hash
        //return Some(&script[2..]);
        return Some(bech32::segwit::encode_v0(hrp::BC, &script[2..]).unwrap());
    }
    if script.len() == 34 && script[0] == OP_0 && script[1] == 0x20 {
        // v0 pay to witness pubkey hash
        return Some(bech32::segwit::encode_v0(hrp::BC, &script[2..]).unwrap());
    }
    if script.len() == 34 && script[0] == OP_1 && script[1] == 0x20 {
        // v1 taproot
        return Some(bech32::segwit::encode_v1(hrp::BC, &script[2..]).unwrap());
    }

    None
}
