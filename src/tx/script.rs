#![allow(unused)]
use super::{
    flags::{SCRIPT_VERIFY_P2SH, SCRIPT_VERIFY_WITNESS, ScriptFlags},
    opcodes::{
        OP_0, OP_1, OP_16, OP_CHECKMULTISIG, OP_CHECKMULTISIGVERIFY, OP_CHECKSIG,
        OP_CHECKSIGVERIFY, OP_DUP, OP_EQUAL, OP_EQUALVERIFY, OP_HASH160, OP_PUSHDATA1,
        OP_PUSHDATA2, OP_PUSHDATA4,
    },
};
use crate::{
    packets::{
        tx::{TxOutRef, TxRef},
        varstr::deserialize_array_of_varsrs_iter,
    },
    some_or_break,
    tx::opcodes::OP_INVALIDOPCODE,
    util::hash::hash160,
};
use sha2::{Digest, Sha256};

pub const MAX_PUBKEYS_PER_MULTISIG: u32 = 20;
pub const WITNESS_V0_SCRIPTHASH_SIZE: usize = 32;
pub const WITNESS_V0_KEYHASH_SIZE: usize = 20;
pub const WITNESS_V1_TAPROOT_SIZE: usize = 32;

pub enum AppendableToScript<'l> {
    Int64(i64),
    DataSize(u32),
    ByteArray(&'l [u8]),
}

pub trait Script {
    fn append_item(&mut self, item: AppendableToScript);
}

impl Script for Vec<u8> {
    fn append_item(&mut self, item: AppendableToScript) {
        match item {
            AppendableToScript::Int64(n) => {
                if n == -1 || (1..=16).contains(&n) {
                    self.push((n + (OP_1 - 1) as i64) as u8);
                } else if n == 0 {
                    self.push(OP_0);
                } else {
                    let serialized = serialize_i64(n);
                    self.append_item(AppendableToScript::ByteArray(&serialized));
                }
            }
            AppendableToScript::DataSize(size) => {
                if size < OP_PUSHDATA1 as u32 {
                    self.push(size as u8);
                } else if size <= 0xFF {
                    self.push(OP_PUSHDATA1);
                    self.push(size as u8);
                } else if size <= 0xFFFF {
                    self.push(OP_PUSHDATA2);
                    let le = (size as u16).to_le_bytes();
                    self.extend_from_slice(&le);
                } else {
                    self.push(OP_PUSHDATA4);
                    let le = size.to_le_bytes();
                    self.extend_from_slice(&le);
                }
            }
            AppendableToScript::ByteArray(items) => {
                self.append_item(AppendableToScript::DataSize(items.len() as u32));
                self.extend_from_slice(items);
            }
        }
    }
}

pub enum Destination<'l> {
    None(&'l [u8]),
    PubKey(&'l [u8]),
    PubKeyHash(&'l [u8]),
    ScriptHash(&'l [u8]),
    WitnessV0KeyHash(&'l [u8]),
    WitnessV0ScriptHash(&'l [u8]),
    WitnessV1Taproot(&'l [u8]),
    WitnessUnknown(u8, &'l [u8]),
}

pub fn get_script_for_destination(destination: Destination) -> Vec<u8> {
    match destination {
        Destination::None(items) => items.to_vec(),
        Destination::PubKey(items) => {
            let mut res = vec![];
            res.append_item(AppendableToScript::ByteArray(items));
            res.push(OP_CHECKSIG);
            res
        }
        Destination::PubKeyHash(items) => {
            let mut res = vec![];
            res.push(OP_DUP);
            res.push(OP_HASH160);
            res.append_item(AppendableToScript::ByteArray(items));
            res.push(OP_EQUALVERIFY);
            res.push(OP_CHECKSIG);
            res
        }
        Destination::ScriptHash(items) => {
            let mut res = vec![];
            res.push(OP_HASH160);
            let hashed = hash160(items);
            res.append_item(AppendableToScript::ByteArray(&hashed));
            res.push(OP_EQUAL);
            res
        }
        Destination::WitnessV0KeyHash(items) => {
            let mut res = vec![];
            res.push(OP_0);
            let hashed = hash160(items);
            res.append_item(AppendableToScript::ByteArray(&hashed));
            res
        }
        Destination::WitnessV0ScriptHash(items) => {
            let mut res = vec![];
            res.push(OP_0);
            let hashed: [u8; 32] = Sha256::digest(items).into();
            res.append_item(AppendableToScript::ByteArray(&hashed));
            res
        }
        Destination::WitnessV1Taproot(items) => {
            let mut res = vec![];
            res.push(OP_1);
            res.append_item(AppendableToScript::ByteArray(items));
            res
        }
        Destination::WitnessUnknown(version, program) => {
            let mut res = vec![];
            res.push(encode_op_n(version));
            res.append_item(AppendableToScript::ByteArray(program));
            res
        }
    }
}

pub fn get_transaction_sigop_cost(tx: TxRef, dependencies: &[TxOutRef], flags: ScriptFlags) -> u32 {
    let mut sigops = 4 * get_legacy_sigops(tx);

    if tx.is_coinbase() {
        return sigops;
    }

    if flags & SCRIPT_VERIFY_P2SH != 0 {
        sigops += 4 * get_p2sh_sigop_count(tx, dependencies);
    }

    if flags & SCRIPT_VERIFY_WITNESS != 0 {
        debug_assert!(tx.witness_data().is_some());

        for (txin_idx, txin) in tx.txins().enumerate() {
            let txout = &dependencies[txin_idx];

            let wd = tx.witness_data().unwrap();
            match wd {
                either::Either::Left(wd) => {
                    let a = &wd[txin_idx];
                    sigops += count_witness_sigops(txin.sig_script(), txout.script(), a, flags);
                }
                either::Either::Right(wd) => {
                    let a = wd[txin_idx];
                    sigops += count_witness_sigops(txin.sig_script(), txout.script(), a, flags);
                }
            }
        }
    }

    sigops
}

pub fn get_legacy_sigops(tx: TxRef) -> u32 {
    let txins_sigops: u32 = tx
        .txins()
        .map(|x| get_sigop_count(x.sig_script(), false))
        .sum();

    let txouts_sigops: u32 = tx
        .txouts()
        .map(|x| get_sigop_count(x.script(), false))
        .sum();

    txins_sigops + txouts_sigops
}

pub fn get_sigop_count(script: &[u8], accurate: bool) -> u32 {
    let mut total = 0;
    let mut remaining_script = script;
    let mut last_opcode = OP_INVALIDOPCODE;

    while !remaining_script.is_empty() {
        let (opcode, _) = some_or_break!(get_script_op(&mut remaining_script));

        if opcode == OP_CHECKSIG || opcode == OP_CHECKSIGVERIFY {
            total += 1;
        } else if opcode == OP_CHECKMULTISIG || opcode == OP_CHECKMULTISIGVERIFY {
            if accurate && (OP_1..=OP_16).contains(&last_opcode) {
                total += decode_op_n(last_opcode) as u32;
            } else {
                total += MAX_PUBKEYS_PER_MULTISIG;
            }
        }

        last_opcode = opcode;
    }

    total
}

pub fn get_p2sh_sigop_count(tx: TxRef, dependencies: &[TxOutRef]) -> u32 {
    if tx.is_coinbase() {
        return 0;
    }

    let mut total = 0;

    for (i, txin) in tx.txins().enumerate() {
        let txout = &dependencies[i];

        if is_pay_to_script_hash(txout.script()) {
            total += get_sigop_count2(txout.script(), txin.sig_script());
        }
    }

    total
}

pub fn count_witness_sigops(
    script: &[u8],
    script_pub_key: &[u8],
    witness_components: &[u8],
    flags: ScriptFlags,
) -> u32 {
    if flags & SCRIPT_VERIFY_WITNESS == 0 {
        return 0;
    }

    let wp = get_witness_program(script_pub_key);
    if let Some((version, program)) = wp {
        return witness_sigops(version, program, witness_components);
    }

    if is_pay_to_script_hash(script_pub_key) && is_push_only(script) {
        let mut remaining_script_sig = script;

        let mut last_data = None;
        while !remaining_script_sig.is_empty() {
            if let Some((_, Some(data))) = get_script_op(&mut remaining_script_sig) {
                last_data = Some(data);
            }
        }
        if last_data.is_none() {
            return 0;
        }

        if let Some((version, witness_program)) = get_witness_program(last_data.unwrap()) {
            return witness_sigops(version, witness_program, witness_components);
        }
    }

    0
}

pub fn get_script_op<'a>(script: &mut &'a [u8]) -> Option<(u8, Option<&'a [u8]>)> {
    if script.is_empty() {
        return None;
    }

    let opcode = script[0];
    *script = &script[1..];

    let operand = if opcode <= OP_PUSHDATA4 {
        let mut nsize = 0;
        if opcode < OP_PUSHDATA1 {
            nsize = opcode as usize;
        } else if opcode == OP_PUSHDATA1 {
            if script.is_empty() {
                return None;
            }
            nsize = script[0] as usize;
            *script = &script[1..];
        } else if opcode == OP_PUSHDATA2 {
            if script.len() < 2 {
                return None;
            }
            nsize = u16::from_le_bytes(script[0..2].try_into().unwrap()) as usize;
            *script = &script[2..];
        } else if opcode == OP_PUSHDATA4 {
            if script.len() < 4 {
                return None;
            }
            nsize = u32::from_le_bytes(script[0..4].try_into().unwrap()) as usize;
            *script = &script[4..];
        }

        if script.len() < nsize {
            return None;
        }

        let operand = &script[0..nsize];
        *script = &script[nsize..];
        Some(operand)
    } else {
        None
    };

    Some((opcode, operand))
}

pub fn decode_op_n(opcode: u8) -> u8 {
    if opcode == OP_0 {
        return 0;
    }
    opcode - (OP_1 - 1)
}

pub fn encode_op_n(value: u8) -> u8 {
    if value == 0 {
        return OP_0;
    }
    OP_1 + value - 1
}

pub fn is_pay_to_script_hash(script: &[u8]) -> bool {
    script.len() == 23 && script[0] == OP_HASH160 && script[1] == 0x14 && script[22] == OP_EQUAL
}

pub fn get_sigop_count2(script_pub_key: &[u8], script_sig: &[u8]) -> u32 {
    if !is_pay_to_script_hash(script_pub_key) {
        return get_sigop_count(script_pub_key, true);
    }

    let mut remaining_script = script_sig;
    let mut last_operand = None;

    while !remaining_script.is_empty() {
        if let Some((opcode, operand)) = get_script_op(&mut remaining_script) {
            if opcode > OP_16 {
                return 0;
            }

            last_operand = operand;
        } else {
            return 0;
        }
    }

    match last_operand {
        Some(v) => get_sigop_count(v, true),
        None => 0,
    }
}

pub fn get_witness_program(script: &[u8]) -> Option<(u8, &[u8])> {
    if script.len() < 4 || script.len() > 42 {
        return None;
    }

    if script[0] != OP_0 && (script[0] < OP_1 || script[0] > OP_16) {
        return None;
    }

    if (script[1] + 2) as usize == script.len() {
        let version = decode_op_n(script[0]);
        let program = &script[2..];
        return Some((version, program));
    }

    None
}

pub fn witness_sigops(version: u8, witprogram: &[u8], witness: &[u8]) -> u32 {
    if version == 0 {
        if witprogram.len() == WITNESS_V0_KEYHASH_SIZE {
            return 1;
        }

        // ensure there's at least 1 witness component
        if witprogram.len() == WITNESS_V0_SCRIPTHASH_SIZE && !witness.is_empty() && witness[0] != 0
        {
            let witness_components = deserialize_array_of_varsrs_iter(witness).unwrap();
            let subscript = witness_components.last().unwrap();
            return get_sigop_count(subscript, true);
        }
    }

    0
}

pub fn is_push_only(script: &[u8]) -> bool {
    let mut remaining_script = script;
    while !remaining_script.is_empty() {
        match get_script_op(&mut remaining_script) {
            Some((opcode, _)) => {
                if opcode > OP_16 {
                    return false;
                }
            }
            None => return false,
        }
    }

    true
}

// little endian
pub fn serialize_i64(value: i64) -> Vec<u8> {
    if value == 0 {
        return vec![];
    }

    let mut result = vec![];
    let neg = value < 0;
    let mut absvalue = if neg {
        !(value as u64) + 1
    } else {
        value as u64
    };

    while absvalue != 0 {
        result.push((absvalue & 0xFF) as u8);
        absvalue >>= 8;
    }
    //    - If the most significant byte is >= 0x80 and the value is positive, push a
    //    new zero-byte to make the significant byte < 0x80 again.

    //    - If the most significant byte is >= 0x80 and the value is negative, push a
    //    new 0x80 byte that will be popped off when converting to an integral.

    //    - If the most significant byte is < 0x80 and the value is negative, add
    //    0x80 to it, since it will be subtracted and interpreted as a negative when
    //    converting to an integral.

    if result.last().unwrap() & 0x80 != 0 {
        result.push(if neg { 0x80 } else { 0 });
    } else if neg {
        *result.last_mut().unwrap() |= 0x80;
    }

    result
}

pub fn get_script_for_multisig(n_required: i64, keys: &[&[u8]]) -> Vec<u8> {
    let mut res = vec![];
    res.append_item(AppendableToScript::Int64(n_required));
    for k in keys {
        res.append_item(AppendableToScript::ByteArray(k));
    }
    res.append_item(AppendableToScript::Int64(keys.len() as i64));
    res.push(OP_CHECKMULTISIG);
    res
}
