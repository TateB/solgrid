// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title ComplexFunctions
/// @author test
/// @notice Contract with various function signatures for conformance testing
contract ComplexFunctions is Ownable, ReentrancyGuard {
    uint256 public constant MAX_UINT = type(uint256).max;
    uint256 public immutable deploymentTime;

    mapping(address => mapping(address => uint256)) private _allowances;

    constructor() Ownable(msg.sender) {
        deploymentTime = block.timestamp;
    }

    function multipleParams(address to, uint256 amount, bytes calldata data, bool flag, string calldata name) external nonReentrant returns (bool success, uint256 remaining) {
        require(to != address(0), "Invalid address");
        require(amount > 0, "Amount must be positive");
        success = true;
        remaining = amount;
    }

    function noParams() external pure returns (uint256) {
        return 42;
    }

    function viewFunction(address user, uint256 index) external view returns (uint256 balance, bool active) {
        balance = _allowances[user][address(this)];
        active = balance > 0;
    }

    function internalHelper(uint256 a, uint256 b) internal pure returns (uint256) {
        unchecked {
            return a + b;
        }
    }

    receive() external payable {}
    fallback() external payable {}
}
