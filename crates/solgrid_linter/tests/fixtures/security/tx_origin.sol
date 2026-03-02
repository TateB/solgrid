// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract TxOriginTest {
    address owner;

    function badAuth() public {
        require(tx.origin == owner, "not owner");
    }

    function goodAuth() public {
        require(msg.sender == owner, "not owner");
    }
}
