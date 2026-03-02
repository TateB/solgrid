// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ConstNameTest {
    uint256 constant MAX_SUPPLY = 1000;
    uint256 constant badName = 500;
    uint256 immutable GOOD_IMMUTABLE;
    uint256 immutable badImmutable;

    constructor() {
        GOOD_IMMUTABLE = 100;
        badImmutable = 200;
    }
}
