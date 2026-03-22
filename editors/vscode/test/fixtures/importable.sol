// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Importable {
    uint256 public value;

    function getValue() external view returns (uint256) {
        return value;
    }
}

enum Status {
    Active,
    Inactive
}
