#![allow(unused)]
pub type ScriptFlags = u32;

pub const SCRIPT_VERIFY_NONE: ScriptFlags = 0;

// Evaluate P2SH subscripts (BIP16).
pub const SCRIPT_VERIFY_P2SH: ScriptFlags = 1 << 0;

// Passing a non-strict-DER signature or one with undefined hashtype to a checksig operation causes script failure.
// Evaluating a pubkey that is not (0x04 + 64 bytes) or (0x02 or 0x03 + 32 bytes) by checksig causes script failure.
// (not used or intended as a consensus rule).
pub const SCRIPT_VERIFY_STRICTENC: ScriptFlags = 1 << 1;

// Passing a non-strict-DER signature to a checksig operation causes script failure (BIP62 rule 1)
pub const SCRIPT_VERIFY_DER_SIG: ScriptFlags = 1 << 2;

// Passing a non-strict-DER signature or one with S > order/2 to a checksig operation causes script failure
// (BIP62 rule 5).
pub const SCRIPT_VERIFY_LOW_S: ScriptFlags = 1 << 3;

// verify dummy stack item consumed by CHECKMULTISIG is of zero-length (BIP62 rule 7).
pub const SCRIPT_VERIFY_NULLDUMMY: ScriptFlags = 1 << 4;

// Using a non-push operator in the scriptSig causes script failure (BIP62 rule 2).
pub const SCRIPT_VERIFY_SIGPUSHONLY: ScriptFlags = 1 << 5;

// Require minimal encodings for all push operations (OP_0... OP_16, OP_1NEGATE where possible, direct
// pushes up to 75 bytes, OP_PUSHDATA up to 255 bytes, OP_PUSHDATA2 for anything larger). Evaluating
// any other push causes the script to fail (BIP62 rule 3).
// In addition, whenever a stack element is interpreted as a number, it must be of minimal length (BIP62 rule 4).
pub const SCRIPT_VERIFY_MINIMALDATA: ScriptFlags = 1 << 6;

// Discourage use of NOPs reserved for upgrades (NOP1-10)
//
// Provided so that nodes can avoid accepting or mining transactions
// containing executed NOP's whose meaning may change after a soft-fork,
// thus rendering the script invalid; with this flag set executing
// discouraged NOPs fails the script. This verification flag will never be
// a mandatory flag applied to scripts in a block. NOPs that are not
// executed, e.g.  within an unexecuted IF ENDIF block, are *not* rejected.
// NOPs that have associated forks to give them new meaning (CLTV, CSV)
// are not subject to this rule.
pub const SCRIPT_VERIFY_DISCOURAGE_UPGRADABLE_NOPS: ScriptFlags = 1 << 7;

// Require that only a single stack element remains after evaluation. This changes the success criterion from
// "At least one stack element must remain, and when interpreted as a boolean, it must be true" to
// "Exactly one stack element must remain, and when interpreted as a boolean, it must be true".
// (BIP62 rule 6)
// Note: CLEANSTACK should never be used without P2SH or WITNESS.
// Note: WITNESS_V0 and TAPSCRIPT script execution have behavior similar to CLEANSTACK as part of their
//       consensus rules. It is automatic there and does not need this flag.
pub const SCRIPT_VERIFY_CLEANSTACK: ScriptFlags = 1 << 8;

// Verify CHECKLOCKTIMEVERIFY
//
// See BIP65 for details.
pub const SCRIPT_VERIFY_CHECKLOCKTIMEVERIFY: ScriptFlags = 1 << 9;

// support CHECKSEQUENCEVERIFY opcode
//
// See BIP112 for details
pub const SCRIPT_VERIFY_CHECKSEQUENCEVERIFY: ScriptFlags = 1 << 10;

// Support segregated witness
pub const SCRIPT_VERIFY_WITNESS: ScriptFlags = 1 << 11;

// Making v1-v16 witness program non-standard
pub const SCRIPT_VERIFY_DISCOURAGE_UPGRADABLE_WITNESS_PROGRAM: ScriptFlags = 1 << 12;

// Segwit script only: Require the argument of OP_IF/NOTIF to be exactly 0x01 or empty vector
//
// Note: TAPSCRIPT script execution has behavior similar to MINIMALIF as part of its consensus
//       rules. It is automatic there and does not depend on this flag.
pub const SCRIPT_VERIFY_MINIMALIF: ScriptFlags = 1 << 13;

// Signature(s) must be empty vector if a CHECK(MULTI)SIG operation failed
pub const SCRIPT_VERIFY_NULLFAIL: ScriptFlags = 1 << 14;

// Public keys in segregated witness scripts must be compressed
pub const SCRIPT_VERIFY_WITNESS_PUBKEYTYPE: ScriptFlags = 1 << 15;

// Making OP_CODESEPARATOR and FindAndDelete fail any non-segwit scripts
pub const SCRIPT_VERIFY_CONST_SCRIPTCODE: ScriptFlags = 1 << 16;

// Taproot/Tapscript validation (BIPs 341 & 342)
pub const SCRIPT_VERIFY_TAPROOT: ScriptFlags = 1 << 17;

// Making unknown Taproot leaf versions non-standard
pub const SCRIPT_VERIFY_DISCOURAGE_UPGRADABLE_TAPROOT_VERSION: ScriptFlags = 1 << 18;

// Making unknown OP_SUCCESS non-standard
pub const SCRIPT_VERIFY_OP_SUCCESS: ScriptFlags = 1 << 19;

// Making unknown public key versions (in BIP 342 scripts) non-standard
pub const SCRIPT_VERIFY_UPGRADABLE_PUBKEYTYPE: ScriptFlags = 1 << 20;
