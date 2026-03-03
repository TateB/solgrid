// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title Comments Test
/// @notice Tests that comments are preserved during formatting
contract Comments {
    // Single-line comment
    uint256 public value; // Trailing comment

    /* Multi-line comment
       spanning multiple lines */
    address public owner;

    /**
     * @notice NatSpec function documentation
     * @param _value The new value to set
     * @return The old value
     */
    function setValue(uint256 _value) external returns (uint256) {
        uint256 old = value; // save old value
        value = _value; /* update */
        return old;
    }

    // Another single-line comment before function
    function getValue()
        external
        view
        returns (
            // This is an unusual comment placement
            uint256
        )
    {
        return value;
    }
}
