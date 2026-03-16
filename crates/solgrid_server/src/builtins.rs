//! Built-in function and global variable definitions for Solidity (>=0.8.0) and Yul.
//!
//! Provides hover documentation for native identifiers that are not user-defined
//! (e.g. `msg.sender`, `keccak256`, `abi.encode`, Yul opcodes like `mload`).

/// A built-in symbol definition for hover display.
pub struct BuiltinDef {
    /// The signature shown in the code block, e.g. `"keccak256(bytes memory) returns (bytes32)"`.
    pub signature: &'static str,
    /// Markdown description shown below the signature.
    pub description: &'static str,
}

/// A namespace with members accessible via dot notation (e.g. `msg.sender`).
struct Namespace {
    /// The namespace name (e.g. `"msg"`).
    name: &'static str,
    /// Hover shown when hovering on the namespace itself.
    summary: BuiltinDef,
    /// Members of the namespace.
    members: &'static [(&'static str, BuiltinDef)],
}

// ---------------------------------------------------------------------------
// Solidity global functions
// ---------------------------------------------------------------------------

static SOLIDITY_GLOBALS: &[(&str, BuiltinDef)] = &[
    // Cryptographic functions
    (
        "keccak256",
        BuiltinDef {
            signature: "keccak256(bytes memory) returns (bytes32)",
            description: "Compute the Keccak-256 hash of the input.",
        },
    ),
    (
        "sha256",
        BuiltinDef {
            signature: "sha256(bytes memory) returns (bytes32)",
            description: "Compute the SHA-256 hash of the input.",
        },
    ),
    (
        "ripemd160",
        BuiltinDef {
            signature: "ripemd160(bytes memory) returns (bytes20)",
            description: "Compute the RIPEMD-160 hash of the input.",
        },
    ),
    (
        "ecrecover",
        BuiltinDef {
            signature: "ecrecover(bytes32 hash, uint8 v, bytes32 r, bytes32 s) returns (address)",
            description: "Recover the address associated with the public key from an elliptic curve signature. Returns zero on error.",
        },
    ),
    // Math functions
    (
        "addmod",
        BuiltinDef {
            signature: "addmod(uint256 x, uint256 y, uint256 k) returns (uint256)",
            description: "Compute `(x + y) % k` where the addition is performed with arbitrary precision and does not wrap around at `2**256`. Reverts if `k == 0`.",
        },
    ),
    (
        "mulmod",
        BuiltinDef {
            signature: "mulmod(uint256 x, uint256 y, uint256 k) returns (uint256)",
            description: "Compute `(x * y) % k` where the multiplication is performed with arbitrary precision and does not wrap around at `2**256`. Reverts if `k == 0`.",
        },
    ),
    // Gas & block
    (
        "gasleft",
        BuiltinDef {
            signature: "gasleft() returns (uint256)",
            description: "Remaining gas.",
        },
    ),
    (
        "blockhash",
        BuiltinDef {
            signature: "blockhash(uint256 blockNumber) returns (bytes32)",
            description: "Hash of the given block — only works for the 256 most recent blocks, excluding the current one.",
        },
    ),
    (
        "blobhash",
        BuiltinDef {
            signature: "blobhash(uint256 index) returns (bytes32)",
            description: "Versioned hash of the `index`-th blob associated with the current transaction (EIP-4844). Returns zero if no blob exists at the given index.",
        },
    ),
    // Control flow
    (
        "require",
        BuiltinDef {
            signature: "require(bool condition [, string memory message])",
            description: "Revert execution if `condition` is `false`. Optionally provide an error message string. Use for validating inputs and preconditions.",
        },
    ),
    (
        "assert",
        BuiltinDef {
            signature: "assert(bool condition)",
            description: "Revert execution if `condition` is `false`. Use for checking internal errors and invariants. Consumes all remaining gas (uses the `INVALID` opcode).",
        },
    ),
    (
        "revert",
        BuiltinDef {
            signature: "revert([string memory reason])",
            description: "Abort execution and revert state changes, optionally providing an explanation string. Can also be used with custom errors: `revert CustomError(arg)`.",
        },
    ),
    // Destructive
    (
        "selfdestruct",
        BuiltinDef {
            signature: "selfdestruct(address payable recipient)",
            description: "Deprecated (EIP-6049). Send all Ether held by the contract to `recipient` and mark the contract for destruction.",
        },
    ),
];

// ---------------------------------------------------------------------------
// Solidity namespaces (msg, block, tx, abi, string, bytes)
// ---------------------------------------------------------------------------

static NAMESPACES: &[Namespace] = &[
    // -- msg --
    Namespace {
        name: "msg",
        summary: BuiltinDef {
            signature: "msg",
            description: "\
Properties of the current message call:\n\n\
| Member | Type | Description |\n\
|--------|------|-------------|\n\
| `sender` | `address` | Sender of the message (current call) |\n\
| `value` | `uint256` | Number of wei sent with the message |\n\
| `data` | `bytes calldata` | Complete calldata |\n\
| `sig` | `bytes4` | First four bytes of the calldata (function selector) |",
        },
        members: &[
            (
                "sender",
                BuiltinDef {
                    signature: "msg.sender -> (address)",
                    description: "Sender of the message (current call).",
                },
            ),
            (
                "value",
                BuiltinDef {
                    signature: "msg.value -> (uint256)",
                    description: "Number of wei sent with the message.",
                },
            ),
            (
                "data",
                BuiltinDef {
                    signature: "msg.data -> (bytes calldata)",
                    description: "Complete calldata.",
                },
            ),
            (
                "sig",
                BuiltinDef {
                    signature: "msg.sig -> (bytes4)",
                    description: "First four bytes of the calldata (function selector).",
                },
            ),
        ],
    },
    // -- block --
    Namespace {
        name: "block",
        summary: BuiltinDef {
            signature: "block",
            description: "\
Properties of the current block:\n\n\
| Member | Type | Description |\n\
|--------|------|-------------|\n\
| `timestamp` | `uint256` | Current block timestamp (seconds since epoch) |\n\
| `number` | `uint256` | Current block number |\n\
| `basefee` | `uint256` | Current block's base fee (EIP-3198) |\n\
| `prevrandao` | `uint256` | Random value provided by the beacon chain (EIP-4399) |\n\
| `gaslimit` | `uint256` | Current block gas limit |\n\
| `coinbase` | `address payable` | Current block miner/proposer |\n\
| `chainid` | `uint256` | Current chain ID |\n\
| `blobbasefee` | `uint256` | Current block's blob base fee (EIP-7516) |\n\
| `difficulty` | `uint256` | Alias for `prevrandao` (deprecated post-merge) |",
        },
        members: &[
            (
                "timestamp",
                BuiltinDef {
                    signature: "block.timestamp -> (uint256)",
                    description: "Current block timestamp as seconds since Unix epoch.",
                },
            ),
            (
                "number",
                BuiltinDef {
                    signature: "block.number -> (uint256)",
                    description: "Current block number.",
                },
            ),
            (
                "basefee",
                BuiltinDef {
                    signature: "block.basefee -> (uint256)",
                    description: "Current block's base fee (EIP-3198).",
                },
            ),
            (
                "prevrandao",
                BuiltinDef {
                    signature: "block.prevrandao -> (uint256)",
                    description: "Random value provided by the beacon chain (EIP-4399). Replaces `block.difficulty` post-merge.",
                },
            ),
            (
                "gaslimit",
                BuiltinDef {
                    signature: "block.gaslimit -> (uint256)",
                    description: "Current block gas limit.",
                },
            ),
            (
                "coinbase",
                BuiltinDef {
                    signature: "block.coinbase -> (address payable)",
                    description: "Current block miner/proposer address.",
                },
            ),
            (
                "chainid",
                BuiltinDef {
                    signature: "block.chainid -> (uint256)",
                    description: "Current chain ID.",
                },
            ),
            (
                "blobbasefee",
                BuiltinDef {
                    signature: "block.blobbasefee -> (uint256)",
                    description: "Current block's blob base fee (EIP-7516).",
                },
            ),
            (
                "difficulty",
                BuiltinDef {
                    signature: "block.difficulty -> (uint256)",
                    description: "Deprecated post-merge. Alias for `block.prevrandao`.",
                },
            ),
        ],
    },
    // -- tx --
    Namespace {
        name: "tx",
        summary: BuiltinDef {
            signature: "tx",
            description: "\
Properties of the current transaction:\n\n\
| Member | Type | Description |\n\
|--------|------|-------------|\n\
| `gasprice` | `uint256` | Gas price of the transaction |\n\
| `origin` | `address` | Sender of the transaction (full call chain) |",
        },
        members: &[
            (
                "gasprice",
                BuiltinDef {
                    signature: "tx.gasprice -> (uint256)",
                    description: "Gas price of the transaction.",
                },
            ),
            (
                "origin",
                BuiltinDef {
                    signature: "tx.origin -> (address)",
                    description: "Sender of the transaction (full call chain). **Avoid using for authorization** — use `msg.sender` instead.",
                },
            ),
        ],
    },
    // -- abi --
    Namespace {
        name: "abi",
        summary: BuiltinDef {
            signature: "abi",
            description: "\
ABI encoding and decoding functions:\n\n\
| Member | Description |\n\
|--------|-------------|\n\
| `encode(...)` | ABI-encode the given arguments |\n\
| `encodePacked(...)` | Packed (non-standard) encoding |\n\
| `encodeWithSelector(bytes4, ...)` | Encode with a function selector |\n\
| `encodeWithSignature(string, ...)` | Encode with a function signature string |\n\
| `encodeCall(fn, (...))` | Type-safe ABI encoding for a function call |\n\
| `decode(bytes, (types))` | ABI-decode data |",
        },
        members: &[
            (
                "encode",
                BuiltinDef {
                    signature: "abi.encode(...) returns (bytes memory)",
                    description: "ABI-encodes the given arguments.",
                },
            ),
            (
                "encodePacked",
                BuiltinDef {
                    signature: "abi.encodePacked(...) returns (bytes memory)",
                    description: "Performs packed encoding of the given arguments. Note: packed encoding can be ambiguous — avoid using with multiple dynamic types.",
                },
            ),
            (
                "encodeWithSelector",
                BuiltinDef {
                    signature: "abi.encodeWithSelector(bytes4 selector, ...) returns (bytes memory)",
                    description: "ABI-encodes the given arguments starting from the second, prepending the given four-byte selector.",
                },
            ),
            (
                "encodeWithSignature",
                BuiltinDef {
                    signature: "abi.encodeWithSignature(string memory signature, ...) returns (bytes memory)",
                    description: "Equivalent to `abi.encodeWithSelector(bytes4(keccak256(bytes(signature))), ...)`.",
                },
            ),
            (
                "encodeCall",
                BuiltinDef {
                    signature: "abi.encodeCall(functionPointer, (arg1, arg2, ...)) returns (bytes memory)",
                    description: "Type-safe ABI-encoding of a function call. Arguments are checked against the function signature at compile time.",
                },
            ),
            (
                "decode",
                BuiltinDef {
                    signature: "abi.decode(bytes memory data, (T1, T2, ...)) returns (T1, T2, ...)",
                    description: "ABI-decodes the given data. Types are given in parentheses as second argument, e.g. `(uint256, string)`.",
                },
            ),
        ],
    },
    // -- string --
    Namespace {
        name: "string",
        summary: BuiltinDef {
            signature: "string",
            description: "Built-in string type with utility functions.",
        },
        members: &[(
            "concat",
            BuiltinDef {
                signature: "string.concat(string memory, ...) returns (string memory)",
                description: "Concatenate an arbitrary number of string values.",
            },
        )],
    },
    // -- bytes --
    Namespace {
        name: "bytes",
        summary: BuiltinDef {
            signature: "bytes",
            description: "Built-in dynamic bytes type with utility functions.",
        },
        members: &[(
            "concat",
            BuiltinDef {
                signature: "bytes.concat(bytes memory, ...) returns (bytes memory)",
                description: "Concatenate an arbitrary number of bytes values.",
            },
        )],
    },
];

// ---------------------------------------------------------------------------
// Yul / inline assembly built-in functions
// ---------------------------------------------------------------------------

static YUL_BUILTINS: &[(&str, BuiltinDef)] = &[
    // -- Arithmetic --
    ("add", BuiltinDef { signature: "add(x, y) -> result", description: "Addition: `x + y`." }),
    ("sub", BuiltinDef { signature: "sub(x, y) -> result", description: "Subtraction: `x - y`." }),
    ("mul", BuiltinDef { signature: "mul(x, y) -> result", description: "Multiplication: `x * y`." }),
    ("div", BuiltinDef { signature: "div(x, y) -> result", description: "Unsigned integer division: `x / y`. Returns 0 if `y == 0`." }),
    ("sdiv", BuiltinDef { signature: "sdiv(x, y) -> result", description: "Signed integer division: `x / y` (two's complement). Returns 0 if `y == 0`." }),
    ("mod", BuiltinDef { signature: "mod(x, y) -> result", description: "Unsigned modulo: `x % y`. Returns 0 if `y == 0`." }),
    ("smod", BuiltinDef { signature: "smod(x, y) -> result", description: "Signed modulo (two's complement). Returns 0 if `y == 0`." }),
    ("exp", BuiltinDef { signature: "exp(x, y) -> result", description: "Exponentiation: `x ** y`." }),
    ("signextend", BuiltinDef { signature: "signextend(b, x) -> result", description: "Sign-extend `x` from `(b + 1) * 8` bits to 256 bits." }),
    ("addmod", BuiltinDef { signature: "addmod(x, y, m) -> result", description: "Compute `(x + y) % m` with arbitrary precision arithmetic. Returns 0 if `m == 0`." }),
    ("mulmod", BuiltinDef { signature: "mulmod(x, y, m) -> result", description: "Compute `(x * y) % m` with arbitrary precision arithmetic. Returns 0 if `m == 0`." }),
    // -- Comparison --
    ("lt", BuiltinDef { signature: "lt(x, y) -> result", description: "Unsigned less-than: 1 if `x < y`, 0 otherwise." }),
    ("gt", BuiltinDef { signature: "gt(x, y) -> result", description: "Unsigned greater-than: 1 if `x > y`, 0 otherwise." }),
    ("slt", BuiltinDef { signature: "slt(x, y) -> result", description: "Signed less-than (two's complement): 1 if `x < y`, 0 otherwise." }),
    ("sgt", BuiltinDef { signature: "sgt(x, y) -> result", description: "Signed greater-than (two's complement): 1 if `x > y`, 0 otherwise." }),
    ("eq", BuiltinDef { signature: "eq(x, y) -> result", description: "Equality: 1 if `x == y`, 0 otherwise." }),
    ("iszero", BuiltinDef { signature: "iszero(x) -> result", description: "1 if `x == 0`, 0 otherwise." }),
    // -- Bitwise --
    ("and", BuiltinDef { signature: "and(x, y) -> result", description: "Bitwise AND of `x` and `y`." }),
    ("or", BuiltinDef { signature: "or(x, y) -> result", description: "Bitwise OR of `x` and `y`." }),
    ("xor", BuiltinDef { signature: "xor(x, y) -> result", description: "Bitwise XOR of `x` and `y`." }),
    ("not", BuiltinDef { signature: "not(x) -> result", description: "Bitwise NOT of `x` (every bit is flipped)." }),
    ("byte", BuiltinDef { signature: "byte(n, x) -> result", description: "The `n`-th byte of `x` (0-indexed from the most significant byte). Returns 0 if `n >= 32`." }),
    ("shl", BuiltinDef { signature: "shl(shift, value) -> result", description: "Logical shift left: `value << shift`." }),
    ("shr", BuiltinDef { signature: "shr(shift, value) -> result", description: "Logical shift right: `value >> shift`." }),
    ("sar", BuiltinDef { signature: "sar(shift, value) -> result", description: "Arithmetic shift right (sign-extending): `value >> shift`." }),
    // -- Memory --
    ("mload", BuiltinDef { signature: "mload(offset) -> value", description: "Load 32 bytes from memory at the given byte offset." }),
    ("mstore", BuiltinDef { signature: "mstore(offset, value)", description: "Store 32 bytes to memory at the given byte offset." }),
    ("mstore8", BuiltinDef { signature: "mstore8(offset, value)", description: "Store a single byte to memory at the given byte offset (least significant byte of `value`)." }),
    ("msize", BuiltinDef { signature: "msize() -> size", description: "Current size of memory in bytes (highest accessed offset rounded up to 32)." }),
    ("mcopy", BuiltinDef { signature: "mcopy(dst, src, length)", description: "Copy `length` bytes from memory position `src` to `dst` (EIP-5656)." }),
    // -- Storage --
    ("sload", BuiltinDef { signature: "sload(slot) -> value", description: "Load a 32-byte word from storage at the given slot." }),
    ("sstore", BuiltinDef { signature: "sstore(slot, value)", description: "Store a 32-byte word to storage at the given slot." }),
    ("tload", BuiltinDef { signature: "tload(slot) -> value", description: "Load from transient storage at the given slot (EIP-1153). Value is cleared at the end of the transaction." }),
    ("tstore", BuiltinDef { signature: "tstore(slot, value)", description: "Store to transient storage at the given slot (EIP-1153). Value is cleared at the end of the transaction." }),
    // -- Calldata --
    ("calldataload", BuiltinDef { signature: "calldataload(offset) -> value", description: "Load 32 bytes of calldata starting from the given byte offset." }),
    ("calldatasize", BuiltinDef { signature: "calldatasize() -> size", description: "Size of the calldata in bytes." }),
    ("calldatacopy", BuiltinDef { signature: "calldatacopy(destOffset, offset, length)", description: "Copy `length` bytes of calldata starting at `offset` to memory at `destOffset`." }),
    // -- Returndata --
    ("returndatasize", BuiltinDef { signature: "returndatasize() -> size", description: "Size of the return data from the last external call, in bytes." }),
    ("returndatacopy", BuiltinDef { signature: "returndatacopy(destOffset, offset, length)", description: "Copy `length` bytes from return data at `offset` to memory at `destOffset`." }),
    // -- Code --
    ("codesize", BuiltinDef { signature: "codesize() -> size", description: "Size of the code of the current contract in bytes." }),
    ("codecopy", BuiltinDef { signature: "codecopy(destOffset, offset, length)", description: "Copy `length` bytes of the current contract's code at `offset` to memory at `destOffset`." }),
    ("extcodesize", BuiltinDef { signature: "extcodesize(addr) -> size", description: "Size of the code at address `addr`, in bytes." }),
    ("extcodecopy", BuiltinDef { signature: "extcodecopy(addr, destOffset, offset, length)", description: "Copy `length` bytes of the code at address `addr` at `offset` to memory at `destOffset`." }),
    ("extcodehash", BuiltinDef { signature: "extcodehash(addr) -> hash", description: "Keccak-256 hash of the code at address `addr` (EIP-1052)." }),
    // -- Hashing --
    ("keccak256", BuiltinDef { signature: "keccak256(offset, length) -> hash", description: "Compute the Keccak-256 hash of `length` bytes of memory starting at `offset`." }),
    // -- Environment --
    ("address", BuiltinDef { signature: "address() -> addr", description: "Address of the current contract." }),
    ("balance", BuiltinDef { signature: "balance(addr) -> wei", description: "Wei balance of address `addr`." }),
    ("selfbalance", BuiltinDef { signature: "selfbalance() -> wei", description: "Wei balance of the current contract. Equivalent to `balance(address())` but cheaper." }),
    ("caller", BuiltinDef { signature: "caller() -> addr", description: "Message caller (`msg.sender`)." }),
    ("callvalue", BuiltinDef { signature: "callvalue() -> wei", description: "Wei sent with the current call (`msg.value`)." }),
    ("origin", BuiltinDef { signature: "origin() -> addr", description: "Transaction sender (`tx.origin`)." }),
    ("gasprice", BuiltinDef { signature: "gasprice() -> price", description: "Gas price of the transaction." }),
    ("gas", BuiltinDef { signature: "gas() -> remaining", description: "Remaining gas." }),
    ("coinbase", BuiltinDef { signature: "coinbase() -> addr", description: "Block proposer/miner address." }),
    ("timestamp", BuiltinDef { signature: "timestamp() -> ts", description: "Current block timestamp (seconds since epoch)." }),
    ("number", BuiltinDef { signature: "number() -> blockNumber", description: "Current block number." }),
    ("difficulty", BuiltinDef { signature: "difficulty() -> diff", description: "Current block difficulty. Post-merge: alias for `prevrandao()`." }),
    ("prevrandao", BuiltinDef { signature: "prevrandao() -> rand", description: "Random value provided by the beacon chain (EIP-4399). Replaces `difficulty()` post-merge." }),
    ("gaslimit", BuiltinDef { signature: "gaslimit() -> limit", description: "Current block gas limit." }),
    ("chainid", BuiltinDef { signature: "chainid() -> id", description: "Current chain ID (EIP-1344)." }),
    ("basefee", BuiltinDef { signature: "basefee() -> fee", description: "Current block's base fee (EIP-3198)." }),
    ("blobbasefee", BuiltinDef { signature: "blobbasefee() -> fee", description: "Current block's blob base fee (EIP-7516)." }),
    ("blobhash", BuiltinDef { signature: "blobhash(index) -> hash", description: "Versioned hash of blob at `index` in the current transaction (EIP-4844)." }),
    ("blockhash", BuiltinDef { signature: "blockhash(blockNumber) -> hash", description: "Hash of the given block (only for the 256 most recent blocks, excluding current)." }),
    // -- Calls --
    ("call", BuiltinDef { signature: "call(gas, addr, value, argsOffset, argsLength, retOffset, retLength) -> success", description: "Call contract at `addr` with `value` wei. Writes return data to memory at `retOffset`. Returns 1 on success, 0 on revert." }),
    ("callcode", BuiltinDef { signature: "callcode(gas, addr, value, argsOffset, argsLength, retOffset, retLength) -> success", description: "Like `call`, but uses the code at `addr` with the current contract's storage. Deprecated — use `delegatecall` instead." }),
    ("delegatecall", BuiltinDef { signature: "delegatecall(gas, addr, argsOffset, argsLength, retOffset, retLength) -> success", description: "Call code at `addr` in the context of the current contract (same `msg.sender`, `msg.value`, storage). Returns 1 on success." }),
    ("staticcall", BuiltinDef { signature: "staticcall(gas, addr, argsOffset, argsLength, retOffset, retLength) -> success", description: "Like `call` but disallows state modifications. Returns 1 on success." }),
    // -- Create --
    ("create", BuiltinDef { signature: "create(value, offset, length) -> addr", description: "Create a new contract with `length` bytes of code from memory at `offset`, sending `value` wei. Returns the new address, or 0 on failure." }),
    ("create2", BuiltinDef { signature: "create2(value, offset, length, salt) -> addr", description: "Create a new contract at a deterministic address derived from `salt`, creation code, and the sender. Returns the new address, or 0 on failure." }),
    // -- Return / revert --
    ("return", BuiltinDef { signature: "return(offset, length)", description: "End execution, returning `length` bytes of data from memory at `offset`." }),
    ("revert", BuiltinDef { signature: "revert(offset, length)", description: "End execution, reverting state changes. Returns `length` bytes of data from memory at `offset`." }),
    ("stop", BuiltinDef { signature: "stop()", description: "Stop execution (equivalent to `return(0, 0)`)." }),
    ("invalid", BuiltinDef { signature: "invalid()", description: "End execution with the `INVALID` opcode. Consumes all remaining gas." }),
    ("selfdestruct", BuiltinDef { signature: "selfdestruct(addr)", description: "Deprecated (EIP-6049). Send all Ether to `addr` and mark the contract for destruction." }),
    // -- Logging --
    ("log0", BuiltinDef { signature: "log0(offset, length)", description: "Emit a log with `length` bytes of data from memory at `offset` and 0 topics." }),
    ("log1", BuiltinDef { signature: "log1(offset, length, topic1)", description: "Emit a log with `length` bytes of data and 1 topic." }),
    ("log2", BuiltinDef { signature: "log2(offset, length, topic1, topic2)", description: "Emit a log with `length` bytes of data and 2 topics." }),
    ("log3", BuiltinDef { signature: "log3(offset, length, topic1, topic2, topic3)", description: "Emit a log with `length` bytes of data and 3 topics." }),
    ("log4", BuiltinDef { signature: "log4(offset, length, topic1, topic2, topic3, topic4)", description: "Emit a log with `length` bytes of data and 4 topics." }),
    // -- Misc --
    ("pop", BuiltinDef { signature: "pop(x)", description: "Discard the top stack element." }),
    ("datasize", BuiltinDef { signature: "datasize(name) -> size", description: "Size of the data section named `name`." }),
    ("dataoffset", BuiltinDef { signature: "dataoffset(name) -> offset", description: "Offset of the data section named `name` within the current subassembly." }),
    ("datacopy", BuiltinDef { signature: "datacopy(destOffset, offset, length)", description: "Copy `length` bytes from a data section at `offset` to memory at `destOffset`." }),
];

// ---------------------------------------------------------------------------
// Lookup functions
// ---------------------------------------------------------------------------

/// Look up a global Solidity function by name (e.g. `"keccak256"`, `"require"`).
pub fn lookup_solidity_global(name: &str) -> Option<&'static BuiltinDef> {
    SOLIDITY_GLOBALS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, def)| def)
}

/// Look up a Solidity namespace by name, returning a summary (e.g. hovering on `"msg"`).
pub fn lookup_solidity_namespace(name: &str) -> Option<&'static BuiltinDef> {
    NAMESPACES
        .iter()
        .find(|ns| ns.name == name)
        .map(|ns| &ns.summary)
}

/// Look up a member of a Solidity namespace (e.g. `"msg"`, `"sender"`).
pub fn lookup_solidity_member(namespace: &str, member: &str) -> Option<&'static BuiltinDef> {
    let ns = NAMESPACES.iter().find(|ns| ns.name == namespace)?;
    ns.members
        .iter()
        .find(|(n, _)| *n == member)
        .map(|(_, def)| def)
}

/// Look up a Yul built-in function by name (e.g. `"mload"`, `"sstore"`).
pub fn lookup_yul_builtin(name: &str) -> Option<&'static BuiltinDef> {
    YUL_BUILTINS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, def)| def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_solidity_global_keccak256() {
        let def = lookup_solidity_global("keccak256").unwrap();
        assert!(def.signature.contains("bytes32"));
        assert!(def.description.contains("Keccak-256"));
    }

    #[test]
    fn test_lookup_solidity_global_require() {
        let def = lookup_solidity_global("require").unwrap();
        assert!(def.signature.contains("bool"));
    }

    #[test]
    fn test_lookup_solidity_global_nonexistent() {
        assert!(lookup_solidity_global("nonexistent").is_none());
    }

    #[test]
    fn test_lookup_solidity_namespace_msg() {
        let def = lookup_solidity_namespace("msg").unwrap();
        assert!(def.description.contains("sender"));
        assert!(def.description.contains("value"));
    }

    #[test]
    fn test_lookup_solidity_namespace_nonexistent() {
        assert!(lookup_solidity_namespace("foo").is_none());
    }

    #[test]
    fn test_lookup_solidity_member_msg_sender() {
        let def = lookup_solidity_member("msg", "sender").unwrap();
        assert!(def.signature.contains("address"));
    }

    #[test]
    fn test_lookup_solidity_member_abi_encode() {
        let def = lookup_solidity_member("abi", "encode").unwrap();
        assert!(def.signature.contains("bytes memory"));
    }

    #[test]
    fn test_lookup_solidity_member_block_timestamp() {
        let def = lookup_solidity_member("block", "timestamp").unwrap();
        assert!(def.signature.contains("uint256"));
    }

    #[test]
    fn test_lookup_solidity_member_nonexistent() {
        assert!(lookup_solidity_member("msg", "nonexistent").is_none());
        assert!(lookup_solidity_member("nonexistent", "sender").is_none());
    }

    #[test]
    fn test_lookup_yul_builtin_mload() {
        let def = lookup_yul_builtin("mload").unwrap();
        assert!(def.signature.contains("offset"));
    }

    #[test]
    fn test_lookup_yul_builtin_sstore() {
        let def = lookup_yul_builtin("sstore").unwrap();
        assert!(def.signature.contains("slot"));
    }

    #[test]
    fn test_lookup_yul_builtin_nonexistent() {
        assert!(lookup_yul_builtin("nonexistent").is_none());
    }
}
