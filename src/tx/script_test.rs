use k256::elliptic_curve::rand_core::OsRng;
use supercow::Supercow;

use crate::{
    packets::{
        SupercowVec,
        tx::{Tx, TxIn, TxOut},
        varstr::VarStr,
    },
    tx::{
        opcodes::{OP_0, OP_1, OP_2, OP_CHECKMULTISIG, OP_CHECKSIG, OP_ENDIF, OP_IF},
        script::{
            AppendableToScript, Destination, MAX_PUBKEYS_PER_MULTISIG, Script,
            get_script_for_destination, get_script_for_multisig, get_sigop_count, get_sigop_count2,
            get_transaction_sigop_cost,
        },
    },
};

use super::{
    flags::{SCRIPT_VERIFY_P2SH, SCRIPT_VERIFY_WITNESS},
    opcodes::OP_CHECKMULTISIGVERIFY,
};

#[test]
fn test_get_sigop_count() {
    let mut s1 = vec![];
    assert_eq!(0, get_sigop_count(&s1, false));
    assert_eq!(0, get_sigop_count(&s1, true));

    let dummy = vec![0u8; 20];
    s1.push(OP_1);
    s1.append_item(AppendableToScript::ByteArray(&dummy));
    s1.push(OP_2);
    s1.push(OP_CHECKMULTISIG);
    assert_eq!(2, get_sigop_count(&s1, true));
    s1.push(OP_IF);
    s1.push(OP_CHECKSIG);
    s1.push(OP_ENDIF);
    assert_eq!(3, get_sigop_count(&s1, true));
    assert_eq!(21, get_sigop_count(&s1, false));

    let p2sh = get_script_for_destination(Destination::ScriptHash(&s1));
    let mut script_sig = vec![OP_0];
    script_sig.append_item(AppendableToScript::ByteArray(&s1));
    assert_eq!(3, get_sigop_count2(&p2sh, &script_sig));

    let mut keys = vec![];

    for _ in 0..3 {
        let k = k256::SecretKey::random(&mut OsRng {});
        keys.push(k.public_key().to_sec1_bytes());
    }

    // Ok so I spent a lot of time digging thru the bitcoin core codebase to figure out why my tests were failing here.
    // There are 6 (six!!!!!!!!!!!) << overrides for CScript in that c++ codebase.
    // Insane levels of indirections if im being honest.
    let keys_refs = keys.iter().map(|x| x.as_ref()).collect::<Vec<&[u8]>>();
    let s2 = get_script_for_multisig(1, &keys_refs);
    assert_eq!(3, get_sigop_count(&s2, true));
    assert_eq!(20, get_sigop_count(&s2, false));

    let p2sh = get_script_for_destination(Destination::ScriptHash(&s2));
    assert_eq!(0, get_sigop_count(&p2sh, true));
    assert_eq!(0, get_sigop_count(&p2sh, false));

    let mut script_sig2 = vec![OP_1];
    script_sig2.append_item(AppendableToScript::ByteArray(&dummy));
    script_sig2.append_item(AppendableToScript::ByteArray(&dummy));
    script_sig2.append_item(AppendableToScript::ByteArray(&s2));
    assert_eq!(3, get_sigop_count2(&p2sh, &script_sig2));
}

#[test]
fn test_get_transaction_sigop_cost() {
    let key = k256::SecretKey::random(&mut OsRng {});
    let pubkey = key.public_key().to_sec1_bytes();
    let flags_witness = SCRIPT_VERIFY_WITNESS | SCRIPT_VERIFY_P2SH;
    let flags_no_witness = SCRIPT_VERIFY_P2SH;

    // Multisig script (legacy counting)
    {
        let mut creation_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut spending_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut script_pubkey = vec![];
        script_pubkey.append_item(AppendableToScript::Int64(1));
        script_pubkey.append_item(AppendableToScript::ByteArray(&pubkey));
        script_pubkey.append_item(AppendableToScript::ByteArray(&pubkey));
        script_pubkey.append_item(AppendableToScript::Int64(2));
        script_pubkey.push(OP_CHECKMULTISIGVERIFY);

        let script_sig = vec![OP_0, OP_0];
        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey,
            script_sig,
            None,
        );

        assert_eq!(
            0,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_no_witness)
        );
        assert_eq!(
            MAX_PUBKEYS_PER_MULTISIG * 4,
            get_transaction_sigop_cost(&creation_tx, &[], flags_no_witness)
        );
    }

    // Multisig nested in P2SH
    {
        let mut creation_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut spending_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut redeem_script = vec![];
        redeem_script.append_item(AppendableToScript::Int64(1));
        redeem_script.append_item(AppendableToScript::ByteArray(&pubkey));
        redeem_script.append_item(AppendableToScript::ByteArray(&pubkey));
        redeem_script.append_item(AppendableToScript::Int64(2));
        redeem_script.push(OP_CHECKMULTISIGVERIFY);

        let script_pubkey = get_script_for_destination(Destination::ScriptHash(&redeem_script));
        let mut script_sig = vec![OP_0, OP_0];
        script_sig.append_item(AppendableToScript::ByteArray(&redeem_script));

        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey,
            script_sig,
            None,
        );

        assert_eq!(
            8,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_no_witness)
        );
    }

    // P2WPKH witness program
    {
        let mut creation_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut spending_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut script_pubkey = get_script_for_destination(Destination::WitnessV0KeyHash(&pubkey));
        let script_sig = vec![];

        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey.clone(),
            script_sig.clone(),
            Some(vec![vec![], vec![]]),
        );
        assert_eq!(
            1,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_witness)
        );
        // No signature operations if we dont verify the witness
        assert_eq!(
            0,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_no_witness)
        );

        // The sigop cost for witness != 0 is zero
        assert_eq!(0, script_pubkey[0]);
        script_pubkey[0] = 0x51;
        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey.clone(),
            script_sig.clone(),
            Some(vec![vec![], vec![]]),
        );
        assert_eq!(
            0,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_witness)
        );

        script_pubkey[0] = 0x00;
        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey.clone(),
            script_sig.clone(),
            Some(vec![vec![], vec![]]),
        );
        // The witness of a coinbase transaction is not taken into account
        spending_tx.txins = Supercow::owned(SupercowVec::from_owned(vec![TxIn {
            prevout_hash: Supercow::owned([0u8; 32]),
            prevout_index: 0xFFFFFFFF,
            sequence: 0,
            sig_script: Supercow::owned(VarStr::from_owned(script_sig)),
        }]));
        assert_eq!(
            0,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_witness)
        );
    }

    // P2WPKH nested in P2SH
    {
        let mut creation_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut spending_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut script_sig = get_script_for_destination(Destination::WitnessV0KeyHash(&pubkey));
        let script_pubkey = get_script_for_destination(Destination::ScriptHash(&script_sig));
        let mut script_sig2 = vec![];
        script_sig2.append_item(AppendableToScript::ByteArray(&script_sig));
        script_sig = script_sig2;

        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey,
            script_sig,
            Some(vec![vec![], vec![]]),
        );

        assert_eq!(
            1,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_witness)
        );
    }

    // P2WSH witness program
    {
        let mut creation_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut spending_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut witness_script = vec![];
        witness_script.append_item(AppendableToScript::Int64(1));
        witness_script.append_item(AppendableToScript::ByteArray(&pubkey));
        witness_script.append_item(AppendableToScript::ByteArray(&pubkey));
        witness_script.append_item(AppendableToScript::Int64(2));
        witness_script.push(OP_CHECKMULTISIGVERIFY);
        let script_pubkey =
            get_script_for_destination(Destination::WitnessV0ScriptHash(&witness_script));
        let script_sig = vec![];
        let script_witness = vec![witness_script];

        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey,
            script_sig,
            Some(script_witness),
        );

        assert_eq!(
            2,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_witness)
        );
        // no sig ops if we dont verify the witness
        assert_eq!(
            0,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_no_witness)
        );
    }

    // P2WSH nested in P2SH
    {
        let mut creation_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut spending_tx = Tx {
            version: 0,
            locktime: 0,
            txins: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            txouts: Supercow::owned(SupercowVec {
                inner: Supercow::owned(vec![]),
            }),
            witness_data: None,
            hash: [0u8; 32],
        };
        let mut witness_script = vec![];
        witness_script.append_item(AppendableToScript::Int64(1));
        witness_script.append_item(AppendableToScript::ByteArray(&pubkey));
        witness_script.append_item(AppendableToScript::ByteArray(&pubkey));
        witness_script.append_item(AppendableToScript::Int64(2));
        witness_script.push(OP_CHECKMULTISIGVERIFY);
        let redeem_script =
            get_script_for_destination(Destination::WitnessV0ScriptHash(&witness_script));
        let script_pubkey = get_script_for_destination(Destination::ScriptHash(&redeem_script));
        let mut script_sig = vec![];
        script_sig.append_item(AppendableToScript::ByteArray(&redeem_script));
        let script_witness = vec![witness_script];

        let deps = build_txs(
            &mut spending_tx,
            &mut creation_tx,
            script_pubkey,
            script_sig,
            Some(script_witness),
        );

        assert_eq!(
            2,
            get_transaction_sigop_cost(&spending_tx, &deps, flags_witness)
        );
    }
}

fn build_txs<'creation>(
    spending_tx: &mut Tx,
    creation_tx: &'creation mut Tx,
    script_pubkey: Vec<u8>,
    script_sig: Vec<u8>,
    witness: Option<Vec<Vec<u8>>>,
) -> Vec<&'creation TxOut<'creation>> {
    creation_tx.version = 1;
    creation_tx.txins = Supercow::owned(SupercowVec {
        inner: Supercow::owned(vec![]),
    });
    creation_tx.txouts = Supercow::owned(SupercowVec::from_owned(vec![TxOut {
        value: 1,
        script: Supercow::owned(VarStr::from_owned(script_pubkey)),
    }]));
    creation_tx.witness_data = None;

    spending_tx.version = 1;
    spending_tx.txins = Supercow::owned(SupercowVec::from_owned(vec![TxIn {
        prevout_hash: Supercow::owned(creation_tx.hash),
        prevout_index: 0,
        sequence: 0,
        sig_script: Supercow::owned(VarStr::from_owned(script_sig)),
    }]));
    if let Some(witness) = witness {
        let mapped_to_varstrs = witness
            .into_iter()
            .map(VarStr::from_owned)
            .collect::<Vec<VarStr>>();
        spending_tx.witness_data = Some(Supercow::owned(SupercowVec::from_owned(vec![
            SupercowVec::from_owned(mapped_to_varstrs),
        ])));
    } else {
        spending_tx.witness_data = None;
    }

    vec![&creation_tx.txouts.inner.first().unwrap()]
}
