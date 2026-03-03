// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract ControlFlow {
    uint256[] public values;
    mapping(address => bool) public allowed;

    function processValues(uint256[] calldata input) external {
        for (uint256 i = 0; i < input.length; i++) {
            if (input[i] == 0) {
                continue;
            } else if (input[i] > 1000) {
                revert("Value too large");
            } else {
                values.push(input[i]);
            }
        }
    }

    function complexLogic(uint256 a, uint256 b) external pure returns (uint256 result) {
        if (a > b) {
            result = a - b;
        } else if (a == b) {
            result = 0;
        } else {
            result = b - a;
        }

        while (result > 100) {
            result = result / 2;
        }

        result = a > 0 ? result * a : result;
    }

    function simpleLoop(uint256 count) external {
        uint256 total = 0;
        for (uint256 i = 0; i < count; i++) {
            total += i;
        }
        values.push(total);
    }
}
