// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

/// @title Clean
/// @author test
contract Clean {
    uint256 public balance;

    /// @notice Get the balance
    /// @return The balance value
    function getBalance() external view returns (uint256) {
        return balance;
    }
}
