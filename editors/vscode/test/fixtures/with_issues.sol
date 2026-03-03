// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract diagnostics_test {
    uint public balance;
    address owner;

    function badAuth() public {
        require(tx.origin == owner, "not owner");
    }
}
