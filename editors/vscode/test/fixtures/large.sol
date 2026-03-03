// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

/// @title LargeContract
/// @author test
contract LargeContract {
    uint256 public a;
    uint256 public b;
    uint256 public c;
    uint256 public d;
    uint256 public e;

    event ValueChanged(uint256 indexed oldValue, uint256 indexed newValue);
    event Transfer(address indexed from, address indexed to, uint256 value);

    error InsufficientBalance(uint256 requested, uint256 available);
    error Unauthorized();

    modifier onlyPositive(uint256 _value) {
        require(_value > 0, "Must be positive");
        _;
    }

    /// @notice Set value a
    /// @param _a The new value
    function setA(uint256 _a) external onlyPositive(_a) {
        uint256 old = a;
        a = _a;
        emit ValueChanged(old, _a);
    }

    /// @notice Set value b
    /// @param _b The new value
    function setB(uint256 _b) external onlyPositive(_b) {
        uint256 old = b;
        b = _b;
        emit ValueChanged(old, _b);
    }

    /// @notice Set value c
    /// @param _c The new value
    function setC(uint256 _c) external onlyPositive(_c) {
        uint256 old = c;
        c = _c;
        emit ValueChanged(old, _c);
    }

    /// @notice Set value d
    /// @param _d The new value
    function setD(uint256 _d) external onlyPositive(_d) {
        uint256 old = d;
        d = _d;
        emit ValueChanged(old, _d);
    }

    /// @notice Set value e
    /// @param _e The new value
    function setE(uint256 _e) external onlyPositive(_e) {
        uint256 old = e;
        e = _e;
        emit ValueChanged(old, _e);
    }

    /// @notice Get the sum of all values
    /// @return The total sum
    function sum() external view returns (uint256) {
        return a + b + c + d + e;
    }

    /// @notice Check if all values are set
    /// @return True if all values are non-zero
    function allSet() external view returns (bool) {
        return a > 0 && b > 0 && c > 0 && d > 0 && e > 0;
    }

    /// @notice Reset all values to zero
    function resetAll() external {
        a = 0;
        b = 0;
        c = 0;
        d = 0;
        e = 0;
    }
}
