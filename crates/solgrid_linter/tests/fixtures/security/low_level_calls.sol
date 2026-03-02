// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract LowLevelCallsTest {
    function unsafeCall(address target) public {
        target.call("");
    }

    function unsafeDelegatecall(address target) public {
        target.delegatecall("");
    }

    function unsafeStaticcall(address target) public {
        target.staticcall("");
    }
}
